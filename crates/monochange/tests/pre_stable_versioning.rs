use insta::assert_json_snapshot;
use insta::assert_snapshot;
use rstest::rstest;
use serde_json::Value;

mod test_support;
use test_support::monochange_command;
use test_support::run_json_command;
use test_support::setup_scenario_workspace;
use test_support::snapshot_settings;

#[rstest]
#[case::pre_stable_major_text("pre-stable-versioning/pre-stable-major", "pre_stable_major_text")]
#[case::stable_major_text("pre-stable-versioning/stable-major", "stable_major_text")]
#[case::pre_stable_grouped_major_text(
	"pre-stable-versioning/pre-stable-grouped-major",
	"pre_stable_grouped_major_text"
)]
fn pre_stable_release_text_scenarios_match_snapshot(
	#[case] fixture: &str,
	#[case] snapshot_name: &str,
) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(snapshot_name);
	let _guard = settings.bind_to_scope();
	let tempdir = setup_scenario_workspace(fixture);

	let output = monochange_command(Some("2026-04-06"))
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("text")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_snapshot!(String::from_utf8_lossy(&output.stdout));
}

#[rstest]
#[case::pre_stable_major_json("pre-stable-versioning/pre-stable-major", "pre_stable_major_json")]
#[case::pre_stable_minor_json("pre-stable-versioning/pre-stable-minor", "pre_stable_minor_json")]
#[case::stable_major_json("pre-stable-versioning/stable-major", "stable_major_json")]
fn pre_stable_release_json_scenarios_match_snapshot(
	#[case] fixture: &str,
	#[case] snapshot_name: &str,
) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(snapshot_name);
	let _guard = settings.bind_to_scope();
	let tempdir = setup_scenario_workspace(fixture);
	let json = run_json_command(tempdir.path(), "release", Some("2026-04-06"));
	assert_json_snapshot!(json);
}

fn find_decision<'a>(json: &'a Value, package_name_fragment: &str) -> &'a Value {
	json["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("decisions array"))
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains(package_name_fragment))
		})
		.unwrap_or_else(|| panic!("expected decision for {package_name_fragment}"))
}

#[test]
fn pre_stable_major_bump_keeps_expected_decisions() {
	let tempdir = setup_scenario_workspace("pre-stable-versioning/pre-stable-major");
	let json = run_json_command(tempdir.path(), "release", Some("2026-04-06"));

	let core_decision = find_decision(&json, "core");
	assert_eq!(core_decision["bump"], "major");
	assert_eq!(core_decision["plannedVersion"], "0.2.0");
	assert_eq!(core_decision["trigger"], "direct-change");

	let app_decision = find_decision(&json, "app");
	assert_eq!(app_decision["bump"], "patch");
	assert_eq!(app_decision["plannedVersion"], "0.1.1");
	assert_eq!(app_decision["trigger"], "transitive-dependency");
}

#[test]
fn pre_stable_minor_bump_keeps_expected_decisions() {
	let tempdir = setup_scenario_workspace("pre-stable-versioning/pre-stable-minor");
	let json = run_json_command(tempdir.path(), "release", Some("2026-04-06"));

	let core_decision = find_decision(&json, "core");
	assert_eq!(core_decision["bump"], "minor");
	assert_eq!(core_decision["plannedVersion"], "0.1.1");
}

#[test]
fn stable_major_bump_keeps_expected_decisions() {
	let tempdir = setup_scenario_workspace("pre-stable-versioning/stable-major");
	let json = run_json_command(tempdir.path(), "release", Some("2026-04-06"));

	let core_decision = find_decision(&json, "core");
	assert_eq!(core_decision["bump"], "major");
	assert_eq!(core_decision["plannedVersion"], "2.0.0");
}
