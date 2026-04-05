use std::path::Path;
use std::process::Command;

use insta_cmd::get_cargo_bin;
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
fn verify_skipped_required_changes_when_allowed_label_is_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("changeset-policy/with-changeset-core"),
		tempdir.path(),
	);

	let json = run_affected_json(
		tempdir.path(),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--label",
			"no-changeset-required",
		],
	);
	assert_eq!(json["status"], "skipped");
	assert_eq!(json["required"], false);
	assert_eq!(json["matchedSkipLabels"][0], "no-changeset-required");
}

#[test]
fn verify_does_not_require_changesets_for_non_package_changes() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("changeset-policy/with-changeset-core"),
		tempdir.path(),
	);

	let json = run_affected_json(tempdir.path(), &["--changed-paths", "docs/readme.md"]);
	assert_eq!(json["status"], "not_required");
	assert_eq!(json["affectedPackageIds"].as_array().map(Vec::len), Some(0));
}

#[test]
fn verify_respects_package_ignored_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("changeset-policy/with-changeset-core"),
		tempdir.path(),
	);

	let json = run_affected_json(
		tempdir.path(),
		&["--changed-paths", "crates/core/tests/smoke.rs"],
	);
	assert_eq!(json["status"], "not_required");
	assert_eq!(json["ignoredPaths"][0], "crates/core/tests/smoke.rs");
}

#[test]
fn verify_respects_package_additional_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("changeset-policy/with-changeset-core"),
		tempdir.path(),
	);

	let json = run_affected_json(tempdir.path(), &["--changed-paths", "Cargo.lock"]);
	assert_eq!(json["status"], "failed");
	assert_eq!(json["affectedPackageIds"][0], "core");
}

#[test]
fn verify_fails_when_attached_changeset_targets_the_wrong_package() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("changeset-policy/with-changeset-other"),
		tempdir.path(),
	);

	let json = run_affected_json(
		tempdir.path(),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(json["status"], "failed");
	assert_eq!(json["uncoveredPackageIds"][0], "core");
}

#[test]
fn verify_accepts_attached_changesets_that_cover_changed_packages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("changeset-policy/with-changeset-core"),
		tempdir.path(),
	);

	let json = run_affected_json(
		tempdir.path(),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(json["status"], "passed");
	assert_eq!(json["coveredPackageIds"][0], "core");
}

#[test]
fn verify_reports_invalid_attached_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("changeset-policy/with-changeset-invalid-core"),
		tempdir.path(),
	);

	let json = run_affected_json(
		tempdir.path(),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(json["status"], "failed");
	assert!(json["errors"].as_array().is_some_and(|errors| {
		errors.iter().any(|error| {
			error
				.as_str()
				.is_some_and(|msg| msg.contains("must map to `patch`, `minor`, or `major`"))
		})
	}));
	assert!(json["comment"]
		.as_str()
		.is_some_and(|comment| comment.contains("Attached changeset files:")));
}

fn run_affected_json(root: &Path, args: &[&str]) -> Value {
	let output = cli()
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
