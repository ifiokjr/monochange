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
fn deploy_workflow_dry_run_renders_deployment_manifest_metadata() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_deploy_fixture(tempdir.path());

	let json = run_json_workflow(tempdir.path(), "release-deploy");
	let deployments = json["deployments"]
		.as_array()
		.unwrap_or_else(|| panic!("expected deployments array"));
	assert_eq!(deployments.len(), 2);
	assert_eq!(deployments[0]["name"], "preview");
	assert_eq!(deployments[0]["trigger"], "workflow");
	assert_eq!(deployments[0]["workflow"], "deploy-preview");
	assert_eq!(deployments[1]["name"], "production");
	assert_eq!(deployments[1]["environment"], "production");
	assert_eq!(deployments[1]["releaseTargets"][0], "sdk");
	assert_eq!(deployments[1]["metadata"]["channel"], "stable");
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

fn seed_deploy_fixture(root: &Path) {
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

[[deployments]]
name = "preview"
trigger = "workflow"
workflow = "deploy-preview"
release_targets = ["sdk"]

[[deployments]]
name = "production"
trigger = "workflow"
workflow = "deploy-production"
environment = "production"
release_targets = ["sdk"]
requires = ["main"]

[deployments.metadata]
channel = "stable"

[ecosystems.cargo]
enabled = true

[[workflows]]
name = "release-deploy"

[[workflows.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[workflows.steps]]
type = "PrepareRelease"

[[workflows.steps]]
type = "Deploy"
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
