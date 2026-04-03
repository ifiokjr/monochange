use std::fs;
use std::path::Path;
use std::process::Command;

use insta_cmd::get_cargo_bin;
use serde_json::Value;
use tempfile::tempdir;

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command
}

fn fixture_path(name: &str) -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/affected")
		.join(name)
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

// --- Basic affected detection ---

#[test]
fn affected_detects_package_changes() {
	let json = run_affected_json(
		&fixture_path("single-package"),
		&["--changed-paths", "crates/core/src/lib.rs"],
	);
	assert_eq!(json["status"], "failed");
	assert_eq!(json["affectedPackageIds"][0], "core");
	assert_eq!(json["uncoveredPackageIds"][0], "core");
}

// --- Non-package changes ---

#[test]
fn affected_reports_not_required_for_non_package_changes() {
	let json = run_affected_json(
		&fixture_path("single-package"),
		&["--changed-paths", "docs/readme.md"],
	);
	assert_eq!(json["status"], "not_required");
	assert_eq!(json["affectedPackageIds"].as_array().map(Vec::len), Some(0));
}

// --- Ignored paths ---

#[test]
fn affected_respects_package_ignored_paths() {
	let json = run_affected_json(
		&fixture_path("ignored-paths"),
		&["--changed-paths", "crates/core/tests/smoke.rs"],
	);
	assert_eq!(json["status"], "not_required");
	assert_eq!(json["ignoredPaths"][0], "crates/core/tests/smoke.rs");
}

// --- Additional paths ---

#[test]
fn affected_respects_package_additional_paths() {
	let json = run_affected_json(
		&fixture_path("additional-paths"),
		&["--changed-paths", "Cargo.lock"],
	);
	assert_eq!(json["status"], "failed");
	assert_eq!(json["affectedPackageIds"][0], "core");
}

// --- Skip labels ---

#[test]
fn affected_skips_when_allowed_label_is_present() {
	let json = run_affected_json(
		&fixture_path("skip-label"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--label",
			"no-changeset-required",
		],
	);
	assert_eq!(json["status"], "skipped");
	assert_eq!(json["matchedSkipLabels"][0], "no-changeset-required");
}

// --- Correct changeset coverage ---

