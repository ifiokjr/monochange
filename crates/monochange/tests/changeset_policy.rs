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
fn changeset_policy_skips_required_changes_when_allowed_label_is_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_policy_fixture(tempdir.path(), false, false);

	let json = run_json_workflow(
		tempdir.path(),
		&[
			"--changed-path",
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
fn changeset_policy_ignores_docs_only_changes() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_policy_fixture(tempdir.path(), false, false);

	let json = run_json_workflow(tempdir.path(), &["--changed-path", "docs/readme.md"]);
	assert_eq!(json["status"], "not_required");
	assert_eq!(json["matchedPaths"].as_array().map(Vec::len), Some(0));
	assert_eq!(json["ignoredPaths"][0], "docs/readme.md");
}

#[test]
fn changeset_policy_validates_changed_changeset_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_policy_fixture(tempdir.path(), true, false);

	let json = run_json_workflow(
		tempdir.path(),
		&[
			"--changed-path",
			"crates/core/src/lib.rs",
			"--changed-path",
			".changeset/feature.md",
		],
	);
	assert_eq!(json["status"], "passed");
	assert_eq!(json["changesetPaths"][0], ".changeset/feature.md");
}

#[test]
fn changeset_policy_reports_invalid_changed_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_policy_fixture(tempdir.path(), true, true);

	let json = run_json_workflow(
		tempdir.path(),
		&[
			"--changed-path",
			"crates/core/src/lib.rs",
			"--changed-path",
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
		.is_some_and(|comment| comment.contains("Changed changeset files:")));
}

fn run_json_workflow(root: &Path, args: &[&str]) -> Value {
	let output = cli()
		.current_dir(root)
		.arg("changeset-check")
		.arg("--format")
		.arg("json")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("workflow output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse workflow json: {error}"))
}

fn seed_policy_fixture(root: &Path, with_changeset: bool, invalid_changeset: bool) {
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
	write_file(root.join("crates/core/src/lib.rs"), "pub fn core() {}\n");
	write_file(root.join("docs/readme.md"), "# docs\n");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[github]
owner = "ifiokjr"
repo = "monochange"

[github.bot.changesets]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["crates/**"]
ignored_paths = ["docs/**", "*.md"]

[[workflows]]
name = "changeset-check"

[[workflows.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[workflows.inputs]]
name = "changed_path"
type = "string_list"
required = true

[[workflows.inputs]]
name = "label"
type = "string_list"

[[workflows.steps]]
type = "EnforceChangesetPolicy"
"#,
	);
	if with_changeset {
		let content = if invalid_changeset {
			r"---
core: nope
---

#### invalid change
"
		} else {
			r"---
core: patch
---

#### add feature
"
		};
		write_file(root.join(".changeset/feature.md"), content);
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
