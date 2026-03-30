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

#[test]
fn verify_skips_required_changes_when_allowed_label_is_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_policy_fixture(tempdir.path(), false, false, "core");

	let json = run_json_workflow(
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
	seed_policy_fixture(tempdir.path(), false, false, "core");

	let json = run_json_workflow(tempdir.path(), &["--changed-paths", "docs/readme.md"]);
	assert_eq!(json["status"], "not_required");
	assert_eq!(json["matchedPaths"].as_array().map(Vec::len), Some(0));
	assert_eq!(json["affectedPackageIds"].as_array().map(Vec::len), Some(0));
}

#[test]
fn verify_respects_package_ignored_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_policy_fixture(tempdir.path(), false, false, "core");

	let json = run_json_workflow(
		tempdir.path(),
		&["--changed-paths", "crates/core/tests/smoke.rs"],
	);
	assert_eq!(json["status"], "not_required");
	assert_eq!(json["ignoredPaths"][0], "crates/core/tests/smoke.rs");
}

#[test]
fn verify_respects_package_additional_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_policy_fixture(tempdir.path(), false, false, "core");

	let json = run_json_workflow(tempdir.path(), &["--changed-paths", "Cargo.lock"]);
	assert_eq!(json["status"], "failed");
	assert_eq!(json["affectedPackageIds"][0], "core");
}

#[test]
fn verify_fails_when_attached_changeset_targets_the_wrong_package() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_policy_fixture(tempdir.path(), true, false, "other");

	let json = run_json_workflow(
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
	seed_policy_fixture(tempdir.path(), true, false, "core");

	let json = run_json_workflow(
		tempdir.path(),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(json["status"], "passed");
	assert_eq!(json["changesetPaths"][0], ".changeset/feature.md");
	assert_eq!(json["coveredPackageIds"][0], "core");
}

#[test]
fn verify_reports_invalid_attached_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_policy_fixture(tempdir.path(), true, true, "core");

	let json = run_json_workflow(
		tempdir.path(),
		&[
			"--changed-paths",
			"crates/core/src/lib.rs",
			"--changed-paths",
			".changeset/feature.md",
		],
	);
	assert_eq!(json["status"], "failed");
	assert!(json["errors"]
		.as_array()
		.unwrap_or_else(|| panic!("expected errors array"))
		.first()
		.and_then(Value::as_str)
		.is_some_and(|error| error.contains("must map to `patch`, `minor`, or `major`")));
	assert!(json["comment"]
		.as_str()
		.is_some_and(|comment| comment.contains("Attached changeset files:")));
}

fn run_json_workflow(root: &Path, args: &[&str]) -> Value {
	let output = cli()
		.current_dir(root)
		.arg("verify")
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

fn seed_policy_fixture(root: &Path, with_changeset: bool, invalid_changeset: bool, target: &str) {
	write_file(
		root.join("Cargo.toml"),
		r#"
[workspace]
members = ["crates/*"]
resolver = "2"
"#,
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	);
	write_file(
		root.join("crates/other/Cargo.toml"),
		"[package]\nname = \"other\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	);
	write_file(root.join("crates/core/src/lib.rs"), "pub fn core() {}\n");
	write_file(
		root.join("crates/core/tests/smoke.rs"),
		"#[test]\nfn smoke() {}\n",
	);
	write_file(root.join("crates/other/src/lib.rs"), "pub fn other() {}\n");
	write_file(root.join("docs/readme.md"), "# docs\n");
	write_file(root.join("Cargo.lock"), "# lock\n");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[changesets.verify]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true

[package.core]
path = "crates/core"
ignored_paths = ["tests/**"]
additional_paths = ["Cargo.lock"]

[package.other]
path = "crates/other"

[cli.verify]

[[cli.verify.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.verify.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.verify.inputs]]
name = "label"
type = "string_list"

[[cli.verify.steps]]
type = "VerifyChangesets"
"#,
	);
	if with_changeset {
		let content = if invalid_changeset {
			format!(
				r"---
{target}: nope
---

#### invalid change
"
			)
		} else {
			format!(
				r"---
{target}: patch
---

#### add feature
"
			)
		};
		write_file(root.join(".changeset/feature.md"), &content);
	}
}

fn write_file(path: impl AsRef<Path>, content: &str) {
	let path = path.as_ref();
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dir: {error}"));
	}
	fs::write(path, content)
		.unwrap_or_else(|error| panic!("write file {}: {error}", path.display()));
}
