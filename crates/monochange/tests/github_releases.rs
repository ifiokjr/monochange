use rstest::rstest;

mod test_support;
use test_support::{
	assert_json_fixture, expected_fixture_path, run_json_command, setup_scenario_workspace,
};

#[rstest]
#[case::group("group")]
#[case::ungrouped("ungrouped")]
#[case::custom_sections("custom-sections")]
#[case::default_custom_sections("default-custom-sections")]
fn publish_github_release_dry_run_matches_expected_fixture(#[case] scenario: &str) {
	let scenario_relative = format!("github-releases/{scenario}");
	let tempdir = setup_scenario_workspace(&scenario_relative);
	let json = run_json_command(tempdir.path(), "publish-release", Some("2026-04-06"));
	assert_json_fixture(
		&json,
		&expected_fixture_path(&scenario_relative, "publish-release.json"),
	);
}
