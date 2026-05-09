use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PublishRateLimitBatch;
use monochange_core::PublishRateLimitReport;
use monochange_core::RateLimitConfidence;
use monochange_core::RateLimitEvidence;
use monochange_core::RateLimitEvidenceKind;
use monochange_core::RateLimitOperation;
use monochange_core::RegistryKind;
use monochange_core::RegistryRateLimitPolicy;
use monochange_core::RegistryRateLimitWindowPlan;
use monochange_core::WorkspaceConfiguration;
use monochange_publish::filter_pending_publish_requests;

use crate::PreparedRelease;
use crate::discover_workspace;
use crate::package_publish;

const CRATES_IO_SOURCE: &str = "https://github.com/rust-lang/crates.io";
const NPM_TRUST_DOCS: &str = "https://docs.npmjs.com/trusted-publishers";
const PUB_DEV_AUTOMATED_PUBLISHING: &str = "https://dart.dev/tools/pub/automated-publishing";
const JSR_PUBLISHING_DOCS: &str = "https://jsr.io/docs/publishing-packages";
const PYPI_TRUSTED_PUBLISHERS_DOCS: &str = "https://docs.pypi.org/trusted-publishers/";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum PublishRateLimitMode {
	Placeholder,
	Publish,
}

impl PublishRateLimitMode {
	#[must_use]
	pub(crate) fn operation(self) -> RateLimitOperation {
		match self {
			Self::Placeholder => RateLimitOperation::PlaceholderPublish,
			Self::Publish => RateLimitOperation::Publish,
		}
	}

	#[must_use]
	fn description(self) -> &'static str {
		match self {
			Self::Placeholder => "placeholder publish",
			Self::Publish => "publish",
		}
	}
}

pub(crate) fn plan_publish_rate_limits(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	selected_packages: &BTreeSet<String>,
	mode: PublishRateLimitMode,
	dry_run: bool,
) -> MonochangeResult<PublishRateLimitReport> {
	let discovery = discover_workspace(root)?;
	let packages = &discovery.packages;
	let requests = if mode == PublishRateLimitMode::Placeholder {
		build_placeholder_plan_requests(root, configuration, packages, selected_packages)?
	} else {
		build_release_plan_requests(
			root,
			configuration,
			prepared_release,
			packages,
			selected_packages,
		)?
	};
	Ok(plan_publish_rate_limits_for_requests(
		&requests,
		mode.operation(),
		dry_run,
	))
}

