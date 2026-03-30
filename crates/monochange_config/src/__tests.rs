use std::fs;

use monochange_core::BumpSeverity;
use monochange_core::ChangelogDefinition;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogTarget;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use tempfile::tempdir;

use crate::apply_version_groups;
use crate::load_change_signals;
use crate::load_workspace_configuration;
use crate::validate_workspace;

#[test]
fn load_workspace_configuration_uses_defaults_when_file_is_missing() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	assert_eq!(configuration.defaults.parent_bump, BumpSeverity::Patch);
	assert!(!configuration.defaults.include_private);
	assert!(configuration.defaults.warn_on_group_mismatch);
	assert_eq!(configuration.defaults.package_type, None);
	assert_eq!(configuration.defaults.changelog, None);
	assert_eq!(
		configuration.defaults.changelog_format,
		ChangelogFormat::Monochange
	);
	assert!(configuration.packages.is_empty());
	assert!(configuration.groups.is_empty());
	assert_eq!(configuration.workflows.len(), 4);
	let workflow_names = configuration
		.workflows
		.iter()
		.map(|workflow| workflow.name.as_str())
		.collect::<Vec<_>>();
	assert_eq!(
		workflow_names,
		vec!["validate", "discover", "change", "release"]
	);
	assert_eq!(configuration.cargo.enabled, None);
	assert_eq!(configuration.npm.enabled, None);
	assert_eq!(configuration.deno.enabled, None);
	assert_eq!(configuration.dart.enabled, None);
}

#[test]
fn load_workspace_configuration_parses_package_group_and_workflow_declarations() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	write_npm_package(tempdir.path(), "packages/web", "web");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
parent_bump = "minor"
include_private = true
package_type = "cargo"
changelog = "{path}/CHANGELOG.md"

[package.core]
path = "crates/core"
changelog = "crates/core/changelog.md"
tag = true
release = true

[package."npm:web"]
path = "packages/web"
type = "npm"

[group.sdk]
packages = ["core", "npm:web"]
changelog = "changelog.md"
version_format = "primary"

[ecosystems.npm]
enabled = true
roots = ["packages/*"]

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"

[[workflows.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.defaults.parent_bump, BumpSeverity::Minor);
	assert!(configuration.defaults.include_private);
	assert_eq!(
		configuration.defaults.package_type,
		Some(monochange_core::PackageType::Cargo)
	);
	assert_eq!(
		configuration.defaults.changelog,
		Some(ChangelogDefinition::PathPattern(
			"{path}/CHANGELOG.md".to_string()
		))
	);
	assert_eq!(
		configuration.defaults.changelog_format,
		ChangelogFormat::Monochange
	);
	assert_eq!(configuration.packages.len(), 2);
	assert_eq!(configuration.groups.len(), 1);
	assert_eq!(configuration.workflows.len(), 1);
	assert_eq!(
		configuration
			.workflows
			.first()
			.unwrap_or_else(|| panic!("expected workflow"))
			.steps
			.len(),
		2
	);
	assert_eq!(configuration.npm.roots, vec!["packages/*"]);
	let first_package = configuration
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected first package"));
	assert_eq!(first_package.id, "core");
	assert_eq!(
		first_package.changelog,
		Some(ChangelogTarget {
			path: std::path::PathBuf::from("crates/core/changelog.md"),
			format: ChangelogFormat::Monochange,
		})
	);
	assert_eq!(
		configuration
			.groups
			.first()
			.unwrap_or_else(|| panic!("expected group"))
			.packages,
		vec!["core", "npm:web"]
	);
}

#[test]
fn load_workspace_configuration_parses_github_release_settings() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[github]
owner = "ifiokjr"
repo = "monochange"

[github.releases]
enabled = true
draft = true
prerelease = true
source = "github_generated"
generate_notes = true
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let github = configuration
		.github
		.unwrap_or_else(|| panic!("expected github config"));
	assert_eq!(github.owner, "ifiokjr");
	assert_eq!(github.repo, "monochange");
	assert!(github.releases.enabled);
	assert!(github.releases.draft);
	assert!(github.releases.prerelease);
	assert!(github.releases.generate_notes);
	assert_eq!(
		github.releases.source,
		monochange_core::GitHubReleaseNotesSource::GitHubGenerated
	);
}

#[test]
fn load_workspace_configuration_uses_defaults_package_type_when_type_is_omitted() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected package"));
	assert_eq!(package.package_type, monochange_core::PackageType::Cargo);
}

