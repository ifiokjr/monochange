use std::ffi::OsString;
use std::path::Path;

use insta::assert_json_snapshot;
use monochange_test_helpers::git;
use rstest::rstest;
use serde_json::Value;

mod test_support;
use test_support::copy_directory;
use test_support::current_test_name;
use test_support::fixture_path;
use test_support::monochange_command;
use test_support::setup_scenario_workspace;
use test_support::snapshot_settings;

#[rstest]
#[case::detects_package_changes(
	"affected/single-package",
	&["--changed-paths", "crates/core/src/lib.rs"]
)]
#[case::reports_not_required_for_non_package_changes(
	"affected/single-package",
	&["--changed-paths", "docs/readme.md"]
)]
#[case::respects_package_ignored_paths(
	"affected/ignored-paths",
	&["--changed-paths", "crates/core/tests/smoke.rs"]
)]
#[case::respects_package_additional_paths(
	"affected/additional-paths",
	&["--changed-paths", "Cargo.lock"]
)]
#[case::skips_when_allowed_label_is_present(
	"affected/skip-label",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--label",
		"no-changeset-required",
	]
)]
#[case::accepts_changesets_covering_changed_packages(
	"affected/single-package-with-changeset",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	]
)]
#[case::fails_when_changeset_targets_wrong_package(
	"affected/single-package-wrong-target",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	]
)]
#[case::accepts_group_changeset_covering_member_package(
	"affected/group-coverage",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	]
)]
#[case::accepts_group_changeset_covering_multiple_members(
	"affected/group-coverage",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		"crates/other/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	]
)]
#[case::fails_when_changeset_targets_wrong_group(
	"affected/group-coverage-missing",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	]
)]
#[case::reports_uncovered_packages_when_changeset_is_partial(
	"affected/multi-package",
	&[
		"--changed-paths",
		"crates/core/src/lib.rs",
		"--changed-paths",
		"crates/other/src/lib.rs",
		"--changed-paths",
		".changeset/feature.md",
	]
)]
fn affected_scenarios_match_snapshot(#[case] fixture: &str, #[case] args: &[&str]) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let output = run_affected_json(&fixture_path(fixture), args);
	assert_json_snapshot!(output);
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
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("affected/since-base");
	let root = tempdir.path();

	git(root, &["init"]);
	git(root, &["config", "user.name", "Test"]);
	git(root, &["config", "user.email", "test@test.com"]);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);

	copy_directory(&fixture_path("affected/since-changed-source"), root);

	let json = run_affected_json(root, &["--since", "HEAD"]);
	assert_json_snapshot!(json);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn affected_since_flag_detects_changeset_added_after_revision() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("affected/since-base");
	let root = tempdir.path();

	git(root, &["init"]);
	git(root, &["config", "user.name", "Test"]);
	git(root, &["config", "user.email", "test@test.com"]);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);

	copy_directory(&fixture_path("affected/since-changed-with-changeset"), root);

	let json = run_affected_json(root, &["--since", "HEAD"]);
	assert_json_snapshot!(json);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn affected_since_takes_priority_over_changed_paths_with_warning() {
	let tempdir = setup_scenario_workspace("affected/since-base");
	let root = tempdir.path();

	git(root, &["init"]);
	git(root, &["config", "user.name", "Test"]);
	git(root, &["config", "user.email", "test@test.com"]);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);

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
	let cli_args = std::iter::once(OsString::from("mc"))
		.chain(std::iter::once(OsString::from("affected")))
		.chain(std::iter::once(OsString::from("--format")))
		.chain(std::iter::once(OsString::from("json")))
		.chain(args.iter().map(|value| OsString::from(*value)));
	let output = monochange::run_with_args_in_dir("mc", cli_args, root)
		.unwrap_or_else(|error| panic!("command output: {error}"));
	serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("parse json: {error}\nstdout: {output}"))
}

fn run_affected_raw(root: &Path, args: &[&str]) -> std::process::Output {
	monochange_command(None)
		.current_dir(root)
		.arg("affected")
		.arg("--format")
		.arg("json")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"))
}
