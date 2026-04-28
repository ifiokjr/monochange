use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackagePublicationTarget;
use monochange_core::WorkspaceConfiguration;
use serde::Serialize;

use crate::OutputFormat;
use crate::discover_release_record;
use crate::package_publish;

const PUBLISH_BOOTSTRAP_SCHEMA_VERSION: u64 = 1;
const PUBLISH_BOOTSTRAP_KIND: &str = "monochange.publishBootstrap";
const EMPTY_BOOTSTRAP_PACKAGE_SENTINEL: &str = "\0monochange-empty-publish-bootstrap-selection";

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PublishBootstrapOptions {
	pub from: String,
	pub selected_packages: BTreeSet<String>,
	pub format: OutputFormat,
	pub output: Option<PathBuf>,
	pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublishBootstrapStatus {
	Planned,
	Completed,
	Blocked,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublishBootstrapReport {
	pub schema_version: u64,
	pub kind: String,
	pub status: PublishBootstrapStatus,
	pub from: String,
	pub resolved_commit: String,
	pub record_commit: String,
	pub dry_run: bool,
	pub release_packages: Vec<String>,
	pub selected_packages: Vec<String>,
	pub package_publish: package_publish::PackagePublishReport,
}

pub(crate) fn run_publish_bootstrap(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	options: &PublishBootstrapOptions,
) -> MonochangeResult<String> {
	let report = build_publish_bootstrap_report(root, configuration, options)?;
	if let Some(output) = &options.output {
		write_bootstrap_artifact(output, &report)?;
	}
	render_publish_bootstrap_report(&report, options.format)
}

pub(crate) fn build_publish_bootstrap_report(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	options: &PublishBootstrapOptions,
) -> MonochangeResult<PublishBootstrapReport> {
	let discovery = discover_release_record(root, &options.from)?;
	let release_packages = release_record_package_ids(&discovery.record.package_publications);
	let selected_packages = selected_bootstrap_package_ids(
		&discovery.record.package_publications,
		&options.selected_packages,
	);
	let package_filter = placeholder_publish_package_filter(&selected_packages);
	let package_publish = package_publish::run_placeholder_publish(
		root,
		configuration,
		&package_filter,
		options.dry_run,
	)?;
	let status = bootstrap_status(&package_publish);

	Ok(PublishBootstrapReport {
		schema_version: PUBLISH_BOOTSTRAP_SCHEMA_VERSION,
		kind: PUBLISH_BOOTSTRAP_KIND.to_string(),
		status,
		from: discovery.input_ref,
		resolved_commit: discovery.resolved_commit,
		record_commit: discovery.record_commit,
		dry_run: options.dry_run,
		release_packages: release_packages.into_iter().collect(),
		selected_packages: selected_packages.into_iter().collect(),
		package_publish,
	})
}

fn selected_bootstrap_package_ids(
	publication_targets: &[PackagePublicationTarget],
	selected_packages: &BTreeSet<String>,
) -> BTreeSet<String> {
	let release_packages = release_record_package_ids(publication_targets);
	if selected_packages.is_empty() {
		return release_packages;
	}

	selected_packages
		.intersection(&release_packages)
		.cloned()
		.collect()
}

fn release_record_package_ids(
	publication_targets: &[PackagePublicationTarget],
) -> BTreeSet<String> {
	publication_targets
		.iter()
		.map(|target| target.package.clone())
		.collect()
}

fn placeholder_publish_package_filter(selected_packages: &BTreeSet<String>) -> BTreeSet<String> {
	if !selected_packages.is_empty() {
		return selected_packages.clone();
	}

	BTreeSet::from([EMPTY_BOOTSTRAP_PACKAGE_SENTINEL.to_string()])
}

fn bootstrap_status(report: &package_publish::PackagePublishReport) -> PublishBootstrapStatus {
	let blocked = report.packages.iter().any(|package| {
		matches!(
			package.status,
			package_publish::PackagePublishStatus::Blocked
				| package_publish::PackagePublishStatus::SkippedExternal
		)
	});
	if blocked {
		return PublishBootstrapStatus::Blocked;
	}

	if report.dry_run {
		return PublishBootstrapStatus::Planned;
	}

	PublishBootstrapStatus::Completed
}

fn write_bootstrap_artifact(
	output: &Path,
	report: &PublishBootstrapReport,
) -> MonochangeResult<()> {
	output
		.parent()
		.filter(|parent| !parent.as_os_str().is_empty())
		.map(create_bootstrap_artifact_directory)
		.transpose()?;
	let body = serde_json::to_string_pretty(report).map_err(publish_bootstrap_json_error)?;
	fs::write(output, format!("{body}\n")).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write publish bootstrap output {}: {error}",
			output.display()
		))
	})
}