#[test]
fn load_workspace_configuration_uses_defaults_changelog_pattern_when_package_changelog_is_omitted()
{
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"
changelog = "{path}/changelog.md"

[package.core]
path = "crates/core"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected package"));
	assert_eq!(
		configuration.defaults.changelog,
		Some(ChangelogDefinition::PathPattern(
			"{path}/changelog.md".to_string()
		))
	);
	assert_eq!(
		package.changelog,
		Some(ChangelogTarget {
			path: std::path::PathBuf::from("crates/core/changelog.md"),
			format: ChangelogFormat::Monochange,
		})
	);
}

#[test]
fn load_workspace_configuration_supports_package_changelog_true_false_and_string() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	write_cargo_package(tempdir.path(), "crates/app", "app");
	write_cargo_package(tempdir.path(), "crates/tool", "tool");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"
changelog = false

[package.core]
path = "crates/core"
changelog = true

[package.app]
path = "crates/app"
changelog = false

[package.tool]
path = "crates/tool"
changelog = "docs/tool-release-notes.md"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let core = configuration
		.package_by_id("core")
		.unwrap_or_else(|| panic!("expected core package"));
	let app = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("expected app package"));
	let tool = configuration
		.package_by_id("tool")
		.unwrap_or_else(|| panic!("expected tool package"));

	assert_eq!(
		configuration.defaults.changelog,
		Some(ChangelogDefinition::Disabled)
	);
	assert_eq!(
		core.changelog,
		Some(ChangelogTarget {
			path: std::path::PathBuf::from("crates/core/CHANGELOG.md"),
			format: ChangelogFormat::Monochange,
		})
	);
	assert_eq!(app.changelog, None);
	assert_eq!(
		tool.changelog,
		Some(ChangelogTarget {
			path: std::path::PathBuf::from("docs/tool-release-notes.md"),
			format: ChangelogFormat::Monochange,
		})
	);
}

#[test]
fn load_workspace_configuration_supports_changelog_format_tables_and_overrides() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	write_cargo_package(tempdir.path(), "crates/app", "app");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[defaults.changelog]
path = "{path}/CHANGELOG.md"
format = "keep_a_changelog"

[package.core]
path = "crates/core"

[package.app]
path = "crates/app"

[package.app.changelog]
path = "docs/app-release-notes.md"
format = "monochange"

[group.sdk]
packages = ["core", "app"]

[group.sdk.changelog]
path = "docs/group-release-notes.md"
format = "keep_a_changelog"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let core = configuration
		.package_by_id("core")
		.unwrap_or_else(|| panic!("expected core package"));
	let app = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("expected app package"));
	let group = configuration
		.groups
		.first()
		.unwrap_or_else(|| panic!("expected group"));

	assert_eq!(
		configuration.defaults.changelog_format,
		ChangelogFormat::KeepAChangelog
	);
	assert_eq!(
		core.changelog,
		Some(ChangelogTarget {
			path: std::path::PathBuf::from("crates/core/CHANGELOG.md"),
			format: ChangelogFormat::KeepAChangelog,
		})
	);
	assert_eq!(
		app.changelog,
		Some(ChangelogTarget {
			path: std::path::PathBuf::from("docs/app-release-notes.md"),
			format: ChangelogFormat::Monochange,
		})
	);
	assert_eq!(
		group.changelog,
		Some(ChangelogTarget {
			path: std::path::PathBuf::from("docs/group-release-notes.md"),
			format: ChangelogFormat::KeepAChangelog,
		})
	);
}

#[test]
fn load_workspace_configuration_rejects_group_changelog_tables_without_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[group.sdk]
packages = ["core"]

[group.sdk.changelog]
enabled = true
format = "keep_a_changelog"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();
	assert!(rendered.contains("group `sdk` changelog must declare a `path`"));
	assert!(rendered.contains("group changelog missing path"));
}

#[test]
fn migration_guide_new_style_example_loads_successfully() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/monochange", "monochange");
	write_cargo_package(tempdir.path(), "crates/monochange_core", "monochange_core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.monochange]
path = "crates/monochange"
changelog = "crates/monochange/changelog.md"

[package.monochange_core]
path = "crates/monochange_core"
changelog = "crates/monochange_core/changelog.md"

[group.main]
packages = ["monochange", "monochange_core"]
tag = true
release = true
version_format = "primary"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.packages.len(), 2);
	assert_eq!(configuration.groups.len(), 1);
	let group = configuration
		.groups
		.first()
		.unwrap_or_else(|| panic!("expected migration group"));
	assert_eq!(group.id, "main");
	assert_eq!(group.packages, vec!["monochange", "monochange_core"]);
}

