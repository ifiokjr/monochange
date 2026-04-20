use std::fs;

use httpmock::Method::GET;
use httpmock::MockServer;
use insta::assert_snapshot;
use insta_cmd::assert_cmd_snapshot;
use monochange_test_helpers::git::git;

mod test_support;
use test_support::copy_directory;
use test_support::current_test_name;
use test_support::fixture_path;
use test_support::monochange_command;
use test_support::setup_scenario_workspace;
use test_support::snapshot_settings;

fn release_cli_command() -> std::process::Command {
	monochange_command(Some("2026-04-06"))
}

fn setup_analyze_cli_repo(first_release: bool) -> tempfile::TempDir {
	let scenario_root = fixture_path("cli-output/analyze-group-release-trajectory");
	let release = scenario_root.join("release");
	let main = scenario_root.join("main");
	let head = scenario_root.join("head");
	let tempdir = tempfile::TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	copy_directory(&release, root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "release"]);
	git(root, &["branch", "-M", "main"]);
	if !first_release {
		git(root, &["tag", "v1.0.0"]);
	}

	copy_directory(&main, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "main evolution"]);
	git(root, &["checkout", "-b", "feature"]);

	copy_directory(&head, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "head evolution"]);

	tempdir
}

fn mock_missing_publish_versions(server: &MockServer) {
	server.mock(|when, then| {
		when.method(GET).path("/crates/core");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": []
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/packages/dart_pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": []
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/@scope/jsr-pkg/meta.json");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": {}
		}));
	});
}

#[test]
fn validate_cli_succeeds_for_valid_workspace() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("monochange/validate-workspace");
	assert_cmd_snapshot!(
		monochange_command(None)
			.current_dir(tempdir.path())
			.arg("validate")
	);
}

#[test]
fn placeholder_publish_dry_run_reports_manual_registry_trust_contexts() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/manual-trust-diagnostics");
	let server = MockServer::start();
	mock_missing_publish_versions(&server);
	assert_cmd_snapshot!(
		monochange_command(None)
			.current_dir(tempdir.path())
			.env(
				"GITHUB_WORKFLOW_REF",
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main"
			)
			.env("GITHUB_JOB", "release")
			.env("MONOCHANGE_CRATES_IO_API_URL", server.base_url())
			.env("MONOCHANGE_PUB_DEV_API_URL", server.base_url())
			.env("MONOCHANGE_JSR_BASE_URL", server.base_url())
			.arg("placeholder-publish")
			.arg("--dry-run")
			.arg("--format")
			.arg("text")
	);
}

#[test]
fn placeholder_publish_dry_run_reports_missing_manual_registry_workflow_configuration() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/manual-trust-diagnostics");
	let server = MockServer::start();
	mock_missing_publish_versions(&server);
	assert_cmd_snapshot!(
		monochange_command(None)
			.current_dir(tempdir.path())
			.env_remove("GITHUB_WORKFLOW_REF")
			.env_remove("GITHUB_JOB")
			.env("MONOCHANGE_CRATES_IO_API_URL", server.base_url())
			.env("MONOCHANGE_PUB_DEV_API_URL", server.base_url())
			.env("MONOCHANGE_JSR_BASE_URL", server.base_url())
			.arg("placeholder-publish")
			.arg("--dry-run")
			.arg("--format")
			.arg("json")
	);
}

#[test]
fn change_cli_help_documents_package_and_group_targeting_rules() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(monochange_command(None).arg("change").arg("--help"));
}

#[test]
fn discover_cli_json_reports_relative_paths_and_stable_ids() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/discover-mixed");
	assert_cmd_snapshot!(
		monochange_command(None)
			.current_dir(tempdir.path())
			.arg("discover")
			.arg("--format")
			.arg("json")
	);
}

#[test]
fn analyze_cli_text_defaults_to_group_release_tag_for_selected_package() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_analyze_cli_repo(false);
	assert_cmd_snapshot!(
		monochange_command(None)
			.current_dir(tempdir.path())
			.arg("analyze")
			.arg("--package")
			.arg("core")
	);
}

