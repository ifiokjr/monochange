use std::process::Command;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use serde_json::Value;
use tempfile::tempdir;

mod test_support;
use test_support::{copy_directory, fixture_path};

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command
}

#[test]
fn pre_stable_major_bump_produces_minor_version_in_text_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("pre-stable-versioning/pre-stable-major");
	copy_directory(&fixture_root, tempdir.path());

	assert_cmd_snapshot!(cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("text"));
}

#[test]
fn pre_stable_major_bump_produces_minor_version_in_json_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("pre-stable-versioning/pre-stable-major");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let json = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json: {error}"));

	// core: major requested on 0.1.0 → planned 0.2.0
	let core_decision = find_decision(&json, "core");
	assert_eq!(core_decision["bump"], "major");
	assert_eq!(core_decision["plannedVersion"], "0.2.0");
	assert_eq!(core_decision["trigger"], "direct-change");

	// app: transitive patch on 0.1.0 → planned 0.1.1
	let app_decision = find_decision(&json, "app");
	assert_eq!(app_decision["bump"], "patch");
	assert_eq!(app_decision["plannedVersion"], "0.1.1");
	assert_eq!(app_decision["trigger"], "transitive-dependency");
}

#[test]
fn pre_stable_minor_bump_produces_patch_version_in_json_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("pre-stable-versioning/pre-stable-minor");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let json = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json: {error}"));

	// core: minor requested on 0.1.0 → planned 0.1.1
	let core_decision = find_decision(&json, "core");
	assert_eq!(core_decision["bump"], "minor");
	assert_eq!(core_decision["plannedVersion"], "0.1.1");
}

#[test]
fn stable_major_bump_produces_next_major_version_in_text_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("pre-stable-versioning/stable-major");
	copy_directory(&fixture_root, tempdir.path());

	assert_cmd_snapshot!(cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("text"));
}

#[test]
fn stable_major_bump_produces_next_major_version_in_json_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("pre-stable-versioning/stable-major");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let json = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json: {error}"));

	let core_decision = find_decision(&json, "core");
	assert_eq!(core_decision["bump"], "major");
	assert_eq!(core_decision["plannedVersion"], "2.0.0");
}

#[test]
fn pre_stable_grouped_major_bump_shifts_group_version() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("pre-stable-versioning/pre-stable-grouped-major");
	copy_directory(&fixture_root, tempdir.path());

	assert_cmd_snapshot!(cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("text"));
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
