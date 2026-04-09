use std::path::Path;
use std::process::Command;

use insta_cmd::get_cargo_bin;
use serde_json::{Map, Value};

#[allow(dead_code)]
#[path = "../../../testing/test_support/fs.rs"]
mod shared_fs_test_support;
#[allow(dead_code)]
#[path = "../../../testing/test_support/insta.rs"]
mod shared_insta_test_support;

#[allow(unused_imports)]
pub use shared_fs_test_support::copy_directory;
#[allow(unused_imports)]
pub use shared_fs_test_support::current_test_name;
#[allow(unused_imports)]
pub use shared_fs_test_support::fixture_path;
#[allow(unused_imports)]
pub use shared_fs_test_support::setup_fixture;
#[allow(unused_imports)]
pub use shared_fs_test_support::setup_scenario_workspace;
#[allow(unused_imports)]
pub use shared_insta_test_support::snapshot_settings;

#[allow(dead_code)]
pub fn monochange_command(release_date: Option<&str>) -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	if let Some(release_date) = release_date {
		command.env("MONOCHANGE_RELEASE_DATE", release_date);
	}
	command
}

#[allow(dead_code)]
pub fn run_json_command(root: &Path, command: &str, release_date: Option<&str>) -> Value {
	let output = monochange_command(release_date)
		.current_dir(root)
		.arg(command)
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
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

#[allow(dead_code)]
pub fn json_subset(value: &Value, fields: &[(&str, &str)]) -> Value {
	let mut subset = Map::new();
	for (key, pointer) in fields {
		subset.insert(
			(*key).to_string(),
			value.pointer(pointer).cloned().unwrap_or(Value::Null),
		);
	}
	Value::Object(subset)
}

#[cfg(test)]
mod tests {
	use std::fs;

	use rstest::rstest;
	use tempfile::TempDir;

	use super::copy_directory;
	use super::current_test_name;
	use super::fixture_path;
	use super::setup_scenario_workspace;
	use super::shared_fs_test_support::setup_fixture;

	#[test]
	fn current_test_name_returns_plain_function_name() {
		assert_eq!(
			current_test_name(),
			"current_test_name_returns_plain_function_name"
		);
	}

	#[rstest]
	fn case_1_strips_numeric_rstest_prefix_from_current_test_name() {
		assert_eq!(
			current_test_name(),
			"strips_numeric_rstest_prefix_from_current_test_name"
		);
	}

	#[test]
	fn fixture_path_resolves_known_fixture_directory() {
		let path = fixture_path("test-support/setup-fixture");
		assert!(path.is_dir());
		assert!(path.ends_with("fixtures/tests/test-support/setup-fixture"));
	}

	#[test]
	fn copy_directory_copies_nested_fixture_files() {
		let destination_root = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let destination = destination_root.path().join("copied");
		copy_directory(&fixture_path("test-support/setup-fixture"), &destination);
		assert_eq!(
			fs::read_to_string(destination.join("root.txt"))
				.unwrap_or_else(|error| panic!("read root fixture: {error}")),
			"root fixture\n"
		);
		assert_eq!(
			fs::read_to_string(destination.join("nested/child.txt"))
				.unwrap_or_else(|error| panic!("read nested fixture: {error}")),
			"nested child\n"
		);
	}

	#[test]
	fn setup_fixture_copies_fixture_contents_into_tempdir() {
		let tempdir = setup_fixture("test-support/setup-fixture");
		assert_eq!(
			fs::read_to_string(tempdir.path().join("nested/child.txt"))
				.unwrap_or_else(|error| panic!("read setup fixture: {error}")),
			"nested child\n"
		);
	}

	#[test]
	fn setup_scenario_workspace_prefers_workspace_directory_and_skips_expected_outputs() {
		let tempdir = setup_scenario_workspace("test-support/scenario-workspace");
		assert_eq!(
			fs::read_to_string(tempdir.path().join("workspace-only.txt"))
				.unwrap_or_else(|error| panic!("read workspace scenario file: {error}")),
			"workspace marker\n"
		);
		assert!(!tempdir.path().join("scenario-root-only.txt").exists());
		assert!(!tempdir.path().join("expected").exists());
	}

	#[test]
	fn setup_scenario_workspace_falls_back_to_scenario_root_when_no_workspace_exists() {
		let tempdir = setup_scenario_workspace("test-support/scenario-root");
		assert_eq!(
			fs::read_to_string(tempdir.path().join("root-only.txt"))
				.unwrap_or_else(|error| panic!("read root scenario file: {error}")),
			"root scenario\n"
		);
		assert!(!tempdir.path().join("expected").exists());
	}
}
