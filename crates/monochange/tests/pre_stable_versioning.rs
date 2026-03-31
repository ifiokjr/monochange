use std::fs;
use std::path::Path;
use std::process::Command;

use insta_cmd::assert_cmd_snapshot;
use insta_cmd::get_cargo_bin;
use serde_json::Value;
use tempfile::tempdir;

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command
}

macro_rules! apply_common_filters {
	() => {
		let _filters = {
			let mut settings = insta::Settings::clone_current();
			settings.add_filter(r"/var/folders/[^\s]+?/T/[^/\s]+", "[ROOT]");
			settings.add_filter(r"/tmp/[^/\s]+", "[ROOT]");
			settings.add_filter(r"/home/runner/work/_temp/[^/\s]+", "[ROOT]");
			settings.add_filter(r"\b[A-Z]:\\[^\s]+?\\Temp\\[^\\\s]+", "[ROOT]");
			settings.add_filter(r"SourceOffset\(\d+\)", "SourceOffset([OFFSET])");
			settings.add_filter(r"length: \d+", "length: [LEN]");
			settings.add_filter(r"@ bytes \d+\.\.\d+", "@ bytes [OFFSET]..[END]");
			settings.bind_to_scope()
		};
	};
}

// ---------------------------------------------------------------------------
// Pre-1.0 major bump → bumps minor (text output)
// ---------------------------------------------------------------------------

#[test]
fn pre_stable_major_bump_produces_minor_version_in_text_output() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_pre_stable_fixture(tempdir.path(), "major");

	assert_cmd_snapshot!(cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("text"));
}

// ---------------------------------------------------------------------------
// Pre-1.0 major bump → bumps minor (JSON output)
// ---------------------------------------------------------------------------

#[test]
fn pre_stable_major_bump_produces_minor_version_in_json_output() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_pre_stable_fixture(tempdir.path(), "major");

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

	// core: major requested on 0.1.0 → planned 0.2.0
	let core_decision = find_decision(&json, "core");
	assert_eq!(core_decision["bump"], "major");
	assert_eq!(core_decision["plannedVersion"], "0.2.0");
	assert_eq!(core_decision["trigger"], "direct-change");

	// app: transitive patch on 0.1.0 → planned 0.1.1
	let app_decision = find_decision(&json, "app");
	assert_eq!(app_decision["bump"], "patch");
	assert_eq!(app_decision["plannedVersion"], "0.1.1");
	assert_eq!(app_decision["trigger"], "transitive-dependency");
}

// ---------------------------------------------------------------------------
// Pre-1.0 minor bump → bumps patch
// ---------------------------------------------------------------------------

#[test]
fn pre_stable_minor_bump_produces_patch_version_in_json_output() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_pre_stable_fixture(tempdir.path(), "minor");

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

	// core: minor requested on 0.1.0 → planned 0.1.1
	let core_decision = find_decision(&json, "core");
	assert_eq!(core_decision["bump"], "minor");
	assert_eq!(core_decision["plannedVersion"], "0.1.1");
}

// ---------------------------------------------------------------------------
// Post-1.0 major bump → normal major
// ---------------------------------------------------------------------------

#[test]
fn stable_major_bump_produces_next_major_version_in_text_output() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_stable_fixture(tempdir.path(), "major");

	assert_cmd_snapshot!(cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("text"));
}

#[test]
fn stable_major_bump_produces_next_major_version_in_json_output() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_stable_fixture(tempdir.path(), "major");

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

	let core_decision = find_decision(&json, "core");
	assert_eq!(core_decision["bump"], "major");
	assert_eq!(core_decision["plannedVersion"], "2.0.0");
}

// ---------------------------------------------------------------------------
// Pre-1.0 grouped release — group version reflects shifted bump
// ---------------------------------------------------------------------------

#[test]
fn pre_stable_grouped_major_bump_shifts_group_version() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_pre_stable_grouped_fixture(tempdir.path(), "major");

	assert_cmd_snapshot!(cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("text"));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_decision<'a>(json: &'a Value, package_name_fragment: &str) -> &'a Value {
	json["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("decisions array"))
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains(package_name_fragment))
		})
		.unwrap_or_else(|| panic!("expected decision for {package_name_fragment}"))
}

fn seed_pre_stable_fixture(root: &Path, bump: &str) {
	write_workspace(root, "0.1.0");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"

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
"#,
	);
	write_file(
		root.join(".changeset/feature.md"),
		&format!("---\ncore: {bump}\n---\n\n#### breaking change in core\n"),
	);
}

fn seed_stable_fixture(root: &Path, bump: &str) {
	write_workspace(root, "1.0.0");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"

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
"#,
	);
	write_file(
		root.join(".changeset/feature.md"),
		&format!("---\ncore: {bump}\n---\n\n#### breaking change in core\n"),
	);
}

fn seed_pre_stable_grouped_fixture(root: &Path, bump: &str) {
	write_workspace(root, "0.1.0");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"

[package.core]
path = "crates/core"

[package.app]
path = "crates/app"

[group.sdk]
packages = ["core", "app"]
tag = true
release = true
version_format = "primary"

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
"#,
	);
	write_file(
		root.join(".changeset/feature.md"),
		&format!("---\ncore: {bump}\n---\n\n#### breaking change in core\n"),
	);
}

fn write_workspace(root: &Path, version: &str) {
	write_file(
		root.join("Cargo.toml"),
		&format!(
			r#"
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "{version}"

[workspace.dependencies]
workflow-core = {{ path = "./crates/core", version = "{version}" }}
workflow-app = {{ path = "./crates/app", version = "{version}" }}
"#
		),
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
