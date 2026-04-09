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
fn affected_detects_package_changes() {
	let output = run_affected_json(
		&fixture_path("affected/single-package"),
		&["--changed-paths", "crates/core/src/lib.rs"],
	);
	assert_eq!(output["status"], "failed");
	assert_eq!(output["affectedPackageIds"][0], "core");
	assert_eq!(output["uncoveredPackageIds"][0], "core");
}

#[test]
fn affected_reports_not_required_for_non_package_changes() {
	let output = run_affected_json(
		&fixture_path("affected/single-package"),
		&["--changed-paths", "docs/readme.md"],
	);
	assert_eq!(output["status"], "not_required");
	assert_eq!(
		output["affectedPackageIds"].as_array().map(Vec::len),
		Some(0)
	);
}

#[test]
fn affected_respects_package_ignored_paths() {
	let output = run_affected_json(
		&fixture_path("affected/ignored-paths"),
		&["--changed-paths", "crates/core/tests/smoke.rs"],
	);
	assert_eq!(output["status"], "not_required");
	assert_eq!(output["ignoredPaths"][0], "crates/core/tests/smoke.rs");
}

#[test]
fn affected_respects_package_additional_paths() {
	let output = run_affected_json(
		&fixture_path("affected/additional-paths"),
		&["--changed-paths", "Cargo.lock"],
	);
	assert_eq!(output["status"], "failed");
	assert_eq!(output["affectedPackageIds"][0], "core");
}

#[test]
fn affected_skips_when_allowed_label_is_present() {
	let output = run_affected_json(
		&fixture_path("affected/skip-label"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--label",
			"no-changeset-required",
		],
	);
	assert_eq!(output["status"], "skipped");
	assert_eq!(output["matchedSkipLabels"][0], "no-changeset-required");
}

#[test]
fn affected_accepts_changesets_covering_changed_packages() {
	let output = run_affected_json(
		&fixture_path("affected/single-package-with-changeset"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(output["status"], "passed");
	assert_eq!(output["coveredPackageIds"][0], "core");
}

#[test]
fn affected_fails_when_changeset_targets_wrong_package() {
	let output = run_affected_json(
		&fixture_path("affected/single-package-wrong-target"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(output["status"], "failed");
	assert_eq!(output["uncoveredPackageIds"][0], "core");
}

#[test]
fn affected_accepts_group_changeset_covering_member_package() {
	let output = run_affected_json(
		&fixture_path("affected/group-coverage"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(
		output["status"],
		"passed",
		"a changeset targeting the group should cover all member packages: {}",
		serde_json::to_string_pretty(&output).unwrap_or_default()
	);
	assert!(
		output["coveredPackageIds"]
			.as_array()
			.is_some_and(|ids| ids.iter().any(|id| id == "core")),
		"core should be covered by the group changeset"
	);
}

#[test]
fn affected_accepts_group_changeset_covering_multiple_members() {
	let output = run_affected_json(
		&fixture_path("affected/group-coverage"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			"crates/other/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(
		output["status"],
		"passed",
		"a group changeset should cover all members changed: {}",
		serde_json::to_string_pretty(&output).unwrap_or_default()
	);
}

#[test]
fn affected_fails_when_changeset_targets_wrong_group() {
	let output = run_affected_json(
		&fixture_path("affected/group-coverage-missing"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(
		output["status"],
		"failed",
		"a changeset targeting 'secondary' should NOT cover 'core' in 'main' group: {}",
		serde_json::to_string_pretty(&output).unwrap_or_default()
	);
}

#[test]
fn affected_reports_uncovered_packages_when_changeset_is_partial() {
	let output = run_affected_json(
		&fixture_path("affected/multi-package"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			"crates/other/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(output["status"], "failed");
	assert!(output["coveredPackageIds"]
		.as_array()
		.is_some_and(|ids| ids.iter().any(|id| id == "core")));
	assert!(output["uncoveredPackageIds"]
		.as_array()
		.is_some_and(|ids| ids.iter().any(|id| id == "other")));
}

#[test]
fn affected_without_verify_flag_exits_zero_even_when_uncovered() {
	let output = run_affected_raw(
		&fixture_path("affected/single-package"),
		&["--changed-paths", "crates/core/src/lib.rs"],
	);
	assert!(
		output.status.success(),
		"without --verify, exit code should be 0 even when packages are uncovered"
	);
}

#[test]
fn affected_with_verify_flag_exits_nonzero_when_uncovered() {
	let output = run_affected_raw(
		&fixture_path("affected/single-package"),
		&["--changed-paths", "crates/core/src/lib.rs", "--verify"],
	);
	assert!(
		!output.status.success(),
		"with --verify, exit code should be non-zero when packages are uncovered"
	);
}

#[test]
fn affected_with_verify_flag_exits_zero_when_covered() {
	let output = run_affected_raw(
		&fixture_path("affected/single-package-with-changeset"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
			"--verify",
		],
	);
	assert!(
		output.status.success(),
		"with --verify, the exit code should be 0 when all packages are covered: stderr={}",
		String::from_utf8_lossy(&output.stderr)
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn affected_since_flag_detects_changes_from_git_revision() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path("affected/since-base"), root);

	run_git(root, &["init"]);
	run_git(root, &["config", "user.name", "Test"]);
	run_git(root, &["config", "user.email", "test@test.com"]);
	run_git(root, &["add", "."]);
	run_git(root, &["commit", "-m", "initial"]);

	copy_directory(&fixture_path("affected/since-changed-source"), root);

	let json = run_affected_json(root, &["--since", "HEAD"]);
	assert_eq!(json["status"], "failed");
	assert!(json["affectedPackageIds"]
		.as_array()
		.is_some_and(|ids| ids.iter().any(|id| id == "core")));
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn affected_since_flag_detects_changeset_added_after_revision() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path("affected/since-base"), root);

	run_git(root, &["init"]);
	run_git(root, &["config", "user.name", "Test"]);
	run_git(root, &["config", "user.email", "test@test.com"]);
	run_git(root, &["add", "."]);
	run_git(root, &["commit", "-m", "initial"]);

	copy_directory(&fixture_path("affected/since-changed-with-changeset"), root);

	let json = run_affected_json(root, &["--since", "HEAD"]);
	assert_eq!(
		json["status"],
		"passed",
		"changeset added after rev should cover the package: {}",
		serde_json::to_string_pretty(&json).unwrap_or_default()
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn affected_since_takes_priority_over_changed_paths_with_warning() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path("affected/since-base"), root);

	run_git(root, &["init"]);
	run_git(root, &["config", "user.name", "Test"]);
	run_git(root, &["config", "user.email", "test@test.com"]);
	run_git(root, &["add", "."]);
	run_git(root, &["commit", "-m", "initial"]);

	let output = run_affected_raw(
		root,
		&[
			"--since",
			"HEAD",
			"--changed-paths",
			"crates/core/src/lib.rs",
		],
	);
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("--since takes priority") || stderr.contains("--changed-paths was ignored"),
		"should warn when both flags are provided: stderr={stderr}"
	);
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
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!(
			"parse json: {error}\nstdout: {}",
			String::from_utf8_lossy(&output.stdout)
		)
	})
}

fn run_affected_raw(root: &Path, args: &[&str]) -> std::process::Output {
	cli()
		.current_dir(root)
		.arg("affected")
		.arg("--format")
		.arg("json")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"))
}

fn run_git(root: &Path, args: &[&str]) {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}
