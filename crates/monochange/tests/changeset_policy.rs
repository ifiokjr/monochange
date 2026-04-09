use std::path::Path;

use rstest::rstest;
use serde_json::Value;

mod test_support;
use test_support::{
	assert_json_fixture, expected_fixture_path, monochange_command, setup_scenario_workspace,
};

#[rstest]
#[case::skip_label(
	"changeset-policy/with-changeset-core",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--label",
		"no-changeset-required",
	],
	"skip-label.json"
)]
#[case::non_package_changes(
	"changeset-policy/with-changeset-core",
	&["--changed-paths", "docs/readme.md"],
	"non-package-changes.json"
)]
#[case::ignored_paths(
	"changeset-policy/with-changeset-core",
	&["--changed-paths", "crates/core/tests/smoke.rs"],
	"ignored-paths.json"
)]
#[case::additional_paths(
	"changeset-policy/with-changeset-core",
	&["--changed-paths", "Cargo.lock"],
	"additional-paths.json"
)]
#[case::wrong_target(
	"changeset-policy/with-changeset-other",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	],
	"wrong-target.json"
)]
#[case::covered(
	"changeset-policy/with-changeset-core",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	],
	"covered.json"
)]
#[case::invalid_changeset(
	"changeset-policy/with-changeset-invalid-core",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	],
	"invalid-changeset.json"
)]
fn verify_changeset_policy_scenarios_match_expected_fixture(
	#[case] scenario_relative: &str,
	#[case] args: &[&str],
	#[case] expected_name: &str,
) {
	let tempdir = setup_scenario_workspace(scenario_relative);
	let json = run_affected_json(tempdir.path(), args);
	assert_json_fixture(
		&json,
		&expected_fixture_path(scenario_relative, expected_name),
	);
}

fn run_affected_json(root: &Path, args: &[&str]) -> Value {
	let output = monochange_command(None)
		.current_dir(root)
		.arg("affected")
		.arg("--format")
		.arg("json")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse command json: {error}"))
}
