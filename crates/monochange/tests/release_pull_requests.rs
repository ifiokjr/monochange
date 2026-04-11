use insta::assert_json_snapshot;
use rstest::rstest;

mod test_support;
use test_support::current_test_name;
use test_support::run_json_command;
use test_support::setup_scenario_workspace;
use test_support::snapshot_settings;

#[rstest]
#[case::group("group")]
#[case::ungrouped("ungrouped")]
fn open_release_pull_request_dry_run_matches_snapshot(#[case] scenario: &str) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let scenario_relative = format!("release-pr/{scenario}");
	let tempdir = setup_scenario_workspace(&scenario_relative);
	let json = run_json_command(tempdir.path(), "release-pr", Some("2026-04-06"));
	assert_json_snapshot!(json);
}
