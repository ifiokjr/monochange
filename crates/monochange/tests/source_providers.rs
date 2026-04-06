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
fn publish_release_dry_run_supports_gitlab_sources() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path("source/gitlab"), tempdir.path());

	let json = run_json_command(tempdir.path(), "publish-release");
	let releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected releases array"));
	assert_eq!(releases.len(), 1);
	assert_eq!(releases[0]["provider"], "gitlab");
	assert_eq!(releases[0]["repository"], "group/monochange");
}

#[test]
fn release_pr_dry_run_supports_gitea_sources() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path("source/gitea"), tempdir.path());

	let json = run_json_command(tempdir.path(), "release-pr");
	let release_request = &json["releaseRequest"];
	assert_eq!(release_request["provider"], "gitea");
	assert_eq!(release_request["repository"], "org/monochange");
	assert_eq!(release_request["baseBranch"], "main");
}

#[test]
fn source_provider_fixtures_support_configured_commands() {
	for (fixture, command) in [
		("source/github", "publish-release"),
		("source/gitlab", "publish-release"),
		("source/gitea", "release-pr"),
	] {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		copy_directory(&fixture_path(fixture), tempdir.path());
		let json = run_json_command(tempdir.path(), command);
		assert!(json.is_object(), "expected json object output");
	}
}

#[test]
fn github_source_fixture_supports_release_pull_request_and_issue_comment_dry_runs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path("source/github"), tempdir.path());

	let publish_json = run_json_command(tempdir.path(), "publish-release");
	let releases = publish_json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected releases array"));
	assert_eq!(releases[0]["provider"], "github");
	assert_eq!(releases[0]["repository"], "ifiokjr/monochange");

	let pr_json = run_json_command(tempdir.path(), "release-pr");
	assert_eq!(pr_json["releaseRequest"]["provider"], "github");
	assert_eq!(
		pr_json["releaseRequest"]["repository"],
		"ifiokjr/monochange"
	);

	let comments_json = run_json_command(tempdir.path(), "release-comments");
	assert!(comments_json.is_object());
}

fn run_json_command(root: &Path, command: &str) -> Value {
	let output = cli()
		.current_dir(root)
		.arg(command)
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json output: {error}"))
}