fn create_bootstrap_artifact_directory(parent: &Path) -> MonochangeResult<()> {
	fs::create_dir_all(parent).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create publish bootstrap output directory {}: {error}",
			parent.display()
		))
	})
}

fn publish_bootstrap_json_error(error: impl std::fmt::Display) -> MonochangeError {
	MonochangeError::Config(format!("publish bootstrap JSON: {error}"))
}

fn render_publish_bootstrap_report(
	report: &PublishBootstrapReport,
	format: OutputFormat,
) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json => {
			serde_json::to_string_pretty(report)
				.map(|body| format!("{body}\n"))
				.map_err(publish_bootstrap_json_error)
		}
		OutputFormat::Text => Ok(render_publish_bootstrap_text(report).join("\n")),
		OutputFormat::Markdown => Ok(render_publish_bootstrap_markdown(report).join("\n")),
	}
}

fn render_publish_bootstrap_text(report: &PublishBootstrapReport) -> Vec<String> {
	let mut lines = vec![
		format!(
			"publish bootstrap: {}",
			bootstrap_status_label(report.status)
		),
		format!("release ref: {}", report.from),
		format!(
			"record commit: {}",
			crate::short_commit_sha(&report.record_commit)
		),
		format!("dry-run: {}", yes_no(report.dry_run)),
	];

	if report.selected_packages.is_empty() {
		lines.push("packages: none".to_string());
		return lines;
	}

	lines.push(format!("packages: {}", report.selected_packages.join(", ")));
	for package in &report.package_publish.packages {
		lines.push(format!(
			"- {} {} [{}]: {}",
			package.package,
			package.version,
			package_publish_status_label(package.status),
			package.message
		));
	}
	lines
}

fn render_publish_bootstrap_markdown(report: &PublishBootstrapReport) -> Vec<String> {
	let mut lines = vec![
		format!(
			"# Publish bootstrap: {}",
			bootstrap_status_label(report.status)
		),
		String::new(),
		format!("- **Release ref:** `{}`", report.from),
		format!(
			"- **Record commit:** `{}`",
			crate::short_commit_sha(&report.record_commit)
		),
		format!("- **Dry-run:** {}", yes_no(report.dry_run)),
	];

	if report.selected_packages.is_empty() {
		lines.push("- **Packages:** none".to_string());
		return lines;
	}

	lines.push(format!(
		"- **Packages:** {}",
		report
			.selected_packages
			.iter()
			.map(|package| format!("`{package}`"))
			.collect::<Vec<_>>()
			.join(", ")
	));
	lines.push(String::new());
	lines.push("## Package results".to_string());
	for package in &report.package_publish.packages {
		lines.push(format!(
			"- `{}` `{}` — **{}**: {}",
			package.package,
			package.version,
			package_publish_status_label(package.status),
			package.message
		));
	}
	lines
}

fn bootstrap_status_label(status: PublishBootstrapStatus) -> &'static str {
	match status {
		PublishBootstrapStatus::Planned => "planned",
		PublishBootstrapStatus::Completed => "completed",
		PublishBootstrapStatus::Blocked => "blocked",
	}
}

fn package_publish_status_label(status: package_publish::PackagePublishStatus) -> &'static str {
	match status {
		package_publish::PackagePublishStatus::Planned => "planned",
		package_publish::PackagePublishStatus::Published => "published",
		package_publish::PackagePublishStatus::SkippedExisting => "already-published",
		package_publish::PackagePublishStatus::SkippedExternal => "external",
		package_publish::PackagePublishStatus::Blocked => "blocked",
	}
}

