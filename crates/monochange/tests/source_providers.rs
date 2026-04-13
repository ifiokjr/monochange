use insta::assert_json_snapshot;
use rstest::rstest;

mod test_support;
use test_support::current_test_name;
use test_support::run_json_command;
use test_support::setup_scenario_workspace;
use test_support::snapshot_settings;

#[rstest]
#[case::gitlab_publish_release("source/gitlab", "publish-release")]
#[case::gitea_release_pr("source/gitea", "release-pr")]
#[case::github_publish_release("source/github", "publish-release")]
#[case::github_release_pr("source/github", "release-pr")]
#[case::github_release_comments("source/github", "release-comments")]
fn source_provider_scenarios_match_snapshot(
	#[case] scenario_relative: &str,
	#[case] command: &str,
) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace(scenario_relative);
	let json = run_json_command(tempdir.path(), command, Some("2026-04-06"));
	assert_json_snapshot!(json);
}

#[rstest]
#[case::github("source/github")]
#[case::gitlab("source/gitlab")]
#[case::gitea("source/gitea")]
fn source_provider_diagnostics_match_snapshot(#[case] scenario_relative: &str) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace(scenario_relative);
	let json = run_json_command(tempdir.path(), "diagnostics", Some("2026-04-06"));
	assert_json_snapshot!(json);
}
