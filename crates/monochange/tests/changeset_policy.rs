use std::path::Path;

use insta::assert_json_snapshot;
use rstest::rstest;
use serde_json::Value;

mod test_support;
use test_support::{
	current_test_name, monochange_command, setup_scenario_workspace, snapshot_settings,
};

#[rstest]
#[case::skip_label(
	"changeset-policy/with-changeset-core",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--label",
		"no-changeset-required",
	]
)]
#[case::non_package_changes(
	"changeset-policy/with-changeset-core",
	&["--changed-paths", "docs/readme.md"]
)]
#[case::ignored_paths(
	"changeset-policy/with-changeset-core",
	&["--changed-paths", "crates/core/tests/smoke.rs"]
)]
#[case::additional_paths(
	"changeset-policy/with-changeset-core",
	&["--changed-paths", "Cargo.lock"]
)]
#[case::wrong_target(
	"changeset-policy/with-changeset-other",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	]
)]
#[case::covered(
	"changeset-policy/with-changeset-core",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	]
)]
#[case::invalid_changeset(
	"changeset-policy/with-changeset-invalid-core",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	]
)]
fn verify_changeset_policy_scenarios_match_snapshot(
	#[case] scenario_relative: &str,
	#[case] args: &[&str],
) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace(scenario_relative);
	let json = run_affected_json(tempdir.path(), args);
	assert_json_snapshot!(json);
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
