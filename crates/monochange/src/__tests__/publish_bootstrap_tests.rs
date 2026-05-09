use monochange_core::Ecosystem;
use monochange_core::PackagePublicationTarget;
use monochange_core::PublishAttestationSettings;
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
		attestations: PublishAttestationSettings::default(),
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
			command: None,
			stdout: None,
			stderr: None,
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
		attestations: PublishAttestationSettings::default(),
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
		sample_publish_outcome("failed", package_publish::PackagePublishStatus::Failed),
	]);

	let json = render_publish_bootstrap_report(&report, OutputFormat::Json)
		.unwrap_or_else(|error| panic!("render json: {error}"));
	assert!(json.contains("\"kind\": \"monochange.publishBootstrap\""));

	let text = render_publish_bootstrap_report(&report, OutputFormat::Text)
		.unwrap_or_else(|error| panic!("render text: {error}"));
	assert!(text.contains("packages: planned, published, existing, external, blocked, failed"));
	assert!(text.contains("[already-published]"));
	assert!(text.contains("[external]"));
	assert!(text.contains("[failed]"));

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
		command: None,
		stdout: None,
		stderr: None,
	}
}