fn yes_no(value: bool) -> &'static str {
	if value { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
	use monochange_core::Ecosystem;
	use monochange_core::PackagePublicationTarget;
	use monochange_core::PublishMode;
	use monochange_core::PublishRegistry;
	use monochange_core::RegistryKind;
	use monochange_core::TrustedPublishingSettings;

	use super::*;

	#[test]
	fn selected_bootstrap_package_ids_intersects_release_packages() {
		let publications = vec![PackagePublicationTarget {
			package: "core".to_string(),
			ecosystem: Ecosystem::Cargo,
			registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
			version: "1.2.3".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
		}];
		let selected = BTreeSet::from(["core".to_string(), "docs".to_string()]);

		assert_eq!(
			selected_bootstrap_package_ids(&publications, &selected),
			BTreeSet::from(["core".to_string()])
		);
	}

	#[test]
	fn bootstrap_status_reports_blocked_before_dry_run_planned() {
		let report = package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Placeholder,
			dry_run: true,
			packages: vec![package_publish::PackagePublishOutcome {
				package: "pkg".to_string(),
				ecosystem: Ecosystem::Cargo,
				registry: "crates.io".to_string(),
				version: "0.0.0".to_string(),
				status: package_publish::PackagePublishStatus::SkippedExternal,
				message: "external publishing configured".to_string(),
				placeholder: true,
				trusted_publishing: package_publish::TrustedPublishingOutcome {
					status: package_publish::TrustedPublishingStatus::Disabled,
					repository: None,
					workflow: None,
					environment: None,
					setup_url: None,
					message: "trusted publishing disabled".to_string(),
				},
			}],
		};

		assert_eq!(bootstrap_status(&report), PublishBootstrapStatus::Blocked);
	}

	#[test]
	fn selected_bootstrap_package_ids_defaults_to_release_packages() {
		let publications = vec![PackagePublicationTarget {
			package: "core".to_string(),
			ecosystem: Ecosystem::Cargo,
			registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
			version: "1.2.3".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
		}];

		assert_eq!(
			selected_bootstrap_package_ids(&publications, &BTreeSet::new()),
			BTreeSet::from(["core".to_string()])
		);
	}

	#[test]
	fn placeholder_publish_package_filter_never_expands_empty_selection_to_all_packages() {
		assert_eq!(
			placeholder_publish_package_filter(&BTreeSet::new()),
			BTreeSet::from([EMPTY_BOOTSTRAP_PACKAGE_SENTINEL.to_string()])
		);

		assert_eq!(
			placeholder_publish_package_filter(&BTreeSet::from(["core".to_string()])),
			BTreeSet::from(["core".to_string()])
		);
	}

	#[test]
	fn bootstrap_status_reports_planned_and_completed() {
		let planned = package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Placeholder,
			dry_run: true,
			packages: Vec::new(),
		};
		let completed = package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Placeholder,
			dry_run: false,
			packages: Vec::new(),
		};
		let completed_with_package = package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Placeholder,
			dry_run: false,
			packages: vec![sample_publish_outcome(
				"published",
				package_publish::PackagePublishStatus::Published,
			)],
		};

		assert_eq!(bootstrap_status(&planned), PublishBootstrapStatus::Planned);
		assert_eq!(
			bootstrap_status(&completed),
			PublishBootstrapStatus::Completed
		);
		assert_eq!(
			bootstrap_status(&completed_with_package),
			PublishBootstrapStatus::Completed
		);
	}

	#[test]
	fn render_publish_bootstrap_report_supports_formats_and_status_labels() {
		let report = sample_bootstrap_report(vec![
			sample_publish_outcome("planned", package_publish::PackagePublishStatus::Planned),
			sample_publish_outcome(
				"published",
				package_publish::PackagePublishStatus::Published,
			),
			sample_publish_outcome(
				"existing",
				package_publish::PackagePublishStatus::SkippedExisting,
			),
			sample_publish_outcome(
				"external",
				package_publish::PackagePublishStatus::SkippedExternal,
			),
			sample_publish_outcome("blocked", package_publish::PackagePublishStatus::Blocked),
		]);

		let json = render_publish_bootstrap_report(&report, OutputFormat::Json)
			.unwrap_or_else(|error| panic!("render json: {error}"));
		assert!(json.contains("\"kind\": \"monochange.publishBootstrap\""));

		let text = render_publish_bootstrap_report(&report, OutputFormat::Text)
			.unwrap_or_else(|error| panic!("render text: {error}"));
		assert!(text.contains("packages: planned, published, existing, external, blocked"));
		assert!(text.contains("[already-published]"));
		assert!(text.contains("[external]"));

		let markdown = render_publish_bootstrap_report(&report, OutputFormat::Markdown)
			.unwrap_or_else(|error| panic!("render markdown: {error}"));
		assert!(markdown.contains("# Publish bootstrap: blocked"));
		assert!(markdown.contains("## Package results"));

		assert_eq!(
			bootstrap_status_label(PublishBootstrapStatus::Completed),
			"completed"
		);
		assert_eq!(yes_no(false), "no");
	}

	#[test]
	fn render_publish_bootstrap_markdown_labels_empty_packages() {
		let report = PublishBootstrapReport {
			selected_packages: Vec::new(),
			package_publish: package_publish::PackagePublishReport {
				mode: package_publish::PackagePublishRunMode::Placeholder,
				dry_run: true,
				packages: Vec::new(),
			},
			..sample_bootstrap_report(Vec::new())
		};

		let markdown = render_publish_bootstrap_report(&report, OutputFormat::Markdown)
			.unwrap_or_else(|error| panic!("render markdown: {error}"));
		assert!(markdown.contains("- **Packages:** none"));
	}

	#[test]
	fn write_bootstrap_artifact_reports_success_and_io_errors() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let report = sample_bootstrap_report(Vec::new());
		let output = tempdir.path().join("nested/bootstrap.json");

		write_bootstrap_artifact(&output, &report)
			.unwrap_or_else(|error| panic!("write artifact: {error}"));
		let artifact =
			fs::read_to_string(&output).unwrap_or_else(|error| panic!("read artifact: {error}"));
		assert!(artifact.contains("monochange.publishBootstrap"));

		let directory_error = write_bootstrap_artifact(tempdir.path(), &report)
			.unwrap_err()
			.to_string();
		assert!(directory_error.contains("failed to write publish bootstrap output"));

		let file_parent = tempdir.path().join("file-parent");
		fs::write(&file_parent, "not a directory")
			.unwrap_or_else(|error| panic!("write file parent: {error}"));
		let create_error = write_bootstrap_artifact(&file_parent.join("bootstrap.json"), &report)
			.unwrap_err()
			.to_string();
		assert!(create_error.contains("failed to create publish bootstrap output directory"));

		assert!(
			publish_bootstrap_json_error("bad json")
				.to_string()
				.contains("bad json")
		);
	}

	fn sample_bootstrap_report(
		packages: Vec<package_publish::PackagePublishOutcome>,
	) -> PublishBootstrapReport {
		let selected_packages = packages
			.iter()
			.map(|package| package.package.clone())
			.collect::<Vec<_>>();

		PublishBootstrapReport {
			schema_version: PUBLISH_BOOTSTRAP_SCHEMA_VERSION,
			kind: PUBLISH_BOOTSTRAP_KIND.to_string(),
			status: PublishBootstrapStatus::Blocked,
			from: "HEAD".to_string(),
			resolved_commit: "1234567890abcdef".to_string(),
			record_commit: "1234567890abcdef".to_string(),
			dry_run: true,
			release_packages: selected_packages.clone(),
			selected_packages,
			package_publish: package_publish::PackagePublishReport {
				mode: package_publish::PackagePublishRunMode::Placeholder,
				dry_run: true,
				packages,
			},
		}
	}

	fn sample_publish_outcome(
		package: &str,
		status: package_publish::PackagePublishStatus,
	) -> package_publish::PackagePublishOutcome {
		package_publish::PackagePublishOutcome {
			package: package.to_string(),
			ecosystem: Ecosystem::Cargo,
			registry: "crates.io".to_string(),
			version: "0.0.0".to_string(),
			status,
			message: "sample outcome".to_string(),
			placeholder: true,
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
}
