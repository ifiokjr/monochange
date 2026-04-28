use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ReleaseRecordDiscovery;
use monochange_core::WorkspaceConfiguration;
use serde::Deserialize;
use serde::Serialize;

use crate::OutputFormat;
use crate::PreparedRelease;
use crate::discover_release_record;
use crate::package_publish;

const PUBLISH_READINESS_KIND: &str = "monochange.publishReadiness";
const PUBLISH_READINESS_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PublishReadinessOptions {
	pub from: String,
	pub selected_packages: BTreeSet<String>,
	pub format: OutputFormat,
	pub output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublishReadinessGlobalStatus {
	Ready,
	Blocked,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublishReadinessPackageStatus {
	Ready,
	AlreadyPublished,
	Unsupported,
	Blocked,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublishReadinessPackage {
	pub package: String,
	pub ecosystem: Ecosystem,
	pub registry: String,
	pub version: String,
	pub status: PublishReadinessPackageStatus,
	pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublishReadinessReport {
	#[serde(default = "publish_readiness_schema_version")]
	pub schema_version: u64,
	#[serde(default = "default_publish_readiness_kind")]
	pub kind: String,
	pub status: PublishReadinessGlobalStatus,
	pub from: String,
	pub resolved_commit: String,
	pub record_commit: String,
	pub package_set_fingerprint: String,
	pub packages: Vec<PublishReadinessPackage>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct PublishReadinessSource<'a> {
	from: &'a str,
	resolved_commit: &'a str,
	record_commit: &'a str,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct PackageIdentity {
	package: String,
	ecosystem: Ecosystem,
	registry: String,
	version: String,
}

pub(crate) fn run_publish_readiness(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	options: &PublishReadinessOptions,
) -> MonochangeResult<String> {
	let report = build_publish_readiness_report(
		root,
		configuration,
		&options.from,
		&options.selected_packages,
	)?;
	options
		.output
		.as_deref()
		.map(|output| write_report_artifact(output, &report))
		.transpose()?;
	render_report(&report, options.format)
}

pub(crate) fn validate_publish_readiness_artifact(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	selected_packages: &BTreeSet<String>,
	artifact_path: &Path,
) -> MonochangeResult<()> {
	let artifact = read_report_artifact(artifact_path)?;
	#[rustfmt::skip]
	let current_report = build_publish_readiness_report_for_publish(root, configuration, prepared_release, selected_packages)?;
	validate_publish_readiness_report(&artifact, &current_report)
}

fn build_publish_readiness_report(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	from: &str,
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<PublishReadinessReport> {
	let discovery = discover_release_record(root, from)?;
	#[rustfmt::skip]
	let publish_report = package_publish::run_publish_packages_with_publications(root, configuration, &discovery.record.package_publications, selected_packages, true)?;
	Ok(build_report_from_publish_report(
		source_from_discovery(&discovery),
		&publish_report,
	))
}

fn build_publish_readiness_report_for_publish(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<PublishReadinessReport> {
	if let Some(prepared_release) = prepared_release {
		#[rustfmt::skip]
		let publish_report = package_publish::run_publish_packages(root, configuration, Some(prepared_release), selected_packages, true)?;
		let source = PublishReadinessSource {
			from: "prepared-release",
			resolved_commit: "prepared-release",
			record_commit: "prepared-release",
		};
		return Ok(build_report_from_publish_report(source, &publish_report));
	}
	build_publish_readiness_report(root, configuration, "HEAD", selected_packages)
}

fn build_report_from_publish_report(
	source: PublishReadinessSource<'_>,
	report: &package_publish::PackagePublishReport,
) -> PublishReadinessReport {
	let packages = report
		.packages
		.iter()
		.map(|package| {
			PublishReadinessPackage {
				package: package.package.clone(),
				ecosystem: package.ecosystem,
				registry: package.registry.clone(),
				version: package.version.clone(),
				status: readiness_status_from_publish_status(package.status),
				message: package.message.clone(),
			}
		})
		.collect::<Vec<_>>();
	let status = if packages.iter().any(|package| {
		matches!(
			package.status,
			PublishReadinessPackageStatus::Unsupported | PublishReadinessPackageStatus::Blocked
		)
	}) {
		PublishReadinessGlobalStatus::Blocked
	} else {
		PublishReadinessGlobalStatus::Ready
	};
	let package_set_fingerprint = package_set_fingerprint(&packages);
	PublishReadinessReport {
		schema_version: PUBLISH_READINESS_SCHEMA_VERSION,
		kind: PUBLISH_READINESS_KIND.to_string(),
		status,
		from: source.from.to_string(),
		resolved_commit: source.resolved_commit.to_string(),
		record_commit: source.record_commit.to_string(),
		package_set_fingerprint,
		packages,
	}
}

fn source_from_discovery(discovery: &ReleaseRecordDiscovery) -> PublishReadinessSource<'_> {
	PublishReadinessSource {
		from: &discovery.input_ref,
		resolved_commit: &discovery.resolved_commit,
		record_commit: &discovery.record_commit,
	}
}

fn readiness_status_from_publish_status(
	status: package_publish::PackagePublishStatus,
) -> PublishReadinessPackageStatus {
	match status {
		package_publish::PackagePublishStatus::Planned
		| package_publish::PackagePublishStatus::Published => PublishReadinessPackageStatus::Ready,
		package_publish::PackagePublishStatus::SkippedExisting => {
			PublishReadinessPackageStatus::AlreadyPublished
		}
		package_publish::PackagePublishStatus::SkippedExternal => {
			PublishReadinessPackageStatus::Unsupported
		}
		package_publish::PackagePublishStatus::Blocked => PublishReadinessPackageStatus::Blocked,
	}
}

fn write_report_artifact(output: &Path, report: &PublishReadinessReport) -> MonochangeResult<()> {
	output
		.parent()
		.filter(|parent| !parent.as_os_str().is_empty())
		.map(create_report_artifact_directory)
		.transpose()?;
	let body = serde_json::to_string_pretty(report).map_err(publish_readiness_json_error)?;
	fs::write(output, format!("{body}\n")).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write publish readiness output {}: {error}",
			output.display()
		))
	})
}

fn read_report_artifact(input: &Path) -> MonochangeResult<PublishReadinessReport> {
	let body = fs::read_to_string(input).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read publish readiness artifact {}: {error}",
			input.display()
		))
	})?;
	serde_json::from_str(&body).map_err(publish_readiness_json_error)
}