#[test]
fn load_workspace_configuration_requires_package_type_without_default() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("must declare `type` or set `[defaults].package_type`"));
	assert!(rendered.contains("single-ecosystem repository"));
}

#[test]
fn load_workspace_configuration_rejects_package_group_namespace_collisions() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.sdk]
path = "crates/core"
type = "cargo"

[group.sdk]
packages = ["sdk"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("collides with an existing package or group id"));
	assert!(rendered.contains("package and group ids share one namespace"));
}

#[test]
fn load_workspace_configuration_rejects_unknown_group_members() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"

[group.sdk]
packages = ["core", "missing"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("references unknown package `missing`"));
	assert!(rendered.contains("declare the package first under [package.<id>]"));
}

#[test]
fn load_workspace_configuration_rejects_multi_group_membership() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	write_cargo_package(tempdir.path(), "crates/other", "other");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"

[package.other]
path = "crates/other"
type = "cargo"

[group.sdk]
packages = ["core"]

[group.cli]
packages = ["core", "other"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("error: package `core` belongs to multiple groups"));
	assert!(rendered.contains("--> monochange.toml"));
	assert!(rendered.contains("labels:"));
	assert!(rendered.contains("move the package into exactly one [group.<id>] declaration"));
}

#[test]
fn load_workspace_configuration_rejects_duplicate_primary_version_format() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	write_cargo_package(tempdir.path(), "crates/other", "other");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"
version_format = "primary"

[package.other]
path = "crates/other"
type = "cargo"

[group.sdk]
packages = ["other"]
version_format = "primary"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("primary release identity"));
	assert!(rendered
		.contains("choose a single package or group as the primary outward release identity"));
}

#[test]
fn load_workspace_configuration_rejects_unknown_versioned_file_dependencies() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"
versioned_files = [{ path = "Cargo.lock", dependency = "missing" }]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("unknown versioned file dependency `missing`"));
	assert!(rendered.contains("reference a declared package id from `versioned_files`"));
}

#[test]
fn apply_version_groups_assigns_group_ids_and_detects_mismatched_versions() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	write_npm_package(tempdir.path(), "packages/web", "web");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"

[package.web]
path = "packages/web"
type = "npm"

[group.sdk]
packages = ["core", "web"]
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

	assert_eq!(group.group_id, "sdk");
	assert_eq!(group.members.len(), 2);
	assert!(group.mismatch_detected);
	let first_package = packages
		.first()
		.unwrap_or_else(|| panic!("expected first package"));
	let second_package = packages
		.get(1)
		.unwrap_or_else(|| panic!("expected second package"));
	assert_eq!(first_package.version_group_id.as_deref(), Some("sdk"));
	assert_eq!(second_package.version_group_id.as_deref(), Some("sdk"));
	assert_eq!(
		first_package.metadata.get("config_id").map(String::as_str),
		Some("core")
	);
	assert_eq!(
		second_package.metadata.get("config_id").map(String::as_str),
		Some("web")
	);
	assert_eq!(warnings.len(), 1);
}

#[test]
fn load_change_signals_resolves_configured_package_ids_and_evidence() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	fs::write(
		tempdir.path().join("change.md"),
		r"---
core: minor
evidence:
  core:
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
	let mut packages = packages;
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let signals = load_change_signals(&tempdir.path().join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("change signals: {error}"));
	let signal = signals
		.first()
		.unwrap_or_else(|| panic!("expected one change signal"));

	let package = packages
		.first()
		.unwrap_or_else(|| panic!("expected discovered package"));
	assert_eq!(signal.package_id, package.id);
	assert_eq!(signal.requested_bump, Some(BumpSeverity::Minor));
	assert_eq!(signal.notes.as_deref(), Some("public API addition"));
	assert_eq!(
		signal.evidence_refs,
		vec!["rust-semver:major:public API break detected"]
	);
}

#[test]
fn load_change_signals_parses_markdown_change_types_and_details() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	fs::write(
		tempdir.path().join("change.md"),
		r"---
core: patch
type:
  core: security
---

#### rotate signing keys

Roll the signing key before the release window closes.
",
	)
	.unwrap_or_else(|error| panic!("changes write: {error}"));
	let mut packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-core",
		tempdir.path().join("crates/core/Cargo.toml"),
		tempdir.path().to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let signals = load_change_signals(&tempdir.path().join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("change signals: {error}"));
	let signal = signals.first().unwrap_or_else(|| panic!("expected signal"));

	assert_eq!(signal.change_type.as_deref(), Some("security"));
	assert_eq!(
		signal.details.as_deref(),
		Some("Roll the signing key before the release window closes.")
	);
}

