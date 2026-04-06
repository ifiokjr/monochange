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
	command.env("MONOCHANGE_RELEASE_DATE", "2026-04-06");
	command
}

#[test]
fn open_release_pull_request_dry_run_renders_group_release_preview() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("release-pr/group");
	copy_directory(&fixture_root, tempdir.path());

	let json = run_json_workflow(tempdir.path(), "release-pr");
	let pull_request = &json["releaseRequest"];
	assert_eq!(pull_request["repository"], "ifiokjr/monochange");
	assert_eq!(pull_request["baseBranch"], "main");
	assert_eq!(pull_request["headBranch"], "monochange/release/release-pr");
	assert_eq!(pull_request["title"], "chore(release): prepare release");
	assert_eq!(pull_request["labels"][0], "release");
	assert_eq!(pull_request["labels"][1], "automated");
	assert!(pull_request["body"]
		.as_str()
		.is_some_and(|text| text.contains("### sdk 1.1.0")));
	assert!(pull_request["body"]
		.as_str()
		.is_some_and(|text| text.contains("#### Features")));

	let manifest = &json["manifest"];
	assert_eq!(manifest["command"], "release-pr");
	assert_eq!(manifest["releaseTargets"][0]["id"], "sdk");
}

#[test]
fn open_release_pull_request_dry_run_renders_package_release_preview() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("release-pr/ungrouped");
	copy_directory(&fixture_root, tempdir.path());

	let json = run_json_workflow(tempdir.path(), "release-pr");
	let pull_request = &json["releaseRequest"];
	assert_eq!(pull_request["baseBranch"], "develop");
	assert_eq!(pull_request["headBranch"], "automation/release/release-pr");
	assert_eq!(pull_request["autoMerge"], true);
	let body = pull_request["body"]
		.as_str()
		.unwrap_or_else(|| panic!("expected release request body"));
	assert!(body.contains("### app 1.0.1"));
	assert!(body.contains("### core 1.1.0"));
	assert!(body.contains(
		"No package-specific changes were recorded; `workflow-app` was updated to 1.0.1."
	));
	assert!(body.contains("add feature"));

	let manifest = &json["manifest"];
	assert!(manifest["version"].is_null());
	assert_eq!(manifest["releaseTargets"][0]["id"], "app");
	assert_eq!(manifest["releaseTargets"][1]["id"], "core");
}

fn run_json_workflow(root: &Path, workflow: &str) -> Value {
	let output = cli()
		.current_dir(root)
		.arg(workflow)
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("workflow output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json output: {error}"))
}
