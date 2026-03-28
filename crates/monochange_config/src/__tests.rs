use std::fs;

use monochange_core::BumpSeverity;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use tempfile::tempdir;

use crate::apply_version_groups;
use crate::load_change_signals;
use crate::load_workspace_configuration;

#[test]
fn load_workspace_configuration_uses_defaults_when_file_is_missing() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	assert_eq!(configuration.defaults.parent_bump, BumpSeverity::Patch);
	assert!(!configuration.defaults.include_private);
	assert!(configuration.defaults.warn_on_group_mismatch);
	assert!(configuration.version_groups.is_empty());
	assert!(configuration.workflows.is_empty());
	assert_eq!(configuration.cargo.enabled, None);
	assert_eq!(configuration.npm.enabled, None);
	assert_eq!(configuration.deno.enabled, None);
	assert_eq!(configuration.dart.enabled, None);
}

#[test]
fn load_workspace_configuration_parses_version_groups_and_ecosystem_settings() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
parent_bump = "minor"
include_private = true

[[version_groups]]
name = "sdk"
members = ["crates/core", "packages/web"]

[ecosystems.npm]
enabled = true
roots = ["packages/*"]

[[package_overrides]]
package = "crates/core"
changelog = "crates/core/CHANGELOG.md"

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"

[[workflows.steps]]
type = "Command"
command = "cargo check --workspace"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.defaults.parent_bump, BumpSeverity::Minor);
	assert!(configuration.defaults.include_private);
	assert_eq!(configuration.version_groups.len(), 1);
	assert_eq!(configuration.npm.roots, vec!["packages/*"]);
	assert_eq!(configuration.package_overrides.len(), 1);
	assert_eq!(configuration.workflows.len(), 1);
	let workflow = configuration
		.workflows
		.first()
		.unwrap_or_else(|| panic!("expected one workflow"));
	assert_eq!(workflow.name, "release");
	let package_override = configuration
		.package_overrides
		.first()
		.unwrap_or_else(|| panic!("expected one package override"));
	assert_eq!(package_override.package, "crates/core");
	assert_eq!(
		package_override
			.changelog
			.as_ref()
			.and_then(|path| path.to_str()),
		Some("crates/core/CHANGELOG.md")
	);
}

#[test]
fn apply_version_groups_assigns_group_ids_and_detects_mismatched_versions() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[[version_groups]]
name = "sdk"
members = ["crates/core", "packages/web"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			tempdir.path().join("crates/core/Cargo.toml"),
			tempdir.path().to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Npm,
			"web",
			tempdir.path().join("packages/web/package.json"),
			tempdir.path().to_path_buf(),
			Some(Version::new(2, 0, 0)),
			PublishState::Public,
		),
	];

	let (groups, warnings) = apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));
	let group = groups
		.first()
		.unwrap_or_else(|| panic!("expected one version group"));
	let first_package = packages
		.first()
		.unwrap_or_else(|| panic!("expected first package"));
	let second_package = packages
		.get(1)
		.unwrap_or_else(|| panic!("expected second package"));

	assert_eq!(group.members.len(), 2);
	assert!(group.mismatch_detected);
	assert_eq!(first_package.version_group_id.as_deref(), Some("sdk"));
	assert_eq!(second_package.version_group_id.as_deref(), Some("sdk"));
	assert_eq!(warnings.len(), 1);
}

#[test]
fn load_change_signals_resolves_package_references_and_evidence() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("change.md"),
		r"---
crates/core: minor
evidence:
  crates/core:
    - rust-semver:major:public API break detected
---

#### public API addition
",
	)
	.unwrap_or_else(|error| panic!("changes write: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-core",
		tempdir.path().join("crates/core/Cargo.toml"),
		tempdir.path().to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let signals = load_change_signals(&tempdir.path().join("change.md"), tempdir.path(), &packages)
		.unwrap_or_else(|error| panic!("change signals: {error}"));
	let signal = signals
		.first()
		.unwrap_or_else(|| panic!("expected one change signal"));

	let package = packages
		.first()
		.unwrap_or_else(|| panic!("expected one discovered package"));
	assert_eq!(signal.package_id, package.id);
	assert_eq!(signal.requested_bump, Some(BumpSeverity::Minor));
	assert_eq!(signal.notes.as_deref(), Some("public API addition"));
	assert_eq!(
		signal.evidence_refs,
		vec!["rust-semver:major:public API break detected"]
	);
}

#[test]
fn load_workspace_configuration_rejects_reserved_workflow_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[[workflows]]
name = "workspace"

[[workflows.steps]]
type = "PrepareRelease"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(error.to_string().contains("reserved built-in command"));
}

#[test]
fn load_workspace_configuration_rejects_duplicate_workflows() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"

[[workflows]]
name = "release"

[[workflows.steps]]
type = "Command"
command = "cargo check"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(error.to_string().contains("duplicate workflow `release`"));
}

#[test]
fn load_change_signals_rejects_unknown_package_references() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("change.md"),
		r"---
missing-package: patch
---

#### unknown package
",
	)
	.unwrap_or_else(|error| panic!("changes write: {error}"));

	let error = load_change_signals(&tempdir.path().join("change.md"), tempdir.path(), &[])
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(error
		.to_string()
		.contains("did not match any discovered package"));
}
