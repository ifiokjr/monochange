use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::WorkspaceConfiguration;
use serde::Serialize;

use crate::OutputFormat;
use crate::package_publish;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PublishReadinessOptions {
	pub from: String,
	pub selected_packages: BTreeSet<String>,
	pub format: OutputFormat,
	pub output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublishReadinessGlobalStatus {
	Ready,
	Blocked,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublishReadinessPackageStatus {
	Ready,
	AlreadyPublished,
	Unsupported,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublishReadinessPackage {
	pub package: String,
	pub ecosystem: Ecosystem,
	pub registry: String,
	pub version: String,
	pub status: PublishReadinessPackageStatus,
	pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublishReadinessReport {
	pub status: PublishReadinessGlobalStatus,
	pub from: String,
	pub packages: Vec<PublishReadinessPackage>,
}

pub(crate) fn run_publish_readiness(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	options: &PublishReadinessOptions,
) -> MonochangeResult<String> {
	let publish_report = package_publish::run_publish_packages_from_ref(
		root,
		configuration,
		&options.from,
		&options.selected_packages,
		true,
	)?;
	let report = build_report_from_publish_report(&options.from, &publish_report);
	options
		.output
		.as_deref()
		.map(|output| write_report_artifact(output, &report))
		.transpose()?;
	render_report(&report, options.format)
}

fn build_report_from_publish_report(
	from: &str,
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
	let status = if packages
		.iter()
		.any(|package| package.status == PublishReadinessPackageStatus::Unsupported)
	{
		PublishReadinessGlobalStatus::Blocked
	} else {
		PublishReadinessGlobalStatus::Ready
	};
	PublishReadinessReport {
		status,
		from: from.to_string(),
		packages,
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
	}
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

	fn sample_readiness_report(packages: Vec<PublishReadinessPackage>) -> PublishReadinessReport {
		PublishReadinessReport {
			status: PublishReadinessGlobalStatus::Ready,
			from: "HEAD".to_string(),
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

	#[test]
	fn build_report_maps_publish_dry_run_statuses_to_readiness_statuses() {
		let report = package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Release,
			dry_run: true,
			packages: vec![
				sample_publish_outcome(package_publish::PackagePublishStatus::Planned),
				sample_publish_outcome(package_publish::PackagePublishStatus::SkippedExisting),
				sample_publish_outcome(package_publish::PackagePublishStatus::SkippedExternal),
			],
		};
		let readiness = build_report_from_publish_report("HEAD", &report);

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
	}

	#[test]
	fn render_report_supports_json_text_and_markdown() {
		let report = sample_readiness_report(vec![sample_readiness_package()]);

		let text = render_report(&report, OutputFormat::Text)
			.unwrap_or_else(|error| panic!("text report: {error}"));
		assert!(text.contains("publish readiness: ready"));
		let markdown = render_report(&report, OutputFormat::Markdown)
			.unwrap_or_else(|error| panic!("markdown report: {error}"));
		assert!(markdown.contains("## Publish readiness"));
		let json = render_report(&report, OutputFormat::Json)
			.unwrap_or_else(|error| panic!("json report: {error}"));
		assert!(json.contains("\"status\": \"ready\""));
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
		let report = PublishReadinessReport {
			status: PublishReadinessGlobalStatus::Blocked,
			from: "HEAD".to_string(),
			packages: vec![
				PublishReadinessPackage {
					status: PublishReadinessPackageStatus::AlreadyPublished,
					..sample_readiness_package()
				},
				PublishReadinessPackage {
					package: "external".to_string(),
					status: PublishReadinessPackageStatus::Unsupported,
					..sample_readiness_package()
				},
			],
		};
		let markdown = render_report(&report, OutputFormat::Markdown)
			.unwrap_or_else(|error| panic!("blocked markdown report: {error}"));

		assert!(markdown.contains("Status: `blocked`"));
		assert!(markdown.contains("already_published"));
		assert!(markdown.contains("unsupported"));
	}

	#[test]
	fn write_report_artifact_creates_parent_directory_and_reports_io_errors() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let report = sample_readiness_report(vec![sample_readiness_package()]);
		let output = tempdir.path().join("nested/readiness.json");

		write_report_artifact(&output, &report)
			.unwrap_or_else(|error| panic!("write readiness artifact: {error}"));
		let body = fs::read_to_string(&output)
			.unwrap_or_else(|error| panic!("read readiness artifact: {error}"));
		assert!(body.contains("\"package\": \"core\""));

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
}
