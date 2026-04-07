use std::fs;
use std::process::Command;

use insta::{assert_json_snapshot, assert_snapshot};
use insta_cmd::get_cargo_bin;
use serde_json::Value;
use tempfile::tempdir;

mod test_support;
use test_support::{copy_directory, fixture_path, snapshot_settings};

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command.env("MONOCHANGE_RELEASE_DATE", "2026-04-06");
	command
}

#[test]
fn ungrouped_transitive_bump_writes_empty_update_message_to_dependent_changelog() {
	let _snapshot = snapshot_settings().bind_to_scope();
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

	assert_snapshot!(
		"ungrouped_transitive_bump_writes_empty_update_message_to_dependent_changelog__core",
		core_changelog
	);
	assert_snapshot!(
		"ungrouped_transitive_bump_writes_empty_update_message_to_dependent_changelog__app",
		app_changelog
	);
}

#[test]
fn ungrouped_transitive_bump_with_parent_bump_minor_escalates_dependent_version() {
	let _snapshot = snapshot_settings().bind_to_scope();
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

	assert_json_snapshot!(
		"ungrouped_transitive_bump_with_parent_bump_minor_escalates_dependent_version__app_decision",
		app_decision
	);
}

#[test]
fn grouped_transitive_bump_writes_empty_update_message_with_group_reference() {
	let _snapshot = snapshot_settings().bind_to_scope();
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

	assert_snapshot!(
		"grouped_transitive_bump_writes_empty_update_message_with_group_reference__app",
		app_changelog
	);
	assert_snapshot!(
		"grouped_transitive_bump_writes_empty_update_message_with_group_reference__group",
		group_changelog
	);
}

#[test]
fn custom_empty_update_message_on_package_overrides_default() {
	let _snapshot = snapshot_settings().bind_to_scope();
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
	assert_snapshot!(
		"custom_empty_update_message_on_package_overrides_default__changelog",
		changelog
	);
}
