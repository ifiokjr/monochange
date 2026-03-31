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

// ---------------------------------------------------------------------------
// Transitive dependency propagation — ungrouped
// ---------------------------------------------------------------------------

#[test]
fn ungrouped_transitive_bump_writes_empty_update_message_to_dependent_changelog() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_changelog_fixture(tempdir.path(), "patch");

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

	// core got a direct change
	assert!(core_changelog.contains("## 1.1.0"));
	assert!(core_changelog.contains("- add core feature"));

	// app got a transitive patch bump with the built-in fallback
	assert!(app_changelog.contains("## 1.0.1"));
	assert!(app_changelog
		.contains("No package-specific changes were recorded; `workflow-app` was updated to 1.0.1."));
}

#[test]
fn ungrouped_transitive_bump_with_parent_bump_minor_escalates_dependent_version() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_changelog_fixture(tempdir.path(), "minor");

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let json = serde_json::from_slice::<Value>(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json: {error}"));
	let app_decision = json["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("decisions array"))
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains("app"))
		})
		.unwrap_or_else(|| panic!("expected app decision"));

	assert_eq!(app_decision["bump"], "minor");
	assert_eq!(app_decision["trigger"], "transitive-dependency");
	assert_eq!(app_decision["plannedVersion"], "1.1.0");
}

// ---------------------------------------------------------------------------
// Transitive dependency propagation — grouped
// ---------------------------------------------------------------------------

#[test]
fn grouped_transitive_bump_writes_empty_update_message_with_group_reference() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_grouped_changelog_fixture(tempdir.path(), None, None);

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

	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	// app got a version-group-synchronized bump
	assert!(app_changelog.contains("## 1.1.0"));
	assert!(app_changelog.contains(
		"No package-specific changes were recorded; `workflow-app` was updated to 1.1.0 as part of group `sdk`."
	));

	// group changelog includes the direct change from core
	assert!(group_changelog.contains("## 1.1.0"));
	assert!(group_changelog.contains("Grouped release for `sdk`."));
	assert!(group_changelog.contains("- **core**: add core feature"));
}

// ---------------------------------------------------------------------------
// Custom empty_update_message — package-level override
// ---------------------------------------------------------------------------

#[test]
fn custom_empty_update_message_on_package_overrides_default() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_changelog_fixture_with_custom_message(
		tempdir.path(),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"
empty_update_message = "Default fallback for {package} at {version}."

[defaults.changelog]
path = "{path}/CHANGELOG.md"

[package.core]
path = "crates/core"

[package.app]
path = "crates/app"
empty_update_message = "Bumped {package} to {version} because: {reasons}."

[ecosystems.cargo]
enabled = true

[cli.release]

[[cli.release.steps]]
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

	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));

	// the package-level template should win over defaults
	assert!(app_changelog.contains("Bumped workflow-app to 1.0.1 because:"));
	assert!(app_changelog.contains("depends on"));
	assert!(!app_changelog.contains("Default fallback"));
	assert!(!app_changelog.contains("No package-specific changes were recorded"));
}

// ---------------------------------------------------------------------------
// Custom empty_update_message — defaults-level fallback
// ---------------------------------------------------------------------------

#[test]
fn custom_empty_update_message_on_defaults_applies_when_no_package_override() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_changelog_fixture_with_custom_message(
		tempdir.path(),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"
empty_update_message = "Automated bump for {package} ({bump} -> {version})."

[defaults.changelog]
path = "{path}/CHANGELOG.md"

[package.core]
path = "crates/core"

[package.app]
path = "crates/app"

[ecosystems.cargo]
enabled = true

[cli.release]

[[cli.release.steps]]
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

	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));

	assert!(app_changelog.contains("Automated bump for workflow-app (patch -> 1.0.1)."));
}

// ---------------------------------------------------------------------------
// Custom empty_update_message — group-level override
// ---------------------------------------------------------------------------

#[test]
fn custom_empty_update_message_on_group_overrides_defaults_for_grouped_packages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_grouped_changelog_fixture(
		tempdir.path(),
		Some("Release driven by group {group}, version {version}."),
		None,
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

	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));

	assert!(app_changelog.contains("Release driven by group sdk, version 1.1.0."));
	assert!(!app_changelog.contains("No package-specific changes were recorded"));
}

// ---------------------------------------------------------------------------
// Custom change_templates — rendering in changelogs
// ---------------------------------------------------------------------------