#[test]
fn load_change_signals_expands_group_targets_into_member_packages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	write_npm_package(tempdir.path(), "packages/web", "web");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"

[package.web]
path = "packages/web"
type = "npm"

[group.sdk]
packages = ["core", "web"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	fs::write(
		tempdir.path().join("change.md"),
		r"---
sdk: minor
---

#### grouped release
",
	)
	.unwrap_or_else(|error| panic!("changes write: {error}"));

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
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let signals = load_change_signals(&tempdir.path().join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("change signals: {error}"));

	assert_eq!(signals.len(), 2);
	assert!(signals
		.iter()
		.all(|signal| signal.requested_bump == Some(BumpSeverity::Minor)));
}

#[test]
fn validate_workspace_rejects_changesets_that_mix_group_and_member_references() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::create_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset dir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"

[group.sdk]
packages = ["core"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	fs::write(
		tempdir.path().join(".changeset/feature.md"),
		r"---
sdk: minor
core: patch
---

#### overlap
",
	)
	.unwrap_or_else(|error| panic!("changeset write: {error}"));

	let error = validate_workspace(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected changeset validation error"));
	let rendered = error.render();

	assert!(rendered.contains("references both group `sdk` and member package `core`"));
	assert!(rendered.contains("reference either the group or one of its member packages"));
}

#[test]
fn load_workspace_configuration_rejects_publish_github_release_without_github_config() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[[workflows]]
name = "publish"

[[workflows.steps]]
type = "PrepareRelease"

[[workflows.steps]]
type = "PublishGitHubRelease"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected github workflow config error"));
	assert!(error
		.to_string()
		.contains("uses `PublishGitHubRelease` but `[github]` is not configured"));
}

#[test]
fn load_workspace_configuration_parses_release_note_customization() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#####"
[defaults]
package_type = "cargo"

[release_notes]
change_templates = ["#### $summary ($package $bump)\n\n$details", "- $summary"]

[package.core]
path = "crates/core"
extra_changelog_sections = [{ name = "Security", types = ["security"] }]
"#####,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.package_by_id("core")
		.unwrap_or_else(|| panic!("expected package"));

	assert_eq!(configuration.release_notes.change_templates.len(), 2);
	assert_eq!(package.extra_changelog_sections.len(), 1);
	let extra_section = package
		.extra_changelog_sections
		.first()
		.unwrap_or_else(|| panic!("expected extra changelog section"));
	assert_eq!(extra_section.name, "Security");
	assert_eq!(extra_section.types, vec!["security"]);
}

#[test]
fn load_workspace_configuration_rejects_empty_extra_changelog_section_types() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"
extra_changelog_sections = [{ name = "Security", types = [] }]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	let rendered = error.render();

	assert!(rendered.contains("extra changelog section `Security` must declare at least one type"));
}

#[test]
fn load_workspace_configuration_rejects_unknown_change_template_variables() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[release_notes]
change_templates = ["- $summary ($commit_hash)"]

[package.core]
path = "crates/core"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(error
		.render()
		.contains("unsupported variables: commit_hash"));
}

#[test]
fn load_workspace_configuration_rejects_reserved_workflow_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[[workflows]]
name = "init"

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
fn load_change_signals_rejects_unknown_package_references_with_diagnostic_help() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	fs::write(
		tempdir.path().join("change.md"),
		r"---
missing-package: patch
---

#### unknown package
",
	)
	.unwrap_or_else(|error| panic!("changes write: {error}"));

	let error = load_change_signals(
		&tempdir.path().join("change.md"),
		&load_workspace_configuration(tempdir.path())
			.unwrap_or_else(|error| panic!("configuration: {error}")),
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("error: changeset `"));
	assert!(rendered.contains("unknown package or group `missing-package`"));
	assert!(rendered.contains("help: declare the package or group id in monochange.toml"));
}

fn write_cargo_package(root: &std::path::Path, relative_dir: &str, name: &str) {
	let dir = root.join(relative_dir);
	fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create cargo dir: {error}"));
	fs::write(
		dir.join("Cargo.toml"),
		format!("[package]\nname = \"{name}\"\nversion = \"1.0.0\"\n"),
	)
	.unwrap_or_else(|error| panic!("cargo manifest: {error}"));
}

fn write_npm_package(root: &std::path::Path, relative_dir: &str, name: &str) {
	let dir = root.join(relative_dir);
	fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create npm dir: {error}"));
	fs::write(
		dir.join("package.json"),
		format!("{{\"name\":\"{name}\",\"version\":\"1.0.0\"}}"),
	)
	.unwrap_or_else(|error| panic!("package.json: {error}"));
}
