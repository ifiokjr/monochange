use rstest::rstest;

mod test_support;
use test_support::{
	assert_json_fixture, expected_fixture_path, run_json_command, setup_scenario_workspace,
};

#[rstest]
#[case::group("group")]
#[case::ungrouped("ungrouped")]
fn open_release_pull_request_dry_run_matches_expected_fixture(#[case] scenario: &str) {
	let scenario_relative = format!("release-pr/{scenario}");
	let tempdir = setup_scenario_workspace(&scenario_relative);
	let json = run_json_command(tempdir.path(), "release-pr", Some("2026-04-06"));
	assert_json_fixture(
		&json,
		&expected_fixture_path(&scenario_relative, "release-pr.json"),
	);
}
