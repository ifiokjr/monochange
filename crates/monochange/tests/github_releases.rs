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
fn publish_github_release_dry_run_renders_group_release_payloads() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_group_release_fixture(tempdir.path());

	let json = run_json_workflow(tempdir.path(), "publish-release");
	let github_releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected github releases array"));
	assert_eq!(github_releases.len(), 1);
	assert_eq!(github_releases[0]["repository"], "ifiokjr/monochange");
	assert_eq!(github_releases[0]["targetId"], "sdk");
	assert_eq!(github_releases[0]["targetKind"], "group");
	assert_eq!(github_releases[0]["tagName"], "v1.1.0");
	assert_eq!(github_releases[0]["name"], "sdk 1.1.0");
	assert!(github_releases[0]["body"]
		.as_str()
		.unwrap_or_else(|| panic!("expected group release body"))
		.contains("Grouped release for `sdk`."));

	let manifest = &json["manifest"];
	assert_eq!(manifest["command"], "publish-release");
	assert_eq!(manifest["version"], "1.1.0");
	assert_eq!(manifest["groupVersion"], "1.1.0");
	assert_eq!(manifest["releaseTargets"][0]["id"], "sdk");
	assert_eq!(manifest["releaseTargets"][0]["kind"], "group");
	assert_eq!(manifest["changelogs"][0]["ownerId"], "sdk");
	assert_eq!(manifest["changelogs"][1]["ownerId"], "core");
}

#[test]
fn publish_github_release_dry_run_renders_package_release_payloads() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_release_fixture(tempdir.path());

	let json = run_json_workflow(tempdir.path(), "publish-release");
	let github_releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected github releases array"));
	assert_eq!(github_releases.len(), 2);
	assert_eq!(github_releases[0]["targetId"], "app");
	assert_eq!(github_releases[0]["tagName"], "app/v1.0.1");
	assert_eq!(github_releases[0]["draft"], true);
	assert_eq!(github_releases[0]["prerelease"], true);
	assert_eq!(github_releases[1]["targetId"], "core");
	assert_eq!(github_releases[1]["tagName"], "core/v1.1.0");
	assert!(github_releases[1]["body"]
		.as_str()
		.unwrap_or_else(|| panic!("expected package release body"))
		.contains("add feature"));

	let manifest = &json["manifest"];
	assert!(manifest["version"].is_null());
	assert_eq!(manifest["groupVersion"], Value::Null);
	assert_eq!(manifest["releaseTargets"][0]["id"], "app");
	assert_eq!(manifest["releaseTargets"][1]["id"], "core");
	assert_eq!(manifest["changelogs"][0]["ownerId"], "app");
	assert_eq!(manifest["changelogs"][1]["ownerId"], "core");
	assert_eq!(
		manifest["plan"]["decisions"][0]["trigger"],
		"transitive-dependency"
	);
}

#[test]
fn publish_github_release_dry_run_supports_custom_sections_and_templates() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_custom_release_notes_fixture(tempdir.path());

	let json = run_json_workflow(tempdir.path(), "publish-release");
	let github_releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected github releases array"));
	assert_eq!(github_releases.len(), 1);
	let body = github_releases[0]["body"]
		.as_str()
		.unwrap_or_else(|| panic!("expected GitHub release body"));
	assert!(body.contains("### Security"));
	assert!(body.contains("#### rotate signing keys (core patch)"));
	assert!(body.contains("Roll the signing key before the release window closes."));

	let manifest = &json["manifest"];
	assert_eq!(manifest["changelogs"][0]["ownerId"], "core");
	assert_eq!(
		manifest["changelogs"][0]["notes"]["sections"][0]["title"],
		"Security"
	);
	assert_eq!(manifest["changelogs"][0]["notes"]["sections"][0]["entries"][0], "#### rotate signing keys (core patch)\n\nRoll the signing key before the release window closes.");
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
		root.join("group.toml"),
		"[workspace.package]\nversion = \"1.0.0\"\n[workspace.dependencies]\nworkflow-core = { version = \"1.0.0\" }\nworkflow-app = { version = \"1.0.0\" }\n",
	);
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
versioned_files = ["group.toml"]
tag = true
release = true
version_format = "primary"

[github]
owner = "ifiokjr"
repo = "monochange"

[github.releases]
source = "monochange"

[ecosystems.cargo]
enabled = true

[cli.publish-release]

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
type = "PrepareRelease"

[[cli.publish-release.steps]]
type = "PublishRelease"
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
changelog = "{path}/CHANGELOG.md"

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

[github.releases]
draft = true
prerelease = true
source = "monochange"

[ecosystems.cargo]
enabled = true

[cli.publish-release]

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
type = "PrepareRelease"

[[cli.publish-release.steps]]
type = "PublishRelease"
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

fn seed_custom_release_notes_fixture(root: &Path) {
	write_file(
		root.join("Cargo.toml"),
		r#"
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "1.0.0"
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
	write_file(root.join("crates/core/CHANGELOG.md"), "# Changelog\n");
	write_file(
		root.join("monochange.toml"),
		r#####"
[defaults]
parent_bump = "patch"
package_type = "cargo"
changelog = "{path}/CHANGELOG.md"

[release_notes]
change_templates = ["#### $summary ($package $bump)\n\n$details", "- $summary"]

[package.core]
path = "crates/core"
tag = true
release = true
extra_changelog_sections = [{ name = "Security", types = ["security"] }]

[github]
owner = "ifiokjr"
repo = "monochange"

[github.releases]
source = "monochange"

[ecosystems.cargo]
enabled = true

[cli.publish-release]

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
type = "PrepareRelease"

[[cli.publish-release.steps]]
type = "PublishRelease"
"#####,
	);
	write_file(
		root.join(".changeset/security.md"),
		r"---
core: patch
type:
  core: security
---

#### rotate signing keys

Roll the signing key before the release window closes.
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