fn build_placeholder_plan_requests(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	packages: &[monochange_core::PackageRecord],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<package_publish::PublishRequest>> {
	let requests = package_publish::build_placeholder_requests(
		root,
		configuration,
		packages,
		selected_packages,
	)?;
	filter_pending_publish_requests(&requests)
}

fn build_release_plan_requests(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	packages: &[monochange_core::PackageRecord],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<package_publish::PublishRequest>> {
	let publications = package_publish::release_record_package_publications_from_prepared_or_head(
		root,
		prepared_release,
	)?;
	let requests = package_publish::build_release_requests(
		configuration,
		packages,
		&publications,
		selected_packages,
	)?;
	filter_pending_publish_requests(&requests)
}

pub(crate) fn plan_publish_rate_limits_for_requests(
	requests: &[package_publish::PublishRequest],
	operation: RateLimitOperation,
	dry_run: bool,
) -> PublishRateLimitReport {
	let mut requests_by_registry =
		BTreeMap::<RegistryKind, Vec<&package_publish::PublishRequest>>::new();
	for request in requests {
		if request.mode == monochange_core::PublishMode::External {
			continue;
		}
		requests_by_registry
			.entry(request.registry)
			.or_default()
			.push(request);
	}

	let policies = policies_for_operation(operation)
		.into_iter()
		.map(|policy| (policy.registry, policy))
		.collect::<BTreeMap<_, _>>();

	let mut windows = Vec::new();
	let mut batches = Vec::new();

	for (registry, requests) in requests_by_registry {
		if let Some(policy) = policies.get(&registry) {
			let window = plan_window(policy, requests.len());
			batches.extend(plan_batches(policy, &requests));
			windows.push(window);
		}
	}

	windows.sort_by(|left, right| {
		left.registry
			.cmp(&right.registry)
			.then(left.operation.cmp(&right.operation))
	});
	batches.sort_by(|left, right| {
		left.registry
			.cmp(&right.registry)
			.then(left.batch_index.cmp(&right.batch_index))
	});

	let warnings = windows
		.iter()
		.filter(|window| !window.fits_single_window)
		.map(|window| {
			format!(
				"{} {} {} operations need {} batches under the current {} window",
				window.pending,
				window.registry,
				window.operation,
				window.batches_required,
				render_window(window.window_seconds)
			)
		})
		.collect();

	PublishRateLimitReport {
		dry_run,
		windows,
		batches,
		warnings,
	}
}

pub(crate) fn enforce_publish_rate_limits(
	configuration: &WorkspaceConfiguration,
	report: &PublishRateLimitReport,
	mode: PublishRateLimitMode,
) -> MonochangeResult<()> {
	let enforced_packages = report
		.batches
		.iter()
		.flat_map(|batch| batch.packages.iter())
		.any(|package| {
			configuration
				.package_by_id(package)
				.is_some_and(|definition| definition.publish.rate_limits.enforce)
		});
	if !enforced_packages {
		return Ok(());
	}

	let blocked = report
		.windows
		.iter()
		.filter(|window| !window.fits_single_window)
		.collect::<Vec<_>>();
	if blocked.is_empty() {
		return Ok(());
	}

	let details = blocked
		.into_iter()
		.map(|window| {
			format!(
				"{} {} {} packages={} batches={} window={}",
				mode.description(),
				window.registry,
				window.operation,
				window.pending,
				window.batches_required,
				render_window(window.window_seconds)
			)
		})
		.collect::<Vec<_>>()
		.join("; ");

	Err(MonochangeError::Config(format!(
		"configured publish rate-limit enforcement blocked this run: {details}; use `mc publish-plan` to inspect batches or publish a filtered package subset"
	)))
}

fn plan_window(policy: &RegistryRateLimitPolicy, pending: usize) -> RegistryRateLimitWindowPlan {
	let batches_required = policy
		.limit
		.map_or(1, |limit| pending.div_ceil(limit as usize));
	let fits_single_window = policy.limit.is_none_or(|limit| pending <= limit as usize);

	RegistryRateLimitWindowPlan {
		registry: policy.registry,
		operation: policy.operation,
		limit: policy.limit,
		window_seconds: policy.window_seconds,
		pending,
		batches_required,
		fits_single_window,
		confidence: policy.confidence,
		notes: policy.notes.clone(),
		evidence: policy.evidence.clone(),
	}
}

fn plan_batches(
	policy: &RegistryRateLimitPolicy,
	requests: &[&package_publish::PublishRequest],
) -> Vec<PublishRateLimitBatch> {
	let chunk_size = policy
		.limit
		.map_or_else(|| requests.len().max(1), |limit| limit as usize);
	let total_batches = requests.len().div_ceil(chunk_size).max(1);

	requests
		.chunks(chunk_size)
		.enumerate()
		.map(|(index, _chunk)| {
			let included_requests = requests
				.iter()
				.take(((index + 1) * chunk_size).min(requests.len()));
			PublishRateLimitBatch {
				registry: policy.registry,
				operation: policy.operation,
				batch_index: index + 1,
				total_batches,
				packages: included_requests
					.map(|request| request.package_id.clone())
					.collect(),
				recommended_wait_seconds: if index == 0 {
					None
				} else {
					policy.window_seconds.map(|seconds| seconds * index as u64)
				},
			}
		})
		.collect()
}

pub(crate) fn render_window(window_seconds: Option<u64>) -> String {
	match window_seconds {
		Some(86_400) => "24h".to_string(),
		Some(seconds) => format!("{seconds}s"),
		None => "unknown window".to_string(),
	}
}

fn policies_for_operation(operation: RateLimitOperation) -> Vec<RegistryRateLimitPolicy> {
	registry_policies()
		.into_iter()
		.map(|mut policy| {
			policy.operation = operation;
			policy
		})
		.collect()
}

fn registry_policies() -> Vec<RegistryRateLimitPolicy> {
	vec![
		RegistryRateLimitPolicy {
			registry: RegistryKind::CratesIo,
			operation: RateLimitOperation::Publish,
			limit: Some(10),
			window_seconds: Some(60),
			confidence: RateLimitConfidence::High,
			notes: "crates.io source enforces 10 uploads per minute for existing crates".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "crates.io application source".to_string(),
				url: CRATES_IO_SOURCE.to_string(),
				kind: RateLimitEvidenceKind::SourceCode,
				notes: "upload endpoint rate limiting in server implementation".to_string(),
			}],
		},
		RegistryRateLimitPolicy {
			registry: RegistryKind::Npm,
			operation: RateLimitOperation::Publish,
			limit: None,
			window_seconds: None,
			confidence: RateLimitConfidence::Low,
			notes: "npm does not publish a precise package publish quota; use sequential CI publishing with retries".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "npm trusted publishing documentation".to_string(),
				url: NPM_TRUST_DOCS.to_string(),
				kind: RateLimitEvidenceKind::Official,
				notes: "official workflow guidance but no exact package publish quota".to_string(),
			}],
		},
		RegistryRateLimitPolicy {
			registry: RegistryKind::Jsr,
			operation: RateLimitOperation::Publish,
			limit: Some(20),
			window_seconds: Some(86_400),
			confidence: RateLimitConfidence::High,
			notes: "JSR documents a daily publish limit per package scope".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "JSR publishing docs".to_string(),
				url: JSR_PUBLISHING_DOCS.to_string(),
				kind: RateLimitEvidenceKind::Official,
				notes: "official JSR publishing limits documentation".to_string(),
			}],
		},
		RegistryRateLimitPolicy {
			registry: RegistryKind::PubDev,
			operation: RateLimitOperation::Publish,
			limit: Some(12),
			window_seconds: Some(86_400),
			confidence: RateLimitConfidence::Medium,
			notes: "pub.dev community guidance consistently cites 12 publishes per day for new versions".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "Dart automated publishing docs".to_string(),
				url: PUB_DEV_AUTOMATED_PUBLISHING.to_string(),
				kind: RateLimitEvidenceKind::Official,
				notes: "official automation docs; limit itself is enforced operationally but not clearly enumerated on this page".to_string(),
			}],
		},
		RegistryRateLimitPolicy {
			registry: RegistryKind::Pypi,
			operation: RateLimitOperation::Publish,
			limit: None,
			window_seconds: None,
			confidence: RateLimitConfidence::Low,
			notes: "PyPI does not publish a precise package publish quota; use sequential CI publishing with retries".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "PyPI trusted publishers documentation".to_string(),
				url: PYPI_TRUSTED_PUBLISHERS_DOCS.to_string(),
				kind: RateLimitEvidenceKind::Official,
				notes: "official trusted-publisher workflow guidance but no exact package publish quota".to_string(),
			}],
		},
		RegistryRateLimitPolicy {
			registry: RegistryKind::GoProxy,
			operation: RateLimitOperation::Publish,
			limit: None,
			window_seconds: None,
			confidence: RateLimitConfidence::Low,
			notes: "Go modules are published by pushing VCS tags; the public proxy does not document a precise publish quota".to_string(),
			evidence: vec![RateLimitEvidence {
				title: "Go module publishing reference".to_string(),
				url: "https://go.dev/ref/mod#publishing".to_string(),
				kind: RateLimitEvidenceKind::Official,
				notes: "official module publishing guidance documents tag-based publication".to_string(),
			}],
		},
	]
}

#[cfg(test)]
#[path = "__tests__/publish_rate_limits_tests.rs"]
mod tests;
