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

use crate::PreparedRelease;
use crate::discover_workspace;
use crate::package_publish;

const CRATES_IO_SOURCE: &str = "https://github.com/rust-lang/crates.io";
const NPM_TRUST_DOCS: &str = "https://docs.npmjs.com/trusted-publishers";
const PUB_DEV_AUTOMATED_PUBLISHING: &str = "https://dart.dev/tools/pub/automated-publishing";
const JSR_PUBLISHING_DOCS: &str = "https://jsr.io/docs/publishing-packages";

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
	let requests = match mode {
		PublishRateLimitMode::Placeholder => {
			package_publish::build_placeholder_requests(
				root,
				configuration,
				&discovery.packages,
				selected_packages,
			)?
		}
		PublishRateLimitMode::Publish => {
			let publications =
				package_publish::release_record_package_publications_from_prepared_or_head(
					root,
					prepared_release,
				)?;
			package_publish::build_release_requests(
				&discovery.packages,
				&publications,
				selected_packages,
			)?
		}
	};

	Ok(plan_publish_rate_limits_for_requests(
		&requests,
		mode.operation(),
		dry_run,
	))
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
		let Some(policy) = policies.get(&registry) else {
			continue;
		};
		let window = plan_window(policy, requests.len());
		batches.extend(plan_batches(policy, &requests));
		windows.push(window);
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
		.map(|(index, chunk)| {
			PublishRateLimitBatch {
				registry: policy.registry,
				operation: policy.operation,
				batch_index: index + 1,
				total_batches,
				packages: chunk
					.iter()
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
	]
}

#[cfg(test)]
mod tests {
	use std::fs;

	use monochange_core::PackagePublicationTarget;
	use monochange_core::PublishMode;
	use monochange_core::PublishRegistry;
	use monochange_core::TrustedPublishingSettings;
	use semver::Version;
	use tempfile::tempdir;

	use super::*;

	fn copy_fixture_dir(source: &Path, destination: &Path) {
		copy_fixture_entry(source, destination, source);
	}

	fn copy_fixture_entry(source: &Path, destination: &Path, current: &Path) {
		let metadata = fs::metadata(current)
			.unwrap_or_else(|error| panic!("fixture metadata {}: {error}", current.display()));
		let relative = current
			.strip_prefix(source)
			.unwrap_or_else(|error| panic!("strip fixture prefix: {error}"));
		let target = destination.join(relative);

		if metadata.is_dir() {
			fs::create_dir_all(&target)
				.unwrap_or_else(|error| panic!("create fixture dir {}: {error}", target.display()));
			for entry in fs::read_dir(current)
				.unwrap_or_else(|error| panic!("read fixture dir {}: {error}", current.display()))
			{
				let entry = entry.unwrap_or_else(|error| panic!("fixture dir entry: {error}"));
				copy_fixture_entry(source, destination, &entry.path());
			}
			return;
		}

		if let Some(parent) = target.parent() {
			fs::create_dir_all(parent).unwrap_or_else(|error| {
				panic!("create fixture parent {}: {error}", parent.display())
			});
		}
		fs::copy(current, &target)
			.unwrap_or_else(|error| panic!("copy fixture {}: {error}", current.display()));
	}

	#[test]
	fn plan_publish_rate_limits_summarizes_pending_publications_and_batches() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../fixtures/tests/publish-rate-limits/single-window/workspace");
		copy_fixture_dir(&fixture, tempdir.path());
		let configuration = crate::load_workspace_configuration(tempdir.path())
			.unwrap_or_else(|error| panic!("load config: {error}"));
		let prepared_release = PreparedRelease {
			plan: monochange_core::ReleasePlan {
				workspace_root: tempdir.path().to_path_buf(),
				decisions: Vec::new(),
				groups: Vec::new(),
				warnings: Vec::new(),
				unresolved_items: Vec::new(),
				compatibility_evidence: Vec::new(),
			},
			changeset_paths: Vec::new(),
			changesets: Vec::new(),
			released_packages: vec!["core".to_string(), "docs".to_string(), "web".to_string()],
			package_publications: vec![
				PackagePublicationTarget {
					package: "core".to_string(),
					ecosystem: monochange_core::Ecosystem::Cargo,
					registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
					version: Version::new(1, 0, 0).to_string(),
					mode: PublishMode::Builtin,
					trusted_publishing: TrustedPublishingSettings::default(),
				},
				PackagePublicationTarget {
					package: "docs".to_string(),
					ecosystem: monochange_core::Ecosystem::Npm,
					registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
					version: Version::new(1, 0, 0).to_string(),
					mode: PublishMode::Builtin,
					trusted_publishing: TrustedPublishingSettings::default(),
				},
				PackagePublicationTarget {
					package: "web".to_string(),
					ecosystem: monochange_core::Ecosystem::Npm,
					registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
					version: Version::new(1, 0, 0).to_string(),
					mode: PublishMode::Builtin,
					trusted_publishing: TrustedPublishingSettings::default(),
				},
			],
			version: None,
			group_version: None,
			release_targets: Vec::new(),
			changed_files: Vec::new(),
			changelogs: Vec::new(),
			updated_changelogs: Vec::new(),
			deleted_changesets: Vec::new(),
			dry_run: true,
		};

		let report = plan_publish_rate_limits(
			tempdir.path(),
			&configuration,
			Some(&prepared_release),
			&BTreeSet::new(),
			PublishRateLimitMode::Publish,
			true,
		)
		.unwrap_or_else(|error| panic!("plan rate limits: {error}"));

		assert_eq!(report.windows.len(), 2);
		assert!(report.warnings.is_empty());
		assert!(report.batches.iter().any(|batch| {
			batch.registry == RegistryKind::Npm
				&& batch.packages == vec!["docs".to_string(), "web".to_string()]
		}));
	}

	#[test]
	fn plan_publish_rate_limits_for_requests_groups_multiple_packages_into_one_batch_when_limit_is_unbounded()
	 {
		let requests = vec![
			package_publish::PublishRequest {
				package_id: "docs".to_string(),
				package_name: "docs".to_string(),
				ecosystem: monochange_core::Ecosystem::Npm,
				manifest_path: Path::new("packages/docs/package.json").to_path_buf(),
				package_root: Path::new("packages/docs").to_path_buf(),
				registry: RegistryKind::Npm,
				package_manager: Some("pnpm".to_string()),
				mode: PublishMode::Builtin,
				version: Version::new(1, 0, 0).to_string(),
				trusted_publishing: TrustedPublishingSettings::default(),
				placeholder_readme: String::new(),
			},
			package_publish::PublishRequest {
				package_id: "web".to_string(),
				package_name: "web".to_string(),
				ecosystem: monochange_core::Ecosystem::Npm,
				manifest_path: Path::new("packages/web/package.json").to_path_buf(),
				package_root: Path::new("packages/web").to_path_buf(),
				registry: RegistryKind::Npm,
				package_manager: Some("pnpm".to_string()),
				mode: PublishMode::Builtin,
				version: Version::new(1, 0, 0).to_string(),
				trusted_publishing: TrustedPublishingSettings::default(),
				placeholder_readme: String::new(),
			},
		];

		let report =
			plan_publish_rate_limits_for_requests(&requests, RateLimitOperation::Publish, true);

		assert_eq!(report.batches.len(), 1);
		assert_eq!(
			report.batches[0].packages,
			vec!["docs".to_string(), "web".to_string()]
		);
	}

	#[test]
	fn plan_window_flags_multiple_batches_when_limit_is_exceeded() {
		let policy = RegistryRateLimitPolicy {
			registry: RegistryKind::PubDev,
			operation: RateLimitOperation::Publish,
			limit: Some(12),
			window_seconds: Some(86_400),
			confidence: RateLimitConfidence::Medium,
			notes: "pub.dev limit".to_string(),
			evidence: Vec::new(),
		};

		let window = plan_window(&policy, 25);

		assert_eq!(window.batches_required, 3);
		assert!(!window.fits_single_window);
	}

	#[test]
	fn enforce_publish_rate_limits_blocks_multi_batch_runs_when_enabled() {
		let requests = (0..13)
			.map(|index| {
				package_publish::PublishRequest {
					package_id: format!("pkg-{index}"),
					package_name: format!("pkg-{index}"),
					ecosystem: monochange_core::Ecosystem::Dart,
					manifest_path: Path::new("pkg-a/pubspec.yaml").to_path_buf(),
					package_root: Path::new("pkg-a").to_path_buf(),
					registry: RegistryKind::PubDev,
					package_manager: None,
					mode: PublishMode::Builtin,
					version: Version::new(1, 0, 0).to_string(),
					trusted_publishing: TrustedPublishingSettings::default(),
					placeholder_readme: String::new(),
				}
			})
			.collect::<Vec<_>>();
		let report =
			plan_publish_rate_limits_for_requests(&requests, RateLimitOperation::Publish, true);

		let configuration = WorkspaceConfiguration {
			root_path: Path::new(".").to_path_buf(),
			defaults: monochange_core::WorkspaceDefaults::default(),
			release_notes: monochange_core::ReleaseNotesSettings::default(),
			packages: (0..13)
				.map(|index| {
					monochange_core::PackageDefinition {
						id: format!("pkg-{index}"),
						path: Path::new("pkg-a").to_path_buf(),
						package_type: monochange_core::PackageType::Dart,
						changelog: None,
						extra_changelog_sections: Vec::new(),
						empty_update_message: None,
						release_title: None,
						changelog_version_title: None,
						versioned_files: Vec::new(),
						ignore_ecosystem_versioned_files: false,
						ignored_paths: Vec::new(),
						additional_paths: Vec::new(),
						tag: false,
						release: false,
						version_format: monochange_core::VersionFormat::default(),
						publish: monochange_core::PublishSettings {
							rate_limits: monochange_core::PublishRateLimitSettings {
								enforce: true,
							},
							..monochange_core::PublishSettings::default()
						},
					}
				})
				.collect(),
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
			cargo: monochange_core::EcosystemSettings::default(),
			npm: monochange_core::EcosystemSettings::default(),
			deno: monochange_core::EcosystemSettings::default(),
			dart: monochange_core::EcosystemSettings::default(),
		};
		let error =
			enforce_publish_rate_limits(&configuration, &report, PublishRateLimitMode::Publish)
				.unwrap_err();
		assert!(error.to_string().contains("blocked this run"));
	}
}
