use std::path::Path;
use std::process::Command;

use insta::assert_json_snapshot;
use insta_cmd::get_cargo_bin;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_publish_plan_dependency_order_repo() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(
		&fixture_path("cli-output/publish-plan-dependency-order"),
		root,
	);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
	tempdir
}

fn monochange_command(release_date: &str) -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command.env_remove("RUST_LOG");
	command.env("MONOCHANGE_RELEASE_DATE", release_date);
	command
}

fn assert_package_batch(value: &Value, batch_index: usize, expected: &[String]) {
	let packages = value["batches"][batch_index]["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("batch {batch_index} packages should be an array"))
		.iter()
		.map(|package| {
			package
				.as_str()
				.unwrap_or_else(|| panic!("batch {batch_index} package should be a string"))
				.to_string()
		})
		.collect::<Vec<_>>();
	assert_eq!(packages, expected);
}

#[test]
fn publish_plan_integration_preserves_dependency_order_across_grouped_batches() {
	let tempdir = setup_publish_plan_dependency_order_repo();
	let output = monochange_command("2026-04-06")
		.current_dir(tempdir.path())
		.arg("plan-release-publish")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run publish plan: {error}"));
	assert!(
		output.status.success(),
		"publish plan failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	let value: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse publish plan json: {error}"));
	let publish_rate_limits = &value["publishRateLimits"];
	let expected = (0..12)
		.map(|index| format!("crate_{index:02}"))
		.collect::<Vec<_>>();
	assert_package_batch(publish_rate_limits, 0, &expected[..10]);
	assert_package_batch(publish_rate_limits, 1, &expected);
	assert_json_snapshot!(publish_rate_limits);
}