#[test]
fn analyze_cli_json_falls_back_to_main_head_for_first_release_packages() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_analyze_cli_repo(true);
	assert_cmd_snapshot!(
		monochange_command(None)
			.current_dir(tempdir.path())
			.arg("analyze")
			.arg("--package")
			.arg("core")
			.arg("--format")
			.arg("json")
	);
}

#[test]
fn change_cli_writes_requested_file_contents() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/ungrouped-basic");
	let output_path = tempdir.path().join("feature.md");

	assert_cmd_snapshot!(
		monochange_command(None)
			.current_dir(tempdir.path())
			.arg("change")
			.arg("--package")
			.arg("core")
			.arg("--bump")
			.arg("minor")
			.arg("--reason")
			.arg("document cli snapshots")
			.arg("--output")
			.arg(&output_path)
	);

	let change_file =
		fs::read_to_string(&output_path).unwrap_or_else(|error| panic!("change file: {error}"));
	assert_snapshot!("change_file", change_file);
}

#[test]
fn change_cli_writes_explicit_versions_when_requested() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/ungrouped-basic");
	let output_path = tempdir.path().join("versioned.md");

	assert_cmd_snapshot!(
		monochange_command(None)
			.current_dir(tempdir.path())
			.arg("change")
			.arg("--package")
			.arg("core")
			.arg("--bump")
			.arg("major")
			.arg("--version")
			.arg("2.0.0")
			.arg("--reason")
			.arg("promote to stable")
			.arg("--output")
			.arg(&output_path)
	);

	let change_file =
		fs::read_to_string(&output_path).unwrap_or_else(|error| panic!("change file: {error}"));
	assert_snapshot!("change_file", change_file);
}

#[test]
fn release_dry_run_cli_defaults_to_markdown_output() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/ungrouped-basic");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("release")
			.arg("--dry-run")
	);
}

#[test]
fn release_dry_run_cli_patches_parent_packages_when_dependencies_change() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/ungrouped-basic");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("release")
			.arg("--dry-run")
			.arg("--format")
			.arg("text")
	);
}

#[test]
fn release_dry_run_cli_uses_explicit_group_versions_from_member_changes() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/group-explicit-version");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("release")
			.arg("--dry-run")
			.arg("--format")
			.arg("text")
	);
}

#[test]
fn release_dry_run_cli_json_exposes_group_owned_release_targets() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("release")
			.arg("--dry-run")
			.arg("--format")
			.arg("json")
	);
}

#[test]
fn versions_cli_text_reports_planned_versions_without_mutating_workspace() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	let before_manifest = fs::read_to_string(tempdir.path().join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("workspace manifest before versions: {error}"));
	let before_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("changelog before versions: {error}"));
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("versions")
			.arg("--format")
			.arg("text")
	);
	assert_eq!(
		fs::read_to_string(tempdir.path().join("Cargo.toml"))
			.unwrap_or_else(|error| panic!("workspace manifest after versions: {error}")),
		before_manifest
	);
	assert_eq!(
		fs::read_to_string(tempdir.path().join("changelog.md"))
			.unwrap_or_else(|error| panic!("changelog after versions: {error}")),
		before_changelog
	);
}

#[test]
fn versions_cli_markdown_reports_planned_versions() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("versions")
			.arg("--format")
			.arg("markdown")
	);
}

#[test]
fn versions_cli_json_reports_planned_versions() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("versions")
			.arg("--format")
			.arg("json")
	);
}

#[test]
fn release_dry_run_cli_text_renders_diff_preview() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("release")
			.arg("--dry-run")
			.arg("--diff")
			.arg("--format")
			.arg("text")
	);
}

#[test]
fn release_dry_run_cli_json_renders_diff_preview() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("release")
			.arg("--dry-run")
			.arg("--diff")
			.arg("--format")
			.arg("json")
	);
}

#[test]
fn verify_cli_json_reports_failure_comment() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/changeset-policy-no-changeset");
	assert_cmd_snapshot!(
		monochange_command(None)
			.current_dir(tempdir.path())
			.arg("affected")
			.arg("--format")
			.arg("json")
			.arg("--changed-paths")
			.arg("crates/core/src/lib.rs")
	);
}

#[test]
fn release_pr_workflow_reports_dry_run_pull_request_preview() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/release-pr-workflow");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("release-pr")
			.arg("--dry-run")
	);
}