fn validate_publish_readiness_report(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	validate_readiness_artifact_header(artifact)?;
	validate_readiness_artifact_status(artifact)?;
	validate_readiness_current_status(current)?;
	validate_readiness_release_commit(artifact, current)?;
	validate_readiness_packages(artifact, current)
}

fn validate_readiness_artifact_header(report: &PublishReadinessReport) -> MonochangeResult<()> {
	if report.kind != PUBLISH_READINESS_KIND {
		return Err(MonochangeError::Config(format!(
			"publish readiness artifact has kind `{}`, expected `{PUBLISH_READINESS_KIND}`",
			report.kind
		)));
	}
	if report.schema_version != PUBLISH_READINESS_SCHEMA_VERSION {
		return Err(MonochangeError::Config(format!(
			"publish readiness artifact schema version {} is not supported; expected {PUBLISH_READINESS_SCHEMA_VERSION}",
			report.schema_version
		)));
	}
	Ok(())
}

fn validate_readiness_artifact_status(report: &PublishReadinessReport) -> MonochangeResult<()> {
	if report.status == PublishReadinessGlobalStatus::Ready {
		return Ok(());
	}
	Err(MonochangeError::Config(
		"publish readiness artifact is blocked; rerun `mc publish-readiness` and resolve blockers before `mc publish`".to_string(),
	))
}

fn validate_readiness_current_status(report: &PublishReadinessReport) -> MonochangeResult<()> {
	if report.status == PublishReadinessGlobalStatus::Ready {
		return Ok(());
	}
	Err(MonochangeError::Config(
		"current publish readiness is blocked; rerun `mc publish-readiness` and resolve blockers before `mc publish`".to_string(),
	))
}

fn validate_readiness_release_commit(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	if artifact.record_commit == current.record_commit {
		return Ok(());
	}
	Err(MonochangeError::Config(format!(
		"publish readiness artifact was generated for release record {}, but `mc publish` selected {}; rerun `mc publish-readiness --from HEAD --output <PATH>`",
		artifact.record_commit, current.record_commit
	)))
}

