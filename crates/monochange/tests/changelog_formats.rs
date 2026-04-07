use std::fs;
use std::process::Command;

use insta::assert_snapshot;
use insta_cmd::get_cargo_bin;
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
fn release_uses_keep_a_changelog_format_from_defaults() {
	let _snapshot = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/defaults-keep-a");
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
	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert_snapshot!(
		"release_uses_keep_a_changelog_format_from_defaults__core",
		core_changelog
	);
	assert_snapshot!(
		"release_uses_keep_a_changelog_format_from_defaults__app",
		app_changelog
	);
	assert_snapshot!(
		"release_uses_keep_a_changelog_format_from_defaults__group",
		group_changelog
	);
}

#[test]
fn release_allows_package_and_group_changelog_format_overrides() {
	let _snapshot = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/defaults-then-package-override");
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
	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert_snapshot!(
		"release_allows_package_and_group_changelog_format_overrides__core",
		core_changelog
	);
	assert_snapshot!(
		"release_allows_package_and_group_changelog_format_overrides__app",
		app_changelog
	);
	assert_snapshot!(
		"release_allows_package_and_group_changelog_format_overrides__group",
		group_changelog
	);
}

#[test]
fn release_uses_alert_syntax_for_group_entries_with_multiline_content() {
	let _snapshot = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/alert-multiline");
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

	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	assert_snapshot!(
		"release_uses_alert_syntax_for_group_entries_with_multiline_content__group",
		group_changelog
	);
}

#[test]
fn release_filters_group_changelog_entries_to_selected_member_packages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/group-include-selected");
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
	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(core_changelog.contains("#### add cli feature"));
	assert!(app_changelog.contains("#### document internal sync work"));
	assert!(group_changelog.contains("Changed members: core, app"));
	assert!(group_changelog.contains("> [!NOTE]"));
	assert!(group_changelog.contains("> *core*"));
	assert!(group_changelog.contains("#### add cli feature"));
	assert!(!group_changelog.contains("document internal sync work"));
}

#[test]
fn release_renders_group_fallback_when_member_notes_are_filtered_out() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/group-include-group-only");
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

	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(group_changelog.contains("Changed members: core"));
	assert!(group_changelog.contains("Synchronized members: app"));
	assert!(group_changelog.contains("No group-facing notes were recorded for this release."));
	assert!(!group_changelog.contains("- **core**: add hidden internal change"));
}

#[test]
fn release_keeps_direct_group_targeted_notes_even_when_group_include_is_group_only() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/group-include-group-note");
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

	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(group_changelog.contains("> [!NOTE]"));
	assert!(group_changelog.contains("> *sdk*"));
	assert!(group_changelog.contains("#### highlight the grouped release"));
	assert!(!group_changelog.contains("member note should stay package-only"));
}

#[test]
fn release_excludes_allowlisted_group_notes_when_a_changeset_targets_disallowed_members_too() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/group-include-multi-target-blocked");
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

	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(group_changelog.contains("Changed members: core, app"));
	assert!(group_changelog.contains("No group-facing notes were recorded for this release."));
	assert!(!group_changelog.contains("add shared release note"));
}

#[test]
fn release_uses_alert_syntax_for_group_entries_with_multiple_packages_in_one_changeset() {
	let _snapshot = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/alert-multi-packages");
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

	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	assert_snapshot!(
		"release_uses_alert_syntax_for_group_entries_with_multiple_packages_in_one_changeset__group",
		group_changelog
	);
}