#[test]
fn prepare_release_writes_manifest_json() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/release-manifest-workflow");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("release")
			.arg("--dry-run")
	);

	let manifest_path = tempdir.path().join(".monochange/release-manifest.json");
	let manifest = fs::read_to_string(&manifest_path)
		.unwrap_or_else(|error| panic!("read manifest {}: {error}", manifest_path.display()));
	assert_snapshot!("manifest", manifest);
}

#[test]
fn release_cli_reports_missing_changesets_cleanly() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/ungrouped-no-changeset");
	assert_cmd_snapshot!(
		release_cli_command()
			.current_dir(tempdir.path())
			.arg("release")
	);
}

#[test]
fn release_cli_writes_group_changelog_and_skips_packages_without_changelogs() {
	let tempdir = setup_scenario_workspace("cli-output/group-basic");

	let output = release_cli_command()
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let root_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let workspace_manifest = fs::read_to_string(tempdir.path().join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("workspace manifest: {error}"));
	let group_versioned_file = fs::read_to_string(tempdir.path().join("group.toml"))
		.unwrap_or_else(|error| panic!("group versioned file: {error}"));

	assert!(root_changelog.contains("Grouped release for `sdk`."));
	assert!(root_changelog.contains("Changed members: core"));
	assert!(root_changelog.contains("Synchronized members: app"));
	assert!(core_changelog.contains("## 1.1.0"));
	assert!(core_changelog.contains("add feature"));
	assert!(!tempdir.path().join("crates/app/CHANGELOG.md").exists());
	assert!(!tempdir.path().join("crates/app/changelog.md").exists());
	assert!(workspace_manifest.contains("version = \"1.1.0\""));
	assert!(group_versioned_file.contains("version = \"1.1.0\""));
}

#[test]
fn release_quiet_suppresses_output_and_skips_workspace_mutation() {
	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	let before_root_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog before quiet release: {error}"));
	let before_workspace_manifest = fs::read_to_string(tempdir.path().join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("workspace manifest before quiet release: {error}"));

	let output = release_cli_command()
		.current_dir(tempdir.path())
		.arg("--quiet")
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("quiet release output: {error}"));
	assert!(output.status.success(), "quiet release failed unexpectedly");
	assert!(
		output.stdout.is_empty(),
		"quiet release should suppress stdout"
	);
	assert!(
		output.stderr.is_empty(),
		"quiet release should suppress stderr"
	);

	let after_root_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog after quiet release: {error}"));
	let after_workspace_manifest = fs::read_to_string(tempdir.path().join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("workspace manifest after quiet release: {error}"));

	assert_eq!(before_root_changelog, after_root_changelog);
	assert_eq!(before_workspace_manifest, after_workspace_manifest);
}

#[test]
fn validate_cli_rejects_packages_in_multiple_groups() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/multiple-groups-validation");
	assert_cmd_snapshot!(
		monochange_command(None)
			.current_dir(tempdir.path())
			.arg("validate")
	);
}

#[test]
fn validate_cli_reports_frontmatter_location_and_fix_hint() {
	let tempdir = setup_scenario_workspace("changeset-target-metadata/render-workspace");
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[package.\"@monochange/skill\"]\npath = \"crates/core\"\ntype = \"cargo\"\n",
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	fs::create_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("mkdir changeset dir: {error}"));
	fs::write(
		tempdir.path().join(".changeset/invalid.md"),
		"---\n@monochange/skill: patch\n---\n\n# broken\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));

	let output = monochange_command(None)
		.current_dir(tempdir.path())
		.arg("validate")
		.output()
		.unwrap_or_else(|error| panic!("validate output: {error}"));
	assert!(!output.status.success());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("-->"), "stderr: {stderr}");
	assert!(
		stderr.contains(".changeset/invalid.md:2:1"),
		"stderr: {stderr}"
	);
	assert!(
		stderr.contains("2 | @monochange/skill: patch"),
		"stderr: {stderr}"
	);
	assert!(
		stderr.contains("wrap package or group ids that contain characters like `@`, `/`, `:`, or spaces in double quotes"),
		"stderr: {stderr}"
	);
}