fn validate_readiness_packages(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	let artifact_packages = package_identities(&artifact.packages)?;
	let current_packages = package_identities(&current.packages)?;
	if artifact.package_set_fingerprint != package_set_fingerprint(&artifact.packages) {
		return Err(MonochangeError::Config(
			"publish readiness artifact package fingerprint does not match its package list"
				.to_string(),
		));
	}
	if artifact_packages == current_packages {
		return Ok(());
	}
	let missing = current_packages
		.difference(&artifact_packages)
		.map(render_package_identity)
		.collect::<Vec<_>>();
	let stale = artifact_packages
		.difference(&current_packages)
		.map(render_package_identity)
		.collect::<Vec<_>>();
	Err(MonochangeError::Config(format!(
		"publish readiness artifact package set is stale or does not match selected packages (missing: {}; stale: {})",
		render_package_identity_list(&missing),
		render_package_identity_list(&stale)
	)))
}

fn package_identities(
	packages: &[PublishReadinessPackage],
) -> MonochangeResult<BTreeSet<PackageIdentity>> {
	let mut identities = BTreeSet::new();
	for package in packages {
		let identity = package_identity(package);
		if !identities.insert(identity.clone()) {
			return Err(MonochangeError::Config(format!(
				"publish readiness artifact contains duplicate package entry {}",
				render_package_identity(&identity)
			)));
		}
	}
	Ok(identities)
}

fn package_set_fingerprint(packages: &[PublishReadinessPackage]) -> String {
	packages
		.iter()
		.map(package_identity)
		.collect::<BTreeSet<_>>()
		.iter()
		.map(render_package_identity)
		.collect::<Vec<_>>()
		.join("\n")
}

fn package_identity(package: &PublishReadinessPackage) -> PackageIdentity {
	PackageIdentity {
		package: package.package.clone(),
		ecosystem: package.ecosystem,
		registry: package.registry.clone(),
		version: package.version.clone(),
	}
}

fn render_package_identity(identity: &PackageIdentity) -> String {
	format!(
		"{} {:?} {} {}",
		identity.package, identity.ecosystem, identity.registry, identity.version
	)
}

fn render_package_identity_list(identities: &[String]) -> String {
	if identities.is_empty() {
		return "none".to_string();
	}
	identities.join(", ")
}

fn publish_readiness_json_error(error: impl std::fmt::Display) -> MonochangeError {
	MonochangeError::Config(format!("publish readiness JSON: {error}"))
}

fn create_report_artifact_directory(parent: &Path) -> MonochangeResult<()> {
	fs::create_dir_all(parent).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create publish readiness output directory {}: {error}",
			parent.display()
		))
	})
}

fn render_report(
	report: &PublishReadinessReport,
	format: OutputFormat,
) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json => {
			serde_json::to_string_pretty(report)
				.map(|body| format!("{body}\n"))
				.map_err(publish_readiness_json_error)
		}
		OutputFormat::Markdown => Ok(render_markdown_report(report)),
		OutputFormat::Text => Ok(render_text_report(report)),
	}
}
fn render_text_report(report: &PublishReadinessReport) -> String {
	let mut lines = vec![format!(
		"publish readiness: {}",
		readiness_global_status_label(report.status)
	)];
	lines.push(format!("release ref: {}", report.from));
	lines.push(format!("release record: {}", report.record_commit));
	if report.packages.is_empty() {
		lines.push("packages: none".to_string());
	} else {
		lines.push("packages:".to_string());
		for package in &report.packages {
			lines.push(format!(
				"- {} {} {} [{}]: {}",
				package.package,
				package.version,
				package.registry,
				readiness_package_status_label(package.status),
				package.message
			));
		}
	}
	format!("{}\n", lines.join("\n"))
}

