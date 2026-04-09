use std::fs;

use insta::assert_snapshot;
use insta_cmd::assert_cmd_snapshot;

mod test_support;
use test_support::{
	current_test_name, monochange_command, setup_scenario_workspace, snapshot_settings,
};

fn release_cli_command() -> std::process::Command {
	monochange_command(Some("2026-04-06"))
}

#[test]
fn validate_cli_succeeds_for_valid_workspace() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("monochange/validate-workspace");
	assert_cmd_snapshot!(monochange_command(None)
		.current_dir(tempdir.path())
		.arg("validate"));
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
	assert_cmd_snapshot!(monochange_command(None)
		.current_dir(tempdir.path())
		.arg("discover")
		.arg("--format")
		.arg("json"));
}

#[test]
fn change_cli_writes_requested_file_contents() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/ungrouped-basic");
	let output_path = tempdir.path().join("feature.md");

	assert_cmd_snapshot!(monochange_command(None)
		.current_dir(tempdir.path())
		.arg("change")
		.arg("--package")
		.arg("core")
		.arg("--bump")
		.arg("minor")
		.arg("--reason")
		.arg("document cli snapshots")
		.arg("--output")
		.arg(&output_path));

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

	assert_cmd_snapshot!(monochange_command(None)
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
		.arg(&output_path));

	let change_file =
		fs::read_to_string(&output_path).unwrap_or_else(|error| panic!("change file: {error}"));
	assert_snapshot!("change_file", change_file);
}

#[test]
fn release_dry_run_cli_patches_parent_packages_when_dependencies_change() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/ungrouped-basic");
	assert_cmd_snapshot!(release_cli_command()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("text"));
}

#[test]
fn release_dry_run_cli_uses_explicit_group_versions_from_member_changes() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/group-explicit-version");
	assert_cmd_snapshot!(release_cli_command()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("text"));
}

#[test]
fn release_dry_run_cli_json_exposes_group_owned_release_targets() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	assert_cmd_snapshot!(release_cli_command()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json"));
}

#[test]
fn verify_cli_json_reports_failure_comment() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/changeset-policy-no-changeset");
	assert_cmd_snapshot!(monochange_command(None)
		.current_dir(tempdir.path())
		.arg("affected")
		.arg("--format")
		.arg("json")
		.arg("--changed-paths")
		.arg("crates/core/src/lib.rs"));
}

#[test]
fn release_pr_workflow_reports_dry_run_pull_request_preview() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/release-pr-workflow");
	assert_cmd_snapshot!(release_cli_command()
		.current_dir(tempdir.path())
		.arg("release-pr")
		.arg("--dry-run"));
}

#[test]
fn release_manifest_workflow_writes_manifest_json() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/release-manifest-workflow");
	assert_cmd_snapshot!(release_cli_command()
		.current_dir(tempdir.path())
		.arg("release-manifest")
		.arg("--dry-run"));

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
	assert_cmd_snapshot!(release_cli_command()
		.current_dir(tempdir.path())
		.arg("release"));
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
	assert!(core_changelog.contains("- add feature"));
	assert!(!tempdir.path().join("crates/app/CHANGELOG.md").exists());
	assert!(!tempdir.path().join("crates/app/changelog.md").exists());
	assert!(workspace_manifest.contains("version = \"1.1.0\""));
	assert!(group_versioned_file.contains("version = \"1.1.0\""));
}

#[test]
fn validate_cli_rejects_packages_in_multiple_groups() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-output/multiple-groups-validation");
	assert_cmd_snapshot!(monochange_command(None)
		.current_dir(tempdir.path())
		.arg("validate"));
}
