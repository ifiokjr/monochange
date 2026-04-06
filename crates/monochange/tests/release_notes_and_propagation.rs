use std::fs;

use serde_json::Value;
use tempfile::tempdir;

use insta_cmd::get_cargo_bin;
use std::process::Command;

mod test_support;
use test_support::{copy_directory, fixture_path};

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command.env("MONOCHANGE_RELEASE_DATE", "2026-04-06");
	command
}

#[test]
fn ungrouped_transitive_bump_writes_empty_update_message_to_dependent_changelog() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("release-notes-and-propagation/ungrouped-patch");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));

	assert!(core_changelog.contains("1.1.0 (2026-04-06)"));
	assert!(core_changelog.contains("- add core feature"));
	assert!(app_changelog.contains("1.0.1 (2026-04-06)"));
	assert!(app_changelog.contains(
		"No package-specific changes were recorded; `workflow-app` was updated to 1.0.1."
	));
}

#[test]
fn ungrouped_transitive_bump_with_parent_bump_minor_escalates_dependent_version() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("release-notes-and-propagation/ungrouped-minor");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let json = serde_json::from_slice::<Value>(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json: {error}"));
	let app_decision = json["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("decisions array"))
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains("app"))
		})
		.unwrap_or_else(|| panic!("expected app decision"));

	assert_eq!(app_decision["bump"], "minor");
	assert_eq!(app_decision["trigger"], "transitive-dependency");
	assert_eq!(app_decision["plannedVersion"], "1.1.0");
}

#[test]
fn grouped_transitive_bump_writes_empty_update_message_with_group_reference() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("release-notes-and-propagation/grouped-default");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(app_changelog.contains("1.1.0 (2026-04-06)"));
	assert!(app_changelog.contains(
		"No package-specific changes were recorded; `workflow-app` was updated to 1.1.0 as part of group `sdk`."
	));
	assert!(group_changelog.contains("1.1.0 (2026-04-06)"));
	assert!(group_changelog.contains("Grouped release for `sdk`."));
	assert!(group_changelog.contains("- **core**: add core feature"));
}

#[test]
fn custom_empty_update_message_on_package_overrides_default() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root =
		fixture_path("release-notes-and-propagation/ungrouped-custom-package-message");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("package changelog: {error}"));
	assert!(changelog.contains("1.1.0 (2026-04-06)"));
	assert!(changelog.contains("This entry appears in changelog via `group.sdk.changelog.text`"));
	assert!(changelog.contains("### Features"));
	assert!(changelog.contains("- **core**: add core feature"));
	assert!(changelog.contains(
		"No changes were recorded for some group members; as a result, package changelogs were synchronized to version 1.1.0."
	));
}
