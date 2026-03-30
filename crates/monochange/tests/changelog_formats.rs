use std::fs;
use std::path::Path;
use std::process::Command;

use insta_cmd::get_cargo_bin;
use tempfile::tempdir;

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command
}

#[test]
fn release_uses_keep_a_changelog_format_from_defaults() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(
		tempdir.path(),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"

[defaults.changelog]
path = "{path}/CHANGELOG.md"
format = "keep_a_changelog"

[package.core]
path = "crates/core"

[package.app]
path = "crates/app"

[group.sdk]
packages = ["core", "app"]
release = true
tag = true
version_format = "primary"

[group.sdk.changelog]
path = "docs/sdk-CHANGELOG.md"

[ecosystems.cargo]
enabled = true

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
"#,
	);

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(core_changelog.contains("## [1.1.0]"));
	assert!(core_changelog.contains("### Features"));
	assert!(core_changelog.contains("- add keep a changelog support"));
	assert!(app_changelog.contains("## [1.1.0]"));
	assert!(app_changelog.contains("### Features"));
	assert!(group_changelog.contains("## [1.1.0]"));
	assert!(group_changelog.contains("Grouped release for `sdk`."));
	assert!(group_changelog.contains("Members: core, app"));
	assert!(group_changelog.contains("### Features"));
}

#[test]
fn release_allows_package_and_group_changelog_format_overrides() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(
		tempdir.path(),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"

[defaults.changelog]
path = "{path}/CHANGELOG.md"
format = "keep_a_changelog"

[package.core]
path = "crates/core"

[package.app]
path = "crates/app"

[package.app.changelog]
path = "crates/app/CHANGELOG.md"
format = "monochange"

[group.sdk]
packages = ["core", "app"]
release = true
tag = true
version_format = "primary"

[group.sdk.changelog]
path = "docs/sdk-CHANGELOG.md"
format = "monochange"

[ecosystems.cargo]
enabled = true

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
"#,
	);

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(core_changelog.contains("## [1.1.0]"));
	assert!(core_changelog.contains("### Features"));
	assert!(app_changelog.contains("## 1.1.0"));
	assert!(!app_changelog.contains("## [1.1.0]"));
	assert!(app_changelog.contains("### Features"));
	assert!(app_changelog.contains("shares version group `sdk`"));
	assert!(group_changelog.contains("## 1.1.0"));
	assert!(!group_changelog.contains("## [1.1.0]"));
	assert!(group_changelog.contains("Grouped release for `sdk`."));
	assert!(group_changelog.contains("### Features"));
}

fn seed_release_fixture(root: &Path, config: &str) {
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
	write_file(root.join("monochange.toml"), config);
	write_file(
		root.join(".changeset/feature.md"),
		r"---
core: minor
---

#### add keep a changelog support
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
