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
fn publish_github_release_dry_run_renders_group_release_payloads() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("github-releases/group");
	copy_directory(&fixture_root, tempdir.path());

	let json = run_json_workflow(tempdir.path(), "publish-release");
	let github_releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected github releases array"));
	assert_eq!(github_releases.len(), 1);
	assert_eq!(github_releases[0]["repository"], "ifiokjr/monochange");
	assert_eq!(github_releases[0]["targetId"], "sdk");
	assert_eq!(github_releases[0]["targetKind"], "group");
	assert_eq!(github_releases[0]["tagName"], "v1.1.0");
	assert_eq!(github_releases[0]["name"], "sdk 1.1.0");
	assert!(github_releases[0]["body"]
		.as_str()
		.is_some_and(|text| text.contains("Grouped release for `sdk`.")));

	let manifest = &json["manifest"];
	assert_eq!(manifest["command"], "publish-release");
	assert_eq!(manifest["version"], "1.1.0");
	assert_eq!(manifest["groupVersion"], "1.1.0");
	assert_eq!(manifest["releaseTargets"][0]["id"], "sdk");
	assert_eq!(manifest["releaseTargets"][0]["kind"], "group");
	assert_eq!(manifest["changelogs"][0]["ownerId"], "sdk");
	assert_eq!(manifest["changelogs"][1]["ownerId"], "core");
}

#[test]
fn publish_github_release_dry_run_renders_package_release_payloads() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("github-releases/ungrouped");
	copy_directory(&fixture_root, tempdir.path());

	let json = run_json_workflow(tempdir.path(), "publish-release");
	let github_releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected github releases array"));
	assert_eq!(github_releases.len(), 2);
	assert_eq!(github_releases[0]["targetId"], "app");
	assert_eq!(github_releases[0]["tagName"], "app/v1.0.1");
	assert_eq!(github_releases[0]["draft"], true);
	assert_eq!(github_releases[0]["prerelease"], true);
	assert_eq!(github_releases[1]["targetId"], "core");
	assert_eq!(github_releases[1]["tagName"], "core/v1.1.0");
	assert!(github_releases[1]["body"]
		.as_str()
		.is_some_and(|text| text.contains("add feature")));

	let manifest = &json["manifest"];
	assert!(manifest["version"].is_null());
	assert_eq!(manifest["groupVersion"], Value::Null);
	assert_eq!(manifest["releaseTargets"][0]["id"], "app");
	assert_eq!(manifest["releaseTargets"][1]["id"], "core");
	assert_eq!(manifest["changelogs"][0]["ownerId"], "app");
	assert_eq!(manifest["changelogs"][1]["ownerId"], "core");
	assert_eq!(
		manifest["plan"]["decisions"][0]["trigger"],
		"transitive-dependency"
	);
}

#[test]
fn publish_github_release_dry_run_supports_custom_sections_and_templates() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("github-releases/custom-sections");
	copy_directory(&fixture_root, tempdir.path());

	let json = run_json_workflow(tempdir.path(), "publish-release");
	let github_releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected github releases array"));
	assert_eq!(github_releases.len(), 1);
	let body = github_releases[0]["body"]
		.as_str()
		.unwrap_or_else(|| panic!("expected GitHub release body"));
	assert!(body.contains("### Security"));
	assert!(body.contains("#### rotate signing keys (core patch)"));
	assert!(body.contains("Roll the signing key before the release window closes."));

	let manifest = &json["manifest"];
	assert_eq!(manifest["changelogs"][0]["ownerId"], "core");
	assert_eq!(
		manifest["changelogs"][0]["notes"]["sections"][0]["title"],
		"Security"
	);
	assert_eq!(
		manifest["changelogs"][0]["notes"]["sections"][0]["entries"][0],
		"#### rotate signing keys (core patch)\n\nRoll the signing key before the release window closes."
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
		.unwrap_or_else(|error| panic!("parse workflow json: {error}"))
}
