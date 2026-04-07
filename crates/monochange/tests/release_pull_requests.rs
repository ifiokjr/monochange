use std::path::Path;
use std::process::Command;

use insta::{assert_json_snapshot, assert_snapshot};
use insta_cmd::get_cargo_bin;
use serde_json::Value;
use tempfile::tempdir;

mod test_support;
use test_support::{copy_directory, fixture_path, json_subset, snapshot_settings};

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command.env("MONOCHANGE_RELEASE_DATE", "2026-04-06");
	command
}

#[test]
fn open_release_pull_request_dry_run_renders_group_release_preview() {
	let _snapshot = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("release-pr/group");
	copy_directory(&fixture_root, tempdir.path());

	let json = run_json_workflow(tempdir.path(), "release-pr");
	let pull_request = &json["releaseRequest"];
	assert_json_snapshot!(
		"open_release_pull_request_dry_run_renders_group_release_preview__release_request",
		json_subset(
			&json,
			&[
				("repository", "/releaseRequest/repository"),
				("baseBranch", "/releaseRequest/baseBranch"),
				("headBranch", "/releaseRequest/headBranch"),
				("title", "/releaseRequest/title"),
				("commitMessage", "/releaseRequest/commitMessage"),
				("labels", "/releaseRequest/labels"),
				("releaseTargetId", "/manifest/releaseTargets/0/id"),
			]
		)
	);
	assert_snapshot!(
		"open_release_pull_request_dry_run_renders_group_release_preview__body",
		pull_request["body"]
			.as_str()
			.unwrap_or_else(|| panic!("expected release request body"))
	);
}

#[test]
fn open_release_pull_request_dry_run_renders_package_release_preview() {
	let _snapshot = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("release-pr/ungrouped");
	copy_directory(&fixture_root, tempdir.path());

	let json = run_json_workflow(tempdir.path(), "release-pr");
	let pull_request = &json["releaseRequest"];
	assert_json_snapshot!(
		"open_release_pull_request_dry_run_renders_package_release_preview__release_request",
		json_subset(
			&json,
			&[
				("baseBranch", "/releaseRequest/baseBranch"),
				("headBranch", "/releaseRequest/headBranch"),
				("autoMerge", "/releaseRequest/autoMerge"),
				("commitMessage", "/releaseRequest/commitMessage"),
				("version", "/manifest/version"),
				("firstTargetId", "/manifest/releaseTargets/0/id"),
				("secondTargetId", "/manifest/releaseTargets/1/id"),
			]
		)
	);
	assert_snapshot!(
		"open_release_pull_request_dry_run_renders_package_release_preview__body",
		pull_request["body"]
			.as_str()
			.unwrap_or_else(|| panic!("expected release request body"))
	);
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
