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
fn open_release_pull_request_dry_run_renders_group_release_preview() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_group_release_fixture(tempdir.path());

	let json = run_json_workflow(tempdir.path(), "release-pr");
	let pull_request = &json["releaseRequest"];
	assert_eq!(pull_request["repository"], "ifiokjr/monochange");
	assert_eq!(pull_request["baseBranch"], "main");
	assert_eq!(pull_request["headBranch"], "monochange/release/release-pr");
	assert_eq!(pull_request["title"], "chore(release): prepare release");
	assert_eq!(pull_request["labels"][0], "release");
	assert_eq!(pull_request["labels"][1], "automated");
	assert!(pull_request["body"]
		.as_str()
		.unwrap_or_else(|| panic!("expected pull request body"))
		.contains("### sdk 1.1.0"));
	assert!(pull_request["body"]
		.as_str()
		.unwrap_or_else(|| panic!("expected pull request body"))
		.contains("#### Features"));

	let manifest = &json["manifest"];
	assert_eq!(manifest["command"], "release-pr");
	assert_eq!(manifest["releaseTargets"][0]["id"], "sdk");
}

#[test]
fn open_release_pull_request_dry_run_renders_package_release_preview() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_release_fixture(tempdir.path());

	let json = run_json_workflow(tempdir.path(), "release-pr");
	let pull_request = &json["releaseRequest"];
	assert_eq!(pull_request["baseBranch"], "develop");
	assert_eq!(pull_request["headBranch"], "automation/release/release-pr");
	assert_eq!(pull_request["autoMerge"], true);
	let body = pull_request["body"]
		.as_str()
		.unwrap_or_else(|| panic!("expected pull request body"));
	assert!(body.contains("### app 1.0.1"));
	assert!(body.contains("### core 1.1.0"));
	assert!(body.contains(
		"No package-specific changes were recorded; `workflow-app` was updated to 1.0.1."
	));
	assert!(body.contains("add feature"));

	let manifest = &json["manifest"];
	assert!(manifest["version"].is_null());
	assert_eq!(manifest["releaseTargets"][0]["id"], "app");
	assert_eq!(manifest["releaseTargets"][1]["id"], "core");
}

fn run_json_workflow(root: &Path, workflow: &str) -> Value {
	let output = cli()
		.current_dir(root)
		.arg(workflow)
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
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

fn seed_group_release_fixture(root: &Path) {
	write_file(
		root.join("Cargo.toml"),
		r#"
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "1.0.0"

[workspace.dependencies]
workflow-core = { path = "./crates/core", version = "1.0.0" }
workflow-app = { path = "./crates/app", version = "1.0.0" }
"#,
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		r#"
[package]
name = "workflow-core"
version = { workspace = true }
edition = "2021"
"#,
	);
	write_file(
		root.join("crates/app/Cargo.toml"),
		r#"
[package]
name = "workflow-app"
version = { workspace = true }
edition = "2021"

[dependencies]
workflow-core = { workspace = true }
"#,
	);
	write_file(root.join("changelog.md"), "# Changelog\n");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"
changelog = false

[package.core]
path = "crates/core"
changelog = true

[package.app]
path = "crates/app"
changelog = false

[group.sdk]
packages = ["core", "app"]
changelog = "changelog.md"
tag = true
release = true
version_format = "primary"

[github]
owner = "ifiokjr"
repo = "monochange"

[github.pull_requests]
enabled = true

[ecosystems.cargo]
enabled = true

[cli.release-pr]

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
"#,
	);
	write_file(
		root.join(".changeset/feature.md"),
		r"---
core: minor
---

#### add feature
",
	);
}

fn seed_ungrouped_release_fixture(root: &Path) {
	write_file(
		root.join("Cargo.toml"),
		r#"
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "1.0.0"

[workspace.dependencies]
workflow-core = { path = "./crates/core", version = "1.0.0" }
workflow-app = { path = "./crates/app", version = "1.0.0" }
"#,
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		r#"
[package]
name = "workflow-core"
version = { workspace = true }
edition = "2021"
"#,
	);
	write_file(
		root.join("crates/app/Cargo.toml"),
		r#"
[package]
name = "workflow-app"
version = { workspace = true }
edition = "2021"

[dependencies]
workflow-core = { workspace = true }
"#,
	);
	write_file(root.join("crates/core/CHANGELOG.md"), "# Changelog\n");
	write_file(root.join("crates/app/CHANGELOG.md"), "# Changelog\n");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"
changelog = "{{ path }}/CHANGELOG.md"

[package.core]
path = "crates/core"
tag = true
release = true

[package.app]
path = "crates/app"
release = true

[github]
owner = "ifiokjr"
repo = "monochange"

[github.pull_requests]
branch_prefix = "automation/release"
base = "develop"
auto_merge = true
labels = ["release", "automated", "preview"]

[ecosystems.cargo]
enabled = true

[cli.release-pr]

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
"#,
	);
	write_file(
		root.join(".changeset/feature.md"),
		r"---
core: minor
---

#### add feature
",
	);
}

fn write_file(path: impl AsRef<Path>, content: &str) {
	let path = path.as_ref();
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dir: {error}"));
	}
	fs::write(path, content)
		.unwrap_or_else(|error| panic!("write file {}: {error}", path.display()));
}
