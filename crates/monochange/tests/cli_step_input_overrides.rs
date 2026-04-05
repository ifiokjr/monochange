use std::fs;
use std::path::Path;
use std::process::Command;

use insta_cmd::get_cargo_bin;
use serde_json::Value;
use tempfile::tempdir;

mod test_support;
use test_support::{copy_directory, fixture_path};

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command
}

#[test]
fn validate_step_runs_without_input_overrides() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-step-input-overrides/workspace"),
		tempdir.path(),
	);

	let output = run_command(tempdir.path(), "validate");
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(String::from_utf8_lossy(&output.stdout).contains("workspace validation passed"));
}

#[test]
fn discover_step_can_hardcode_format_without_cli_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-step-input-overrides/workspace"),
		tempdir.path(),
	);

	let json = run_json_command(tempdir.path(), "discover");
	let packages = json["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("expected packages array"));
	assert_eq!(packages.len(), 2);
	assert_eq!(packages[0]["name"], "workflow-app");
	assert_eq!(packages[1]["name"], "workflow-core");
}

#[test]
fn create_change_file_step_can_hardcode_inputs_without_cli_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-step-input-overrides/workspace"),
		tempdir.path(),
	);

	let output = run_command(tempdir.path(), "change");
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let stdout = String::from_utf8_lossy(&output.stdout);
	assert!(stdout.contains("wrote change file .changeset/hardcoded.md"));
	let contents = fs::read_to_string(tempdir.path().join(".changeset/hardcoded.md"))
		.unwrap_or_else(|error| panic!("hardcoded change file: {error}"));
	assert!(contents.contains("core: minor"));
	assert!(contents.contains("#### hardcoded change"));
}

#[test]
fn prepare_release_step_can_hardcode_format_without_cli_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-step-input-overrides/workspace"),
		tempdir.path(),
	);

	let json = run_json_command(tempdir.path(), "release");
	assert_eq!(json["command"], "release");
	assert_eq!(json["releaseTargets"][0]["id"], "sdk");
}

#[test]
fn render_release_manifest_step_can_hardcode_format_without_cli_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-step-input-overrides/workspace"),
		tempdir.path(),
	);

	let json = run_json_command(tempdir.path(), "release-manifest");
	assert_eq!(json["command"], "release-manifest");
	let manifest_path = tempdir.path().join(".monochange/release-manifest.json");
	assert!(
		manifest_path.exists(),
		"expected {} to exist",
		manifest_path.display()
	);
}

#[test]
fn affected_packages_step_can_hardcode_inputs_without_cli_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-step-input-overrides/workspace"),
		tempdir.path(),
	);

	let json = run_json_command(tempdir.path(), "affected");
	assert_eq!(json["status"], "skipped");
	assert_eq!(json["matchedSkipLabels"][0], "no-changeset-required");
}

#[test]
fn command_step_can_hardcode_inputs_without_cli_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-step-input-overrides/workspace"),
		tempdir.path(),
	);

	let output = run_command_without_dry_run(tempdir.path(), "announce");
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let contents = fs::read_to_string(tempdir.path().join("command-output.txt"))
		.unwrap_or_else(|error| panic!("command output file: {error}"));
	assert_eq!(contents, "hardcoded-message");
}

#[test]
fn publish_release_step_can_hardcode_format_without_cli_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-step-input-overrides/source-github"),
		tempdir.path(),
	);

	let json = run_json_command(tempdir.path(), "publish-release");
	assert_eq!(json["manifest"]["command"], "publish-release");
	assert_eq!(json["releases"][0]["provider"], "github");
}

#[test]
fn open_release_request_step_can_hardcode_format_without_cli_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-step-input-overrides/source-github"),
		tempdir.path(),
	);

	let json = run_json_command(tempdir.path(), "release-pr");
	assert_eq!(json["manifest"]["command"], "release-pr");
	assert_eq!(json["releaseRequest"]["provider"], "github");
}

#[test]
fn comment_released_issues_step_can_hardcode_format_without_cli_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-step-input-overrides/source-github"),
		tempdir.path(),
	);

	let json = run_json_command(tempdir.path(), "release-comments");
	assert_eq!(json["command"], "release-comments");
	assert_eq!(json["releaseTargets"][0]["id"], "core");
}

fn run_command(root: &Path, command: &str) -> std::process::Output {
	cli()
		.current_dir(root)
		.arg(command)
		.arg("--dry-run")
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"))
}

fn run_command_without_dry_run(root: &Path, command: &str) -> std::process::Output {
	cli()
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
