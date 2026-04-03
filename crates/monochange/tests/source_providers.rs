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
fn publish_release_dry_run_supports_gitlab_sources() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_gitlab_release_fixture(tempdir.path());

	let json = run_json_command(tempdir.path(), "publish-release");
	let releases = json["releases"]
		.as_array()
		.unwrap_or_else(|| panic!("expected releases array"));
	assert_eq!(releases.len(), 1);
	assert_eq!(releases[0]["provider"], "gitlab");
	assert_eq!(releases[0]["repository"], "group/monochange");
}

#[test]
fn release_pr_dry_run_supports_gitea_sources() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_gitea_release_pr_fixture(tempdir.path());

	let json = run_json_command(tempdir.path(), "release-pr");
	let release_request = &json["releaseRequest"];
	assert_eq!(release_request["provider"], "gitea");
	assert_eq!(release_request["repository"], "org/monochange");
	assert_eq!(release_request["baseBranch"], "main");
}

fn run_json_command(root: &Path, command: &str) -> Value {
	let output = cli()
		.current_dir(root)
		.arg(command)
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json output: {error}"))
}

fn seed_gitlab_release_fixture(root: &Path) {
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
		r#"
[defaults]
package_type = "cargo"
changelog = "{{ path }}/CHANGELOG.md"

[package.core]
path = "crates/core"
tag = true
release = true

[source]
provider = "gitlab"
owner = "group"
repo = "monochange"
host = "https://gitlab.com"

[source.releases]
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

#### add gitlab support
",
	);
}

fn seed_gitea_release_pr_fixture(root: &Path) {
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
		r#"
[defaults]
package_type = "cargo"
changelog = "{{ path }}/CHANGELOG.md"

[package.core]
path = "crates/core"
tag = true
release = true

[source]
provider = "gitea"
owner = "org"
repo = "monochange"
host = "https://codeberg.org"

[source.pull_requests]
base = "main"
branch_prefix = "monochange/release"
labels = ["release"]

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
core: patch
---

#### add gitea support
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