#[test]
fn affected_accepts_changesets_covering_changed_packages() {
	let json = run_affected_json(
		&fixture_path("single-package-with-changeset"),
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

// --- Wrong changeset target ---

#[test]
fn affected_fails_when_changeset_targets_wrong_package() {
	let json = run_affected_json(
		&fixture_path("single-package-wrong-target"),
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

// --- GROUP COVERAGE (critical test) ---

#[test]
fn affected_accepts_group_changeset_covering_member_package() {
	let json = run_affected_json(
		&fixture_path("group-coverage"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(
		json["status"],
		"passed",
		"a changeset targeting the group should cover all member packages: {}",
		serde_json::to_string_pretty(&json).unwrap_or_default()
	);
	assert!(
		json["coveredPackageIds"]
			.as_array()
			.unwrap_or(&Vec::new())
			.iter()
			.any(|id| id == "core"),
		"core should be covered by the group changeset"
	);
}

#[test]
fn affected_accepts_group_changeset_covering_multiple_members() {
	let json = run_affected_json(
		&fixture_path("group-coverage"),
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
		json["status"],
		"passed",
		"a group changeset should cover all members changed: {}",
		serde_json::to_string_pretty(&json).unwrap_or_default()
	);
}

// --- Group coverage missing ---

#[test]
fn affected_fails_when_changeset_targets_wrong_group() {
	let json = run_affected_json(
		&fixture_path("group-coverage-missing"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(
		json["status"],
		"failed",
		"a changeset targeting 'secondary' should NOT cover 'core' in 'main' group: {}",
		serde_json::to_string_pretty(&json).unwrap_or_default()
	);
}

// --- Multi-package partial coverage ---

#[test]
fn affected_reports_uncovered_packages_when_changeset_is_partial() {
	let json = run_affected_json(
		&fixture_path("multi-package"),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			"crates/other/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(json["status"], "failed");
	assert!(json["coveredPackageIds"]
		.as_array()
		.unwrap_or(&Vec::new())
		.iter()
		.any(|id| id == "core"));
	assert!(json["uncoveredPackageIds"]
		.as_array()
		.unwrap_or(&Vec::new())
		.iter()
		.any(|id| id == "other"));
}

// --- --verify flag exit code behavior ---

#[test]
fn affected_without_verify_flag_exits_zero_even_when_uncovered() {
	let output = run_affected_raw(
		&fixture_path("single-package"),
		&["--changed-paths", "crates/core/src/lib.rs"],
	);
	assert!(
		output.status.success(),
		"without --verify, the exit code should be 0 even when packages are uncovered"
	);
}

#[test]
fn affected_with_verify_flag_exits_nonzero_when_uncovered() {
	let output = run_affected_raw(
		&fixture_path("single-package"),
		&["--changed-paths", "crates/core/src/lib.rs", "--verify"],
	);
	assert!(
		!output.status.success(),
		"with --verify, the exit code should be non-zero when packages are uncovered"
	);
}

#[test]
fn affected_with_verify_flag_exits_zero_when_covered() {
	let output = run_affected_raw(
		&fixture_path("single-package-with-changeset"),
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

// --- --since flag with git ---

#[test]
fn affected_since_flag_detects_changes_from_git_revision() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	// Seed a git repo with a package
	write_file(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	);
	write_file(root.join("crates/core/src/lib.rs"), "pub fn core() {}\n");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[changesets.verify]
enabled = true
required = true

[package.core]
path = "crates/core"

[cli.affected]
[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"
[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
[[cli.affected.inputs]]
name = "since"
type = "string"
[[cli.affected.inputs]]
name = "verify"
type = "boolean"
[[cli.affected.inputs]]
name = "label"
type = "string_list"
[[cli.affected.steps]]
type = "AffectedPackages"
"#,
	);

	run_git(root, &["init"]);
	run_git(root, &["config", "user.name", "Test"]);
	run_git(root, &["config", "user.email", "test@test.com"]);
	run_git(root, &["add", "."]);
	run_git(root, &["commit", "-m", "initial"]);

	// Make a change to the package after the initial commit
	write_file(
		root.join("crates/core/src/lib.rs"),
		"pub fn core() { /* changed */ }\n",
	);

	let json = run_affected_json(root, &["--since", "HEAD"]);
	assert_eq!(json["status"], "failed");
	assert!(
		json["affectedPackageIds"]
			.as_array()
			.unwrap_or(&Vec::new())
			.iter()
			.any(|id| id == "core"),
		"core should be detected as affected via --since: {}",
		serde_json::to_string_pretty(&json).unwrap_or_default()
	);
}

#[test]
fn affected_since_flag_detects_changeset_added_after_revision() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	write_file(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	);
	write_file(root.join("crates/core/src/lib.rs"), "pub fn core() {}\n");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[changesets.verify]
enabled = true
required = true

[package.core]
path = "crates/core"

[cli.affected]
[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"
[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
[[cli.affected.inputs]]
name = "since"
type = "string"
[[cli.affected.inputs]]
name = "verify"
type = "boolean"
[[cli.affected.inputs]]
name = "label"
type = "string_list"
[[cli.affected.steps]]
type = "AffectedPackages"
"#,
	);

	run_git(root, &["init"]);
	run_git(root, &["config", "user.name", "Test"]);
	run_git(root, &["config", "user.email", "test@test.com"]);
	run_git(root, &["add", "."]);
	run_git(root, &["commit", "-m", "initial"]);

	// Make a change AND add a changeset
	write_file(
		root.join("crates/core/src/lib.rs"),
		"pub fn core() { /* changed */ }\n",
	);
	write_file(
		root.join(".changeset/feature.md"),
		"---\ncore: patch\n---\n\n#### add feature\n",
	);

	let json = run_affected_json(root, &["--since", "HEAD"]);
	assert_eq!(
		json["status"],
		"passed",
		"changeset added after rev should cover the package: {}",
		serde_json::to_string_pretty(&json).unwrap_or_default()
	);
}

// --- --since and --changed-paths mutual exclusivity ---

#[test]
fn affected_since_takes_priority_over_changed_paths_with_warning() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	write_file(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	);
	write_file(root.join("crates/core/src/lib.rs"), "pub fn core() {}\n");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[changesets.verify]
enabled = true
required = true

[package.core]
path = "crates/core"

[cli.affected]
[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"
[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
[[cli.affected.inputs]]
name = "since"
type = "string"
[[cli.affected.inputs]]
name = "verify"
type = "boolean"
[[cli.affected.inputs]]
name = "label"
type = "string_list"
[[cli.affected.steps]]
type = "AffectedPackages"
"#,
	);

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

// --- Helpers ---

fn write_file(path: impl AsRef<Path>, content: &str) {
	let path = path.as_ref();
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dir: {error}"));
	}
	fs::write(path, content)
		.unwrap_or_else(|error| panic!("write file {}: {error}", path.display()));
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