fn render_markdown_report(report: &PublishReadinessReport) -> String {
	let mut lines = vec![
		"## Publish readiness".to_string(),
		String::new(),
		format!(
			"- Status: `{}`",
			readiness_global_status_label(report.status)
		),
		format!("- Release ref: `{}`", report.from),
		format!("- Release record: `{}`", report.record_commit),
		String::new(),
	];
	if report.packages.is_empty() {
		lines.push("No packages selected for publishing.".to_string());
	} else {
		lines.push("| Package | Version | Registry | Status | Message |".to_string());
		lines.push("| --- | --- | --- | --- | --- |".to_string());
		for package in &report.packages {
			lines.push(format!(
				"| `{}` | `{}` | `{}` | `{}` | {} |",
				package.package,
				package.version,
				package.registry,
				readiness_package_status_label(package.status),
				package.message.replace('|', "\\|")
			));
		}
	}
	format!("{}\n", lines.join("\n"))
}

fn readiness_global_status_label(status: PublishReadinessGlobalStatus) -> &'static str {
	match status {
		PublishReadinessGlobalStatus::Ready => "ready",
		PublishReadinessGlobalStatus::Blocked => "blocked",
	}
}

fn readiness_package_status_label(status: PublishReadinessPackageStatus) -> &'static str {
	match status {
		PublishReadinessPackageStatus::Ready => "ready",
		PublishReadinessPackageStatus::AlreadyPublished => "already_published",
		PublishReadinessPackageStatus::Unsupported => "unsupported",
		PublishReadinessPackageStatus::Blocked => "blocked",
	}
}

fn publish_readiness_schema_version() -> u64 {
	PUBLISH_READINESS_SCHEMA_VERSION
}

