#![allow(clippy::disallowed_methods)]
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
		command: None,
		stdout: None,
		stderr: None,
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
		input_fingerprint: "fnv1a64:sample".to_string(),
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

fn readiness_package(
	package: &str,
	registry: &str,
	status: PublishReadinessPackageStatus,
) -> PublishReadinessPackage {
	PublishReadinessPackage {
		package: package.to_string(),
		registry: registry.to_string(),
		status,
		message: format!("{package} readiness"),
		..sample_readiness_package()
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
		go: monochange_core::EcosystemSettings::default(),
	}
}

fn sample_package_definition(
	id: &str,
	path: &str,
	package_type: PackageType,
) -> monochange_core::PackageDefinition {
	monochange_core::PackageDefinition {
		id: id.to_string(),
		path: PathBuf::from(path),
		package_type,
		changelog: None,
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		ignore_ecosystem_versioned_files: false,
		ignored_paths: Vec::new(),
		additional_paths: Vec::new(),
		tag: true,
		release: true,
		version_format: monochange_core::VersionFormat::default(),
		publish: monochange_core::PublishSettings::default(),
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
	let readiness =
		build_report_from_publish_report(sample_source(), &report, "fnv1a64:sample".to_string());

	assert_eq!(readiness.schema_version, PUBLISH_READINESS_SCHEMA_VERSION);
	assert_eq!(readiness.kind, PUBLISH_READINESS_KIND);
	assert_eq!(readiness.from, "HEAD");
	assert_eq!(readiness.resolved_commit, "resolved123");
	assert_eq!(readiness.record_commit, "record123");
	assert_eq!(readiness.input_fingerprint, "fnv1a64:sample");
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
fn publish_readiness_input_fingerprint_tracks_publish_inputs() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let mut configuration = sample_configuration(root);
	configuration.packages = vec![
		sample_package_definition("cargo", "crates/core", PackageType::Cargo),
		sample_package_definition("npm", "packages/web", PackageType::Npm),
		sample_package_definition("deno", "packages/deno", PackageType::Deno),
		sample_package_definition("dart", "packages/dart", PackageType::Dart),
		sample_package_definition("flutter", "packages/flutter", PackageType::Flutter),
		sample_package_definition("python", "packages/python", PackageType::Python),
	];

	write_test_file(root.join("monochange.toml"), b"[workspace]\n");
	write_test_file(root.join("Cargo.toml"), b"[workspace]\n");
	write_test_file(root.join("package.json"), br#"{"private":true}"#);
	write_test_file(root.join("pnpm-lock.yaml"), b"lockfileVersion: '9.0'\n");
	write_test_file(root.join(".npmrc"), b"provenance=true\n");
	write_test_file(
		root.join("crates/core/Cargo.toml"),
		b"[package]\nname='core'\n",
	);
	write_test_file(root.join("crates/core/Cargo.lock"), b"version = 4\n");
	write_test_file(root.join("packages/web/package.json"), br#"{"name":"web"}"#);
	write_test_file(
		root.join("packages/web/pnpm-lock.yaml"),
		b"lockfileVersion: '9.0'\n",
	);
	write_test_file(root.join("packages/deno/deno.jsonc"), b"{}\n");
	write_test_file(root.join("packages/dart/pubspec.yaml"), b"name: dart\n");
	write_test_file(
		root.join("packages/flutter/pubspec.yaml"),
		b"name: flutter\n",
	);
	write_test_file(
		root.join("packages/python/pyproject.toml"),
		b"[project]\nname='python'\n",
	);

	let paths = publish_readiness_input_paths(root, &configuration);
	let relative_paths: BTreeSet<_> = paths
		.iter()
		.map(|path| readiness_relative_path(root, path))
		.collect();
	let expected_paths = BTreeSet::from([
		".npmrc".to_string(),
		"Cargo.toml".to_string(),
		"crates/core/Cargo.lock".to_string(),
		"crates/core/Cargo.toml".to_string(),
		"monochange.toml".to_string(),
		"package.json".to_string(),
		"packages/dart/pubspec.yaml".to_string(),
		"packages/deno/deno.jsonc".to_string(),
		"packages/flutter/pubspec.yaml".to_string(),
		"packages/python/pyproject.toml".to_string(),
		"packages/web/package.json".to_string(),
		"packages/web/pnpm-lock.yaml".to_string(),
		"pnpm-lock.yaml".to_string(),
	]);
	assert_eq!(relative_paths, expected_paths);
	assert!(package_manifest_names_for_type("unknown").is_empty());

	let initial_fingerprint = publish_readiness_input_fingerprint(root, &configuration)
		.unwrap_or_else(|error| panic!("initial fingerprint: {error}"));
	write_test_file(
		root.join("packages/web/package.json"),
		br#"{"name":"web","type":"module"}"#,
	);
	let changed_fingerprint = publish_readiness_input_fingerprint(root, &configuration)
		.unwrap_or_else(|error| panic!("changed fingerprint: {error}"));

	assert_ne!(initial_fingerprint, changed_fingerprint);
	assert!(initial_fingerprint.starts_with("fnv1a64:"));
}

#[cfg(unix)]
#[test]
fn publish_readiness_input_fingerprint_reports_read_errors() {
	use std::os::unix::fs::PermissionsExt;

	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let configuration = sample_configuration(root);
	let input_path = root.join("monochange.toml");
	write_test_file(&input_path, b"[workspace]\n");
	fs::set_permissions(&input_path, fs::Permissions::from_mode(0o000))
		.unwrap_or_else(|error| panic!("remove read permission: {error}"));

	let error = publish_readiness_input_fingerprint(root, &configuration)
		.expect_err("unreadable input should report a read error");

	fs::set_permissions(&input_path, fs::Permissions::from_mode(0o600))
		.unwrap_or_else(|error| panic!("restore read permission: {error}"));
	assert!(
		error
			.to_string()
			.contains("failed to read publish readiness input")
	);
}

#[test]
fn validate_publish_readiness_report_rejects_stale_input_fingerprints() {
	let mut artifact = sample_readiness_report(vec![sample_readiness_package()]);
	let current = artifact.clone();
	artifact.input_fingerprint = "fnv1a64:stale".to_string();

	let error = validate_publish_readiness_report(&artifact, &current)
		.expect_err("stale input fingerprint should be rejected");

	assert!(error.to_string().contains("inputs are stale"));
}

fn write_test_file(path: impl AsRef<Path>, contents: &[u8]) {
	let path = path.as_ref();
	let parent = path.parent().unwrap_or(Path::new("."));
	fs::create_dir_all(parent)
		.unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
	fs::write(path, contents).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
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

#[tokio::test(flavor = "multi_thread")]
async fn validate_publish_readiness_artifact_accepts_prepared_release_reports() {
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
	.await
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
	.await
	.unwrap_or_else(|error| panic!("validate readiness artifact: {error}"));
}

#[test]
fn publish_plan_ready_package_ids_requires_every_package_row_to_be_ready() {
	let report = sample_readiness_report(vec![
		readiness_package("core", "crates.io", PublishReadinessPackageStatus::Ready),
		readiness_package(
			"core",
			"npm",
			PublishReadinessPackageStatus::AlreadyPublished,
		),
		readiness_package("web", "crates.io", PublishReadinessPackageStatus::Ready),
		readiness_package("web", "npm", PublishReadinessPackageStatus::Blocked),
		readiness_package(
			"docs",
			"crates.io",
			PublishReadinessPackageStatus::AlreadyPublished,
		),
		readiness_package(
			"external",
			"crates.io",
			PublishReadinessPackageStatus::Unsupported,
		),
	]);

	let ready_packages = publish_plan_ready_package_ids(&report);

	assert_eq!(
		ready_packages,
		BTreeSet::from(["core".to_string(), "docs".to_string()])
	);
}

#[test]
fn validate_publish_readiness_plan_artifact_accepts_blocked_subset_reports() {
	let artifact = sample_readiness_report(vec![
		readiness_package("core", "crates.io", PublishReadinessPackageStatus::Ready),
		readiness_package("web", "crates.io", PublishReadinessPackageStatus::Blocked),
		readiness_package("extra", "crates.io", PublishReadinessPackageStatus::Ready),
	]);
	let current = PublishReadinessReport {
		status: PublishReadinessGlobalStatus::Blocked,
		packages: vec![
			readiness_package("core", "crates.io", PublishReadinessPackageStatus::Ready),
			readiness_package("web", "crates.io", PublishReadinessPackageStatus::Blocked),
		],
		..sample_readiness_report(Vec::new())
	};

	validate_publish_readiness_plan_artifact(&artifact, &current)
		.unwrap_or_else(|error| panic!("planning readiness artifact: {error}"));
}

#[test]
fn validate_publish_readiness_plan_artifact_rejects_tampering_and_missing_coverage() {
	let current = sample_readiness_report(vec![
		readiness_package("core", "crates.io", PublishReadinessPackageStatus::Ready),
		readiness_package("web", "crates.io", PublishReadinessPackageStatus::Ready),
	]);

	let mut tampered = current.clone();
	tampered.package_set_fingerprint = "tampered".to_string();
	let tampered_error = validate_publish_readiness_plan_artifact(&tampered, &current)
		.expect_err("tampered planning artifact should fail validation");
	assert!(tampered_error.to_string().contains("package fingerprint"));

	let missing = sample_readiness_report(vec![readiness_package(
		"core",
		"crates.io",
		PublishReadinessPackageStatus::Ready,
	)]);
	let missing_error = validate_publish_readiness_plan_artifact(&missing, &current)
		.expect_err("planning artifact missing current package should fail");
	assert!(
		missing_error
			.to_string()
			.contains("does not cover selected packages")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_plan_package_filter_from_readiness_artifact_accepts_empty_prepared_release() {
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
	.await
	.unwrap_or_else(|error| panic!("prepared release readiness: {error}"));

	write_report_artifact(&artifact_path, &report)
		.unwrap_or_else(|error| panic!("write readiness artifact: {error}"));
	let planned_packages = publish_plan_package_filter_from_readiness_artifact(
		root,
		&configuration,
		Some(&prepared_release),
		&selected_packages,
		&artifact_path,
	)
	.await
	.unwrap_or_else(|error| panic!("publish plan readiness filter: {error}"));

	assert!(planned_packages.is_empty());
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
	missing_package.package_set_fingerprint = package_set_fingerprint(&missing_package.packages);
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

#[tokio::test(flavor = "multi_thread")]
async fn build_publish_readiness_for_publish_falls_back_to_head_without_prepared_release() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	std::process::Command::new("git")
		.current_dir(root)
		.args(["init"])
		.output()
		.unwrap_or_else(|error| panic!("git init: {error}"));
	std::process::Command::new("git")
		.current_dir(root)
		.args(["config", "user.email", "monochange@example.com"])
		.output()
		.unwrap_or_else(|error| panic!("git config email: {error}"));
	std::process::Command::new("git")
		.current_dir(root)
		.args(["config", "user.name", "monochange Tests"])
		.output()
		.unwrap_or_else(|error| panic!("git config name: {error}"));
	std::process::Command::new("git")
		.current_dir(root)
		.args(["config", "commit.gpgsign", "false"])
		.output()
		.unwrap_or_else(|error| panic!("git config gpgsign: {error}"));
	fs::write(root.join("README.md"), "readme\n")
		.unwrap_or_else(|error| panic!("write readme: {error}"));
	std::process::Command::new("git")
		.current_dir(root)
		.args(["add", "."])
		.output()
		.unwrap_or_else(|error| panic!("git add: {error}"));
	std::process::Command::new("git")
		.current_dir(root)
		.args(["commit", "-m", "initial"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));

	let error = build_publish_readiness_report_for_publish(
		root,
		&sample_configuration(root),
		None,
		&BTreeSet::new(),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("expected missing release record error"));

	assert!(error.to_string().contains("no monochange release record"));
}
