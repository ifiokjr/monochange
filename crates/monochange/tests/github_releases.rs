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
fn publish_github_release_dry_run_renders_group_release_payloads() {
	let _snapshot = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("github-releases/group");
	copy_directory(&fixture_root, tempdir.path());

	let json = run_json_workflow(tempdir.path(), "publish-release");
	let github_releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected github releases array"));
	assert_json_snapshot!(
		"publish_github_release_dry_run_renders_group_release_payloads__release",
		json_subset(
			&json,
			&[
				("repository", "/releases/0/repository"),
				("targetId", "/releases/0/targetId"),
				("targetKind", "/releases/0/targetKind"),
				("tagName", "/releases/0/tagName"),
				("name", "/releases/0/name"),
				("command", "/manifest/command"),
				("version", "/manifest/version"),
				("groupVersion", "/manifest/groupVersion"),
				("releaseTargetId", "/manifest/releaseTargets/0/id"),
				("releaseTargetKind", "/manifest/releaseTargets/0/kind"),
				("firstChangelogOwnerId", "/manifest/changelogs/0/ownerId"),
				("secondChangelogOwnerId", "/manifest/changelogs/1/ownerId"),
			]
		)
	);
	assert_snapshot!(
		"publish_github_release_dry_run_renders_group_release_payloads__body",
		github_releases[0]["body"]
			.as_str()
			.unwrap_or_else(|| panic!("expected GitHub release body"))
	);
}

#[test]
fn publish_github_release_dry_run_renders_package_release_payloads() {
	let _snapshot = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("github-releases/ungrouped");
	copy_directory(&fixture_root, tempdir.path());

	let json = run_json_workflow(tempdir.path(), "publish-release");
	let github_releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected github releases array"));
	assert_json_snapshot!(
		"publish_github_release_dry_run_renders_package_release_payloads__release_manifest_subset",
		json_subset(
			&json,
			&[
				("firstTargetId", "/releases/0/targetId"),
				("firstTagName", "/releases/0/tagName"),
				("firstDraft", "/releases/0/draft"),
				("firstPrerelease", "/releases/0/prerelease"),
				("secondTargetId", "/releases/1/targetId"),
				("secondTagName", "/releases/1/tagName"),
				("version", "/manifest/version"),
				("groupVersion", "/manifest/groupVersion"),
				("firstReleaseTargetId", "/manifest/releaseTargets/0/id"),
				("secondReleaseTargetId", "/manifest/releaseTargets/1/id"),
				("firstChangelogOwnerId", "/manifest/changelogs/0/ownerId"),
				("secondChangelogOwnerId", "/manifest/changelogs/1/ownerId"),
				("firstDecisionTrigger", "/manifest/plan/decisions/0/trigger"),
			]
		)
	);
	assert_snapshot!(
		"publish_github_release_dry_run_renders_package_release_payloads__core_body",
		github_releases[1]["body"]
			.as_str()
			.unwrap_or_else(|| panic!("expected GitHub release body"))
	);
}

#[test]
fn publish_github_release_dry_run_supports_custom_sections_and_templates() {
	let _snapshot = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("github-releases/custom-sections");
	copy_directory(&fixture_root, tempdir.path());

	let json = run_json_workflow(tempdir.path(), "publish-release");
	let github_releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected github releases array"));
	assert_eq!(github_releases.len(), 1);
	assert_snapshot!(
		"publish_github_release_dry_run_supports_custom_sections_and_templates__body",
		github_releases[0]["body"]
			.as_str()
			.unwrap_or_else(|| panic!("expected GitHub release body"))
	);
	assert_json_snapshot!(
		"publish_github_release_dry_run_supports_custom_sections_and_templates__manifest_subset",
		json_subset(
			&json,
			&[
				("ownerId", "/manifest/changelogs/0/ownerId"),
				(
					"sectionTitle",
					"/manifest/changelogs/0/notes/sections/0/title"
				),
				(
					"firstEntry",
					"/manifest/changelogs/0/notes/sections/0/entries/0"
				),
			]
		)
	);
}

#[test]
fn publish_github_release_dry_run_inherits_custom_sections_from_defaults() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("github-releases/default-custom-sections");
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
