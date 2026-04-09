use rstest::rstest;

mod test_support;
use test_support::{
	assert_json_fixture, expected_fixture_path, run_json_command, setup_scenario_workspace,
};

#[rstest]
#[case::gitlab_publish_release("source/gitlab", "publish-release", "publish-release.json")]
#[case::gitea_release_pr("source/gitea", "release-pr", "release-pr.json")]
#[case::github_publish_release("source/github", "publish-release", "publish-release.json")]
#[case::github_release_pr("source/github", "release-pr", "release-pr.json")]
#[case::github_release_comments("source/github", "release-comments", "release-comments.json")]
fn source_provider_scenarios_match_expected_fixture(
	#[case] scenario_relative: &str,
	#[case] command: &str,
	#[case] expected_name: &str,
) {
	let tempdir = setup_scenario_workspace(scenario_relative);
	let json = run_json_command(tempdir.path(), command, Some("2026-04-06"));
	assert_json_fixture(
		&json,
		&expected_fixture_path(scenario_relative, expected_name),
	);
}