fn default_publish_readiness_kind() -> String {
	PUBLISH_READINESS_KIND.to_string()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sample_publish_outcome(
		status: package_publish::PackagePublishStatus,
	) -> package_publish::PackagePublishOutcome {
		package_publish::PackagePublishOutcome {
			package: "core".to_string(),
			ecosystem: Ecosystem::Cargo,
			registry: "crates.io".to_string(),
			version: "1.2.3".to_string(),
			status,
			message: "ready to publish core 1.2.3".to_string(),
			placeholder: false,
			trusted_publishing: package_publish::TrustedPublishingOutcome {
				status: package_publish::TrustedPublishingStatus::Disabled,
				repository: None,
				workflow: None,
				environment: None,
				setup_url: None,
				message: "trusted publishing disabled".to_string(),
			},
		}
	}

	fn sample_source() -> PublishReadinessSource<'static> {
		PublishReadinessSource {
			from: "HEAD",
			resolved_commit: "resolved123",
			record_commit: "record123",
		}
	}

	fn sample_readiness_report(packages: Vec<PublishReadinessPackage>) -> PublishReadinessReport {
		PublishReadinessReport {
			schema_version: PUBLISH_READINESS_SCHEMA_VERSION,
			kind: PUBLISH_READINESS_KIND.to_string(),
			status: PublishReadinessGlobalStatus::Ready,
			from: "HEAD".to_string(),
			resolved_commit: "resolved123".to_string(),
			record_commit: "record123".to_string(),
			package_set_fingerprint: package_set_fingerprint(&packages),
			packages,
		}
	}

	fn sample_readiness_package() -> PublishReadinessPackage {
		PublishReadinessPackage {
			package: "core".to_string(),
			ecosystem: Ecosystem::Cargo,
			registry: "crates.io".to_string(),
			version: "1.2.3".to_string(),
			status: PublishReadinessPackageStatus::Ready,
			message: "ready to publish core 1.2.3".to_string(),
		}
	}

	fn sample_configuration(root: &Path) -> WorkspaceConfiguration {
		WorkspaceConfiguration {
			root_path: root.to_path_buf(),
			defaults: monochange_core::WorkspaceDefaults::default(),
			changelog: monochange_core::ChangelogSettings::default(),
			packages: Vec::new(),
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
			lints: monochange_core::lint::WorkspaceLintSettings::default(),
			cargo: monochange_core::EcosystemSettings::default(),
			npm: monochange_core::EcosystemSettings::default(),
			deno: monochange_core::EcosystemSettings::default(),
			dart: monochange_core::EcosystemSettings::default(),
			python: monochange_core::EcosystemSettings::default(),
		}
	}

	fn sample_prepared_release(root: &Path) -> PreparedRelease {
		PreparedRelease {
			plan: monochange_core::ReleasePlan {
				workspace_root: root.to_path_buf(),
				decisions: Vec::new(),
				groups: Vec::new(),
				warnings: Vec::new(),
				unresolved_items: Vec::new(),
				compatibility_evidence: Vec::new(),
			},
			changeset_paths: Vec::new(),
			changesets: Vec::new(),
			released_packages: Vec::new(),
			package_publications: Vec::new(),
			version: None,
			group_version: None,
			release_targets: Vec::new(),
			changed_files: Vec::new(),
			changelogs: Vec::new(),
			updated_changelogs: Vec::new(),
			deleted_changesets: Vec::new(),
			dry_run: true,
		}
	}

	#[test]
	fn build_report_maps_publish_dry_run_statuses_to_readiness_statuses() {
		let report = package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Release,
			dry_run: true,
			packages: vec![
				sample_publish_outcome(package_publish::PackagePublishStatus::Planned),
				sample_publish_outcome(package_publish::PackagePublishStatus::SkippedExisting),
				sample_publish_outcome(package_publish::PackagePublishStatus::SkippedExternal),
				sample_publish_outcome(package_publish::PackagePublishStatus::Blocked),
			],
		};
		let readiness = build_report_from_publish_report(sample_source(), &report);

		assert_eq!(readiness.schema_version, PUBLISH_READINESS_SCHEMA_VERSION);
		assert_eq!(readiness.kind, PUBLISH_READINESS_KIND);
		assert_eq!(readiness.from, "HEAD");
		assert_eq!(readiness.resolved_commit, "resolved123");
		assert_eq!(readiness.record_commit, "record123");
		assert_eq!(readiness.status, PublishReadinessGlobalStatus::Blocked);
		assert_eq!(
			readiness.packages[0].status,
			PublishReadinessPackageStatus::Ready
		);
		assert_eq!(
			readiness.packages[1].status,
			PublishReadinessPackageStatus::AlreadyPublished
		);
		assert_eq!(
			readiness.packages[2].status,
			PublishReadinessPackageStatus::Unsupported
		);
		assert_eq!(
			readiness.packages[3].status,
			PublishReadinessPackageStatus::Blocked
		);
		assert!(!readiness.package_set_fingerprint.is_empty());
	}

	#[test]
	fn render_report_supports_json_text_and_markdown() {
		let report = sample_readiness_report(vec![sample_readiness_package()]);

		let text = render_report(&report, OutputFormat::Text)
			.unwrap_or_else(|error| panic!("text report: {error}"));
		assert!(text.contains("publish readiness: ready"));
		assert!(text.contains("release record: record123"));
		let markdown = render_report(&report, OutputFormat::Markdown)
			.unwrap_or_else(|error| panic!("markdown report: {error}"));
		assert!(markdown.contains("## Publish readiness"));
		assert!(markdown.contains("Release record: `record123`"));
		let json = render_report(&report, OutputFormat::Json)
			.unwrap_or_else(|error| panic!("json report: {error}"));
		assert!(json.contains("\"status\": \"ready\""));
		assert!(json.contains("\"kind\": \"monochange.publishReadiness\""));
	}

	#[test]
	fn deserialize_report_uses_schema_and_kind_defaults() {
		let report: PublishReadinessReport = serde_json::from_value(serde_json::json!({
			"status": "ready",
			"from": "HEAD",
			"resolvedCommit": "resolved123",
			"recordCommit": "record123",
			"packageSetFingerprint": "packages:none",
			"packages": []
		}))
		.unwrap_or_else(|error| panic!("deserialize defaulted report: {error}"));

		assert_eq!(report.schema_version, PUBLISH_READINESS_SCHEMA_VERSION);
		assert_eq!(report.kind, PUBLISH_READINESS_KIND);
	}

	#[test]
	fn render_report_handles_empty_package_sections() {
		let report = sample_readiness_report(Vec::new());
		let text = render_report(&report, OutputFormat::Text)
			.unwrap_or_else(|error| panic!("empty text report: {error}"));
		let markdown = render_report(&report, OutputFormat::Markdown)
			.unwrap_or_else(|error| panic!("empty markdown report: {error}"));

		assert!(text.contains("packages: none"));
		assert!(markdown.contains("No packages selected for publishing."));
	}

	#[test]
	fn publish_readiness_json_error_renders_context() {
		let error = serde_json::from_str::<serde_json::Value>("{")
			.expect_err("invalid JSON should produce serde error");
		let error = publish_readiness_json_error(error);

		assert!(error.to_string().contains("publish readiness JSON"));
	}

	#[test]
	fn render_report_labels_blocked_already_published_and_unsupported_packages() {
		let packages = vec![
			PublishReadinessPackage {
				status: PublishReadinessPackageStatus::AlreadyPublished,
				..sample_readiness_package()
			},
			PublishReadinessPackage {
				package: "external".to_string(),
				status: PublishReadinessPackageStatus::Unsupported,
				..sample_readiness_package()
			},
			PublishReadinessPackage {
				package: "blocked".to_string(),
				status: PublishReadinessPackageStatus::Blocked,
				..sample_readiness_package()
			},
		];
		let report = PublishReadinessReport {
			status: PublishReadinessGlobalStatus::Blocked,
			package_set_fingerprint: package_set_fingerprint(&packages),
			packages,
			..sample_readiness_report(Vec::new())
		};
		let markdown = render_report(&report, OutputFormat::Markdown)
			.unwrap_or_else(|error| panic!("blocked markdown report: {error}"));

		assert!(markdown.contains("Status: `blocked`"));
		assert!(markdown.contains("already_published"));
		assert!(markdown.contains("unsupported"));
		assert!(markdown.contains("blocked"));
	}

	#[test]
	fn write_and_read_report_artifact_cover_success_and_io_errors() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let report = sample_readiness_report(vec![sample_readiness_package()]);
		let output = tempdir.path().join("nested/readiness.json");

		write_report_artifact(&output, &report)
			.unwrap_or_else(|error| panic!("write readiness artifact: {error}"));
		let body = fs::read_to_string(&output)
			.unwrap_or_else(|error| panic!("read readiness artifact: {error}"));
		assert!(body.contains("\"package\": \"core\""));
		let loaded = read_report_artifact(&output)
			.unwrap_or_else(|error| panic!("load readiness artifact: {error}"));
		assert_eq!(loaded, report);

		let missing_error = read_report_artifact(&tempdir.path().join("missing.json"))
			.expect_err("missing readiness artifact should fail");
		assert!(
			missing_error
				.to_string()
				.contains("failed to read publish readiness artifact")
		);
		fs::write(&output, "{").unwrap_or_else(|error| panic!("write invalid json: {error}"));
		let parse_error =
			read_report_artifact(&output).expect_err("invalid readiness artifact should fail");
		assert!(parse_error.to_string().contains("publish readiness JSON"));

		let parent_file = tempdir.path().join("parent-file");
		fs::write(&parent_file, "not a directory")
			.unwrap_or_else(|error| panic!("write parent file: {error}"));
		let create_dir_error = write_report_artifact(&parent_file.join("readiness.json"), &report)
			.expect_err("file parent should fail directory creation");
		assert!(
			create_dir_error
				.to_string()
				.contains("failed to create publish readiness output directory")
		);

		let write_error = write_report_artifact(tempdir.path(), &report)
			.expect_err("directory output should fail file write");
		assert!(
			write_error
				.to_string()
				.contains("failed to write publish readiness output")
		);
	}

	#[test]
	fn validate_publish_readiness_report_accepts_matching_ready_reports() {
		let artifact = sample_readiness_report(vec![sample_readiness_package()]);
		let mut current = artifact.clone();
		current.from = "HEAD".to_string();

		validate_publish_readiness_report(&artifact, &current)
			.unwrap_or_else(|error| panic!("matching readiness artifact: {error}"));
	}

	#[test]
	fn validate_publish_readiness_artifact_accepts_prepared_release_reports() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		let artifact_path = root.join("readiness.json");
		let configuration = sample_configuration(root);
		let prepared_release = sample_prepared_release(root);
		let selected_packages = BTreeSet::new();
		let report = build_publish_readiness_report_for_publish(
			root,
			&configuration,
			Some(&prepared_release),
			&selected_packages,
		)
		.unwrap_or_else(|error| panic!("prepared release readiness: {error}"));

		assert_eq!(report.from, "prepared-release");
		write_report_artifact(&artifact_path, &report)
			.unwrap_or_else(|error| panic!("write readiness artifact: {error}"));
		validate_publish_readiness_artifact(
			root,
			&configuration,
			Some(&prepared_release),
			&selected_packages,
			&artifact_path,
		)
		.unwrap_or_else(|error| panic!("validate readiness artifact: {error}"));
	}

	#[test]
	fn validate_publish_readiness_report_rejects_bad_kind_schema_and_statuses() {
		let current = sample_readiness_report(vec![sample_readiness_package()]);

		let mut bad_kind = current.clone();
		bad_kind.kind = "other".to_string();
		let bad_kind_error = validate_publish_readiness_report(&bad_kind, &current)
			.expect_err("bad kind should fail readiness validation");
		assert!(bad_kind_error.to_string().contains("expected"));

		let mut bad_schema = current.clone();
		bad_schema.schema_version = 99;
		let bad_schema_error = validate_publish_readiness_report(&bad_schema, &current)
			.expect_err("bad schema should fail readiness validation");
		assert!(bad_schema_error.to_string().contains("not supported"));

		let mut blocked_artifact = current.clone();
		blocked_artifact.status = PublishReadinessGlobalStatus::Blocked;
		let blocked_artifact_error = validate_publish_readiness_report(&blocked_artifact, &current)
			.expect_err("blocked artifact should fail readiness validation");
		assert!(
			blocked_artifact_error
				.to_string()
				.contains("artifact is blocked")
		);

		let mut blocked_current = current.clone();
		blocked_current.status = PublishReadinessGlobalStatus::Blocked;
		let blocked_current_error = validate_publish_readiness_report(&current, &blocked_current)
			.expect_err("blocked current readiness should fail validation");
		assert!(
			blocked_current_error
				.to_string()
				.contains("current publish readiness is blocked")
		);
	}

	#[test]
	fn validate_publish_readiness_report_rejects_stale_commits_and_packages() {
		let current = sample_readiness_report(vec![sample_readiness_package()]);

		let mut stale_commit = current.clone();
		stale_commit.record_commit = "old-record".to_string();
		let stale_commit_error = validate_publish_readiness_report(&stale_commit, &current)
			.expect_err("stale release record should fail validation");
		assert!(stale_commit_error.to_string().contains("old-record"));

		let mut missing_package = current.clone();
		missing_package.packages.clear();
		missing_package.package_set_fingerprint =
			package_set_fingerprint(&missing_package.packages);
		let missing_package_error = validate_publish_readiness_report(&missing_package, &current)
			.expect_err("missing package should fail validation");
		assert!(missing_package_error.to_string().contains("missing: core"));

		let mut stale_package = current.clone();
		stale_package.packages.push(PublishReadinessPackage {
			package: "web".to_string(),
			..sample_readiness_package()
		});
		stale_package.package_set_fingerprint = package_set_fingerprint(&stale_package.packages);
		let stale_package_error = validate_publish_readiness_report(&stale_package, &current)
			.expect_err("stale package should fail validation");
		assert!(stale_package_error.to_string().contains("stale: web"));
	}

	#[test]
	fn validate_publish_readiness_report_rejects_tampered_fingerprint_and_duplicates() {
		let current = sample_readiness_report(vec![sample_readiness_package()]);

		let mut bad_fingerprint = current.clone();
		bad_fingerprint.package_set_fingerprint = "tampered".to_string();
		let bad_fingerprint_error = validate_publish_readiness_report(&bad_fingerprint, &current)
			.expect_err("tampered package fingerprint should fail validation");
		assert!(
			bad_fingerprint_error
				.to_string()
				.contains("package fingerprint")
		);

		let duplicate_package = PublishReadinessPackage {
			message: "duplicate".to_string(),
			..sample_readiness_package()
		};
		let mut duplicates =
			sample_readiness_report(vec![sample_readiness_package(), duplicate_package]);
		duplicates.package_set_fingerprint = package_set_fingerprint(&duplicates.packages);
		let duplicate_error = validate_publish_readiness_report(&duplicates, &current)
			.expect_err("duplicate package should fail validation");
		assert!(
			duplicate_error
				.to_string()
				.contains("duplicate package entry")
		);
	}

	#[test]
	fn render_package_identity_list_labels_empty_lists() {
		assert_eq!(render_package_identity_list(&[]), "none");
		assert_eq!(
			render_package_identity_list(&["core Cargo crates.io 1.2.3".to_string()]),
			"core Cargo crates.io 1.2.3"
		);
	}
}
