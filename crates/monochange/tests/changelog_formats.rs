use std::fs;

use insta::assert_snapshot;
use rstest::rstest;

mod test_support;
use test_support::current_test_name;
use test_support::monochange_command;
use test_support::setup_scenario_workspace;
use test_support::snapshot_settings;

#[rstest]
#[case::defaults_keep_a("defaults-keep-a")]
#[case::defaults_then_package_override("defaults-then-package-override")]
fn release_changelog_snapshots_match_expected_output(#[case] scenario: &str) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace(&format!("changelog-formats/{scenario}"));
	let output = monochange_command(Some("2026-04-06"))
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

	assert_snapshot!("core", core_changelog);
	assert_snapshot!("app", app_changelog);
	assert_snapshot!("group", group_changelog);
}

#[rstest]
#[case::alert_multiline("alert-multiline")]
#[case::alert_multi_packages("alert-multi-packages")]
fn release_group_alert_snapshots_match_expected_output(#[case] scenario: &str) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace(&format!("changelog-formats/{scenario}"));
	let output = monochange_command(Some("2026-04-06"))
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
	assert_snapshot!(group_changelog);
}

#[test]
fn release_uses_linked_keep_a_changelog_titles_without_double_wrapping() {
	let tempdir = setup_scenario_workspace("changelog-formats/linked-title");
	let output = monochange_command(Some("2026-04-06"))
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
	assert_snapshot!(core_changelog);
}

#[test]
fn release_filters_group_changelog_entries_to_selected_member_packages() {
	let tempdir = setup_scenario_workspace("changelog-formats/group-include-selected");

	let output = monochange_command(Some("2026-04-06"))
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
	let tempdir = setup_scenario_workspace("changelog-formats/group-include-group-only");

	let output = monochange_command(Some("2026-04-06"))
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
	let tempdir = setup_scenario_workspace("changelog-formats/group-include-group-note");

	let output = monochange_command(Some("2026-04-06"))
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
	let tempdir = setup_scenario_workspace("changelog-formats/group-include-multi-target-blocked");

	let output = monochange_command(Some("2026-04-06"))
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
