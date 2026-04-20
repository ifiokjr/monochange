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
fn changelog_sections_produce_custom_headings_for_types() {
	let tempdir = setup_scenario_workspace("changelog-formats/custom-changelog-sections");
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

	// Verify section headings appear in priority order
	let breaking_pos = group_changelog.find("### Breaking Changes");
	let features_pos = group_changelog.find("### Features");
	let fixes_pos = group_changelog.find("### Bug Fixes");
	assert!(
		breaking_pos.is_some(),
		"expected ### Breaking Changes heading"
	);
	assert!(features_pos.is_some(), "expected ### Features heading");
	assert!(fixes_pos.is_some(), "expected ### Bug Fixes heading");
	assert!(
		breaking_pos.unwrap() < features_pos.unwrap(),
		"Breaking Changes should appear before Features (lower priority)"
	);
	assert!(
		features_pos.unwrap() < fixes_pos.unwrap(),
		"Features should appear before Bug Fixes (lower priority)"
	);

	// Verify entries appear under correct sections
	assert!(
		group_changelog.contains("### Features"),
		"expected ### Features heading"
	);
	assert!(
		group_changelog.contains("### Bug Fixes"),
		"expected ### Bug Fixes heading"
	);
	assert!(
		group_changelog.contains("### Breaking Changes"),
		"expected ### Breaking Changes heading"
	);
}

#[test]
fn default_changelog_sections_render_heading_for_routed_types() {
	let tempdir = setup_scenario_workspace("changelog-formats/default-sections-mixed-types");
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

	// Default sections: "feat" routes to section with heading "Added",
	// "fix" routes to "Fixed", "docs" routes to "Documentation"
	assert!(
		group_changelog.contains("### Added"),
		"expected ### Added heading for minor/feat types"
	);
	assert!(
		group_changelog.contains("### Fixed"),
		"expected ### Fixed heading for fix type"
	);
	assert!(
		group_changelog.contains("### Documentation"),
		"expected ### Documentation heading for docs type"
	);

	// Verify entries are grouped under headings
	// Core changelog has entries under Added heading
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	assert!(
		core_changelog.contains("### Added"),
		"expected ### Added heading in core changelog"
	);
	assert!(
		core_changelog.contains("add release command"),
		"feat entry should appear in core changelog"
	);
}

#[test]
fn excluded_changelog_types_filters_types_from_package() {
	let tempdir = setup_scenario_workspace("changelog-formats/excluded-changelog-types");
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

	// Core package has excluded_changelog_types = ["test"],
	// so the "test" type cannot be used in changesets targeting core.
	// App has no exclusion, so app: test is valid.
	// Verify that core's changelog has feat entries but not test entries.
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	assert!(
		core_changelog.contains("### Features"),
		"core should have Features section for feat type"
	);
	assert!(
		!core_changelog.contains("integration tests"),
		"core should not contain test entry from other package"
	);
	assert!(
		core_changelog.contains("add new command"),
		"core should contain feat entry"
	);

	// The test-type entry SHOULD appear in app's changelog and the group changelog
	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	assert!(
		app_changelog.contains("add integration tests"),
		"app should contain test entry"
	);

	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	assert!(
		group_changelog.contains("add integration tests"),
		"group changelog should contain test entry from app"
	);
}

#[test]
fn keep_a_changelog_format_includes_section_headings() {
	let tempdir = setup_scenario_workspace("changelog-formats/keep-a-changelog-with-sections");
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

	// Keep-a-changelog format always includes section headings
	assert!(
		group_changelog.contains("### Features"),
		"expected ### Features heading in keep-a-changelog format"
	);
	assert!(
		group_changelog.contains("### Bug Fixes"),
		"expected ### Bug Fixes heading in keep-a-changelog format"
	);
}

#[test]
fn section_priority_controls_heading_order() {
	let tempdir = setup_scenario_workspace("changelog-formats/section-priority-ordering");
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

	// Fixes has priority 10, Features has priority 20,
	// so Bug Fixes should appear BEFORE Features
	let fixes_pos = group_changelog
		.find("### Bug Fixes")
		.expect("expected ### Bug Fixes heading");
	let features_pos = group_changelog
		.find("### Features")
		.expect("expected ### Features heading");
	assert!(
		fixes_pos < features_pos,
		"Bug Fixes (priority 10) should appear before Features (priority 20)"
	);
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
