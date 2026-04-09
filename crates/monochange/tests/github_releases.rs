use insta::assert_json_snapshot;
use rstest::rstest;

mod test_support;
use test_support::{
	current_test_name, run_json_command, setup_scenario_workspace, snapshot_settings,
};

#[rstest]
#[case::group("group")]
#[case::ungrouped("ungrouped")]
#[case::custom_sections("custom-sections")]
#[case::default_custom_sections("default-custom-sections")]
fn publish_github_release_dry_run_matches_snapshot(#[case] scenario: &str) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let scenario_relative = format!("github-releases/{scenario}");
	let tempdir = setup_scenario_workspace(&scenario_relative);
	let json = run_json_command(tempdir.path(), "publish-release", Some("2026-04-06"));
	assert_json_snapshot!(json);
}