#[test]
fn custom_change_templates_render_in_changelog_entries() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_grouped_changelog_fixture(
		tempdir.path(),
		None,
		Some(r#"change_templates = ["- [$bump] $summary"]"#),
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
	let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	// the custom template should produce "- [minor] add core feature"
	assert!(core_changelog.contains("- [minor] add core feature"));
	assert!(group_changelog.contains("- **core**: [minor] add core feature"));
}

// ---------------------------------------------------------------------------
// Transitive dependency JSON plan — verify trigger and reasons
// ---------------------------------------------------------------------------

#[test]
fn transitive_dependency_json_plan_includes_trigger_reasons_and_upstream_sources() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_changelog_fixture(tempdir.path(), "patch");

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let json = serde_json::from_slice::<Value>(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json: {error}"));
	let app_decision = json["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("decisions array"))
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains("app"))
		})
		.unwrap_or_else(|| panic!("expected app decision"));

	assert_eq!(app_decision["bump"], "patch");
	assert_eq!(app_decision["trigger"], "transitive-dependency");
	assert_eq!(app_decision["plannedVersion"], "1.0.1");

	let reasons = app_decision["reasons"]
		.as_array()
		.unwrap_or_else(|| panic!("reasons array"));
	assert!(reasons
		.iter()
		.any(|reason| reason.as_str().is_some_and(|text| text.contains("depends on"))));

	let upstream_sources = app_decision["upstreamSources"]
		.as_array()
		.unwrap_or_else(|| panic!("upstream sources array"));
	assert!(upstream_sources.iter().any(
		|source| source.as_str().is_some_and(|text| text.contains("core"))
	));
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn seed_ungrouped_changelog_fixture(root: &Path, parent_bump: &str) {
	write_workspace_manifests(root);
	write_file(root.join("crates/core/CHANGELOG.md"), "# Changelog\n");
	write_file(root.join("crates/app/CHANGELOG.md"), "# Changelog\n");
	write_file(
		root.join("monochange.toml"),
		&format!(
			r#"
[defaults]
parent_bump = "{parent_bump}"
package_type = "cargo"

[defaults.changelog]
path = "{{path}}/CHANGELOG.md"

[package.core]
path = "crates/core"

[package.app]
path = "crates/app"

[ecosystems.cargo]
enabled = true

[cli.release]

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"
"#
		),
	);
	write_file(
		root.join(".changeset/feature.md"),
		r"---
core: minor
---

#### add core feature
",
	);
}

fn seed_ungrouped_changelog_fixture_with_custom_message(root: &Path, config: &str) {
	write_workspace_manifests(root);
	write_file(root.join("crates/core/CHANGELOG.md"), "# Changelog\n");
	write_file(root.join("crates/app/CHANGELOG.md"), "# Changelog\n");
	write_file(root.join("monochange.toml"), config);
	write_file(
		root.join(".changeset/feature.md"),
		r"---
core: minor
---

#### add core feature
",
	);
}

fn seed_grouped_changelog_fixture(
	root: &Path,
	group_empty_message: Option<&str>,
	release_notes_line: Option<&str>,
) {
	write_workspace_manifests(root);
	write_file(root.join("crates/core/CHANGELOG.md"), "# Changelog\n");
	write_file(root.join("crates/app/CHANGELOG.md"), "# Changelog\n");
	write_file(root.join("changelog.md"), "# Changelog\n");
	let group_empty_msg = group_empty_message
		.map(|message| format!("empty_update_message = \"{message}\""))
		.unwrap_or_default();
	let release_notes = release_notes_line
		.map(|line| format!("[release_notes]\n{line}"))
		.unwrap_or_default();
	write_file(
		root.join("monochange.toml"),
		&format!(
			r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"

[defaults.changelog]
path = "{{path}}/CHANGELOG.md"

{release_notes}

[package.core]
path = "crates/core"

[package.app]
path = "crates/app"

[group.sdk]
packages = ["core", "app"]
changelog = "changelog.md"
tag = true
release = true
version_format = "primary"
{group_empty_msg}

[ecosystems.cargo]
enabled = true

[cli.release]

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"
"#
		),
	);
	write_file(
		root.join(".changeset/feature.md"),
		r"---
core: minor
---

#### add core feature
",
	);
}

fn write_workspace_manifests(root: &Path) {
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
}

fn write_file(path: impl AsRef<Path>, content: &str) {
	let path = path.as_ref();
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dir: {error}"));
	}
	fs::write(path, content)
		.unwrap_or_else(|error| panic!("write file {}: {error}", path.display()));
}
