use std::fs;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use rstest::rstest;
use serde_json::Value;

mod test_support;
use test_support::current_test_name;
use test_support::monochange_command;
use test_support::setup_scenario_workspace;
use test_support::snapshot_settings;

#[rstest]
#[case::ungrouped_patch("ungrouped-patch")]
#[case::ungrouped_caused_by("ungrouped-caused-by")]
#[case::grouped_default("grouped-default")]
fn release_note_changelog_snapshots_match_expected_output(#[case] scenario: &str) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace(&format!("release-notes-and-propagation/{scenario}"));
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

	match scenario {
		"ungrouped-patch" | "ungrouped-caused-by" => {
			let core_changelog =
				fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
					.unwrap_or_else(|error| panic!("core changelog: {error}"));
			let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
				.unwrap_or_else(|error| panic!("app changelog: {error}"));
			assert_snapshot!("core", core_changelog);
			assert_snapshot!("app", app_changelog);
		}
		"grouped-default" => {
			let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
				.unwrap_or_else(|error| panic!("app changelog: {error}"));
			let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
				.unwrap_or_else(|error| panic!("group changelog: {error}"));
			assert_snapshot!("app", app_changelog);
			assert_snapshot!("group", group_changelog);
		}
		_ => unreachable!("unexpected scenario"),
	}
}

#[test]
fn ungrouped_transitive_bump_with_parent_bump_minor_escalates_dependent_version() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix("app_decision");
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("release-notes-and-propagation/ungrouped-minor");
	let output = monochange_command(Some("2026-04-06"))
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
		.find(|decision| decision["package"].as_str() == Some("cargo:crates/app/Cargo.toml"))
		.unwrap_or_else(|| panic!("expected app decision"));

	assert_json_snapshot!(app_decision);
}

#[test]
fn custom_empty_update_message_on_package_overrides_default() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir =
		setup_scenario_workspace("release-notes-and-propagation/ungrouped-custom-package-message");
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

	let changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("package changelog: {error}"));
	assert_snapshot!(changelog);
}
