use std::fs;
use std::path::Path;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use rstest::rstest;
use serde_json::Value;

mod test_support;
use test_support::current_test_name;
use test_support::monochange_command;
use test_support::setup_scenario_workspace;
use test_support::snapshot_settings;

#[test]
fn validate_step_runs_without_input_overrides() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-step-input-overrides/workspace");
	let output = run_command(tempdir.path(), "validate");
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_snapshot!(String::from_utf8_lossy(&output.stdout));
}

#[rstest]
#[case::discover_workspace("cli-step-input-overrides/workspace", "discover")]
#[case::prepare_release("cli-step-input-overrides/workspace", "release")]
#[case::affected_packages("cli-step-input-overrides/workspace", "affected")]
#[case::publish_release("cli-step-input-overrides/source-github", "publish-release")]
#[case::open_release_request("cli-step-input-overrides/source-github", "release-pr")]
#[case::comment_released_issues("cli-step-input-overrides/source-github", "release-comments")]
fn cli_step_override_json_scenarios_match_snapshot(#[case] fixture: &str, #[case] command: &str) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace(fixture);
	let json = run_json_command(tempdir.path(), command);
	assert_json_snapshot!(json);
}

#[test]
fn create_change_file_step_can_hardcode_inputs_without_cli_inputs() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-step-input-overrides/workspace");
	let output = run_command(tempdir.path(), "change");
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_snapshot!("stdout", String::from_utf8_lossy(&output.stdout));
	let contents = fs::read_to_string(tempdir.path().join(".changeset/hardcoded.md"))
		.unwrap_or_else(|error| panic!("hardcoded change file: {error}"));
	assert_snapshot!("change_file", contents);
}

#[test]
fn command_step_can_hardcode_inputs_without_cli_inputs() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("cli-step-input-overrides/workspace");
	let output = run_command_without_dry_run(tempdir.path(), "announce");
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_snapshot!("stdout", String::from_utf8_lossy(&output.stdout));
	let contents = fs::read_to_string(tempdir.path().join("command-output.txt"))
		.unwrap_or_else(|error| panic!("command output file: {error}"));
	assert_snapshot!("command_output", contents);
}

fn run_command(root: &Path, command: &str) -> std::process::Output {
	monochange_command(None)
		.current_dir(root)
		.arg(command)
		.arg("--dry-run")
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"))
}

fn run_command_without_dry_run(root: &Path, command: &str) -> std::process::Output {
	monochange_command(None)
		.current_dir(root)
		.arg(command)
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"))
}

fn run_json_command(root: &Path, command: &str) -> Value {
	let output = run_command(root, command);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!(
			"parse json output: {error}; stdout: {}",
			String::from_utf8_lossy(&output.stdout)
		)
	})
}
