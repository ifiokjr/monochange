use std::fs;
use std::path::{Path, PathBuf};

use monochange_core::BumpSeverity;
use monochange_core::ChangelogDefinition;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogTarget;
use monochange_core::CliStepDefinition;
use monochange_core::Ecosystem;
use monochange_core::EcosystemType;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use tempfile::tempdir;

use crate::apply_version_groups;
use crate::load_change_signals;
use crate::load_changeset_file;
use crate::load_workspace_configuration;
use crate::resolve_package_reference;
use crate::validate_workspace;

#[test]
fn load_workspace_configuration_uses_defaults_when_file_is_missing() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	assert_eq!(configuration.defaults.parent_bump, BumpSeverity::Patch);
	assert!(!configuration.defaults.include_private);
	assert!(configuration.defaults.warn_on_group_mismatch);
	assert!(!configuration.defaults.strict_version_conflicts);
	assert_eq!(configuration.defaults.package_type, None);
	assert_eq!(configuration.defaults.changelog, None);
	assert_eq!(
		configuration.defaults.changelog_format,
		ChangelogFormat::Monochange
	);
	assert_eq!(configuration.defaults.empty_update_message, None);
	assert!(configuration.packages.is_empty());
	assert!(configuration.groups.is_empty());
	assert_eq!(configuration.cli.len(), 6);
	let cli_command_names = configuration
		.cli
		.iter()
		.map(|cli_command| cli_command.name.as_str())
		.collect::<Vec<_>>();
	assert_eq!(
		cli_command_names,
		vec![
			"validate",
			"discover",
			"change",
			"release",
			"affected",
			"diagnostics"
		]
	);
	assert_eq!(configuration.cargo.enabled, None);
	assert_eq!(configuration.npm.enabled, None);
	assert_eq!(configuration.deno.enabled, None);
	assert_eq!(configuration.dart.enabled, None);
}

#[test]
fn load_workspace_configuration_supports_diagnostics_cli_command_definition() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[cli.diagnostics]
help_text = "Show changeset diagnostics and context"

[[cli.diagnostics.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.diagnostics.inputs]]
name = "changeset"
type = "string_list"

[[cli.diagnostics.steps]]
type = "DiagnoseChangesets"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let diagnostics = configuration
		.cli
		.iter()
		.find(|command| command.name == "diagnostics")
		.unwrap_or_else(|| panic!("expected diagnostics command"));
	assert_eq!(diagnostics.steps.len(), 1);
	match diagnostics.steps.first() {
		Some(CliStepDefinition::DiagnoseChangesets { .. }) => {}
		Some(_) => panic!("expected DiagnoseChangesets step"),
		None => panic!("expected diagnostics step"),
	}
}

#[test]
fn load_workspace_configuration_parses_package_group_and_cli_command_declarations() {
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
changelog = "{{ path }}/CHANGELOG.md"

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

[cli.release]

[[cli.release.steps]]
type = "PrepareRelease"

[[cli.release.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.defaults.parent_bump, BumpSeverity::Minor);
	assert!(configuration.defaults.include_private);
	assert!(!configuration.defaults.strict_version_conflicts);
	assert_eq!(
		configuration.defaults.package_type,
		Some(monochange_core::PackageType::Cargo)
	);
	assert_eq!(
		configuration.defaults.changelog,
		Some(ChangelogDefinition::PathPattern(
			"{{ path }}/CHANGELOG.md".to_string()
		))
	);
	assert_eq!(
		configuration.defaults.changelog_format,
		ChangelogFormat::Monochange
	);
	assert_eq!(configuration.packages.len(), 2);
	assert_eq!(configuration.groups.len(), 1);
	assert_eq!(configuration.cli.len(), 1);
	assert_eq!(
		configuration
			.cli
			.first()
			.unwrap_or_else(|| panic!("expected CLI command"))
			.steps
			.len(),
		2
	);
	assert_eq!(configuration.defaults.empty_update_message, None);
	assert_eq!(configuration.npm.roots, vec!["packages/*"]);
	let first_package = configuration
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected first package"));
	assert_eq!(first_package.id, "core");
	assert_eq!(
		first_package.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("crates/core/changelog.md"),
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

[github.pull_requests]
enabled = true
branch_prefix = "automation/release"
base = "develop"
title = "chore(release): prepare release"
labels = ["release", "automated", "bot"]
auto_merge = true
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let source = configuration
		.source
		.unwrap_or_else(|| panic!("expected source config"));
	assert_eq!(source.provider, monochange_core::SourceProvider::GitHub);
	assert_eq!(source.owner, "ifiokjr");
	assert_eq!(source.repo, "monochange");
	assert!(source.releases.enabled);
	assert!(source.releases.draft);
	assert!(source.releases.prerelease);
	assert!(source.releases.generate_notes);
	assert_eq!(
		source.releases.source,
		monochange_core::GitHubReleaseNotesSource::GitHubGenerated
	);
	assert!(source.pull_requests.enabled);
	assert_eq!(source.pull_requests.branch_prefix, "automation/release");
	assert_eq!(source.pull_requests.base, "develop");
	assert_eq!(
		source.pull_requests.title,
		"chore(release): prepare release"
	);
	assert_eq!(
		source.pull_requests.labels,
		vec!["release", "automated", "bot"]
	);
	assert!(source.pull_requests.auto_merge);
}

#[test]
fn load_workspace_configuration_parses_github_changeset_bot_settings() {
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

[github.bot.changesets]
enabled = true
required = true
skip_labels = ["no-changeset-required", "internal"]
comment_on_failure = true
changed_paths = ["crates/**"]
ignored_paths = ["docs/**", "*.md"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let source = configuration
		.source
		.unwrap_or_else(|| panic!("expected source config"));

	assert_eq!(source.provider, monochange_core::SourceProvider::GitHub);
	assert!(source.bot.changesets.enabled);
	assert!(source.bot.changesets.required);
	assert!(source.bot.changesets.comment_on_failure);
	assert_eq!(
		source.bot.changesets.skip_labels,
		vec!["no-changeset-required", "internal"]
	);
	assert_eq!(source.bot.changesets.changed_paths, vec!["crates/**"]);
	assert_eq!(source.bot.changesets.ignored_paths, vec!["docs/**", "*.md"]);
}

#[test]
fn load_workspace_configuration_rejects_missing_package_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
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

	let rendered = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.render();
	assert!(rendered.contains("path `crates/core` does not exist"));
}

#[test]
fn load_workspace_configuration_rejects_duplicate_package_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/shared", "shared");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/shared"

[package.other]
path = "crates/shared"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let rendered = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.render();
	assert!(rendered.contains("already used by `core`"));
}

#[test]
fn load_workspace_configuration_rejects_missing_expected_manifests() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core"))
		.unwrap_or_else(|error| panic!("create package dir: {error}"));
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

	let rendered = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.render();
	assert!(rendered.contains("is missing expected cargo manifest"));
}

#[test]
fn load_workspace_configuration_rejects_empty_github_owner_and_repo() {
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
owner = ""
repo = "monochange"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	assert!(load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("[github].owner must not be empty"));

	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[github]
owner = "ifiokjr"
repo = ""
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	assert!(load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("[github].repo must not be empty"));
}

#[test]
fn load_workspace_configuration_rejects_invalid_pull_request_settings() {
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

[github.pull_requests]
branch_prefix = ""
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	assert!(load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("[github.pull_requests].branch_prefix must not be empty"));
}

#[test]
fn load_workspace_configuration_rejects_empty_pull_request_base_and_title() {
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

[github.pull_requests]
base = ""
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	assert!(load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("[github.pull_requests].base must not be empty"));

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

[github.pull_requests]
title = ""
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	assert!(load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("[github.pull_requests].title must not be empty"));
}

#[test]
fn load_workspace_configuration_rejects_invalid_github_release_note_source_combinations() {
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
source = "monochange"
generate_notes = true
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	assert!(load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("generate_notes cannot be true"));
}

#[test]
fn load_workspace_configuration_rejects_empty_pull_request_labels() {
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

[github.pull_requests]
labels = [""]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	assert!(load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("labels must not include empty values"));
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
changelog = "{{ path }}/changelog.md"

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
			"{{ path }}/changelog.md".to_string()
		))
	);
	assert_eq!(
		package.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("crates/core/changelog.md"),
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
			path: PathBuf::from("crates/core/CHANGELOG.md"),
			format: ChangelogFormat::Monochange,
		})
	);
	assert_eq!(app.changelog, None);
	assert_eq!(
		tool.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("docs/tool-release-notes.md"),
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
path = "{{ path }}/CHANGELOG.md"
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
			path: PathBuf::from("crates/core/CHANGELOG.md"),
			format: ChangelogFormat::KeepAChangelog,
		})
	);
	assert_eq!(
		app.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("docs/app-release-notes.md"),
			format: ChangelogFormat::Monochange,
		})
	);
	assert_eq!(
		group.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("docs/group-release-notes.md"),
			format: ChangelogFormat::KeepAChangelog,
		})
	);
}

#[test]
fn load_workspace_configuration_supports_empty_update_messages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	write_cargo_package(tempdir.path(), "crates/app", "app");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"
empty_update_message = "No package-specific changes for {{ package }}; version is now {{ version }}."

[package.core]
path = "crates/core"
empty_update_message = "Package override for {{ package }}@{{ version }}"

[package.app]
path = "crates/app"

[group.sdk]
packages = ["core", "app"]
empty_update_message = "Group fallback for {{ package }} from {{ group }}"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let core = configuration
		.package_by_id("core")
		.unwrap_or_else(|| panic!("expected core package"));
	let group = configuration
		.group_by_id("sdk")
		.unwrap_or_else(|| panic!("expected sdk group"));

	assert_eq!(
		configuration.defaults.empty_update_message.as_deref(),
		Some("No package-specific changes for {{ package }}; version is now {{ version }}.")
	);
	assert_eq!(
		core.empty_update_message.as_deref(),
		Some("Package override for {{ package }}@{{ version }}")
	);
	assert_eq!(
		group.empty_update_message.as_deref(),
		Some("Group fallback for {{ package }} from {{ group }}")
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
versioned_files = [{ path = "Cargo.lock", type = "cargo", name = "missing" }]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("unknown versioned file name `missing`"));
	assert!(rendered.contains(
		"reference a declared package id from `versioned_files` or remove the name entry"
	));
}

#[test]
fn load_workspace_configuration_infers_package_versioned_file_types_from_string_entries() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"
versioned_files = ["Cargo.toml", "**/crates/*/Cargo.toml"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected package"));

	assert_eq!(package.versioned_files.len(), 2);
	assert!(package
		.versioned_files
		.iter()
		.all(|definition| definition.ecosystem_type == EcosystemType::Cargo));
}

#[test]
fn load_workspace_configuration_rejects_group_string_versioned_files_without_explicit_type() {
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
versioned_files = ["group.toml"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("bare-string `versioned_files`"));
	assert!(rendered.contains("use `versioned_files = [{ path = \"...\", type = \"cargo\" }]`"));
}

#[test]
fn load_workspace_configuration_inherits_ecosystem_versioned_files_unless_package_opt_outs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_npm_package(tempdir.path(), "packages/app", "app");
	write_npm_package(tempdir.path(), "packages/web", "web");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "npm"

[ecosystems.npm]
versioned_files = ["**/package.json"]

[package.app]
path = "packages/app"

[package.web]
path = "packages/web"
ignore_ecosystem_versioned_files = true
versioned_files = ["package.json"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let app = configuration
		.packages
		.iter()
		.find(|package| package.id == "app")
		.unwrap_or_else(|| panic!("expected app package"));
	let web = configuration
		.packages
		.iter()
		.find(|package| package.id == "web")
		.unwrap_or_else(|| panic!("expected web package"));

	assert_eq!(app.versioned_files.len(), 1);
	assert_eq!(
		app.versioned_files
			.first()
			.map(|definition| definition.path.as_str()),
		Some("**/package.json")
	);
	assert!(web.ignore_ecosystem_versioned_files);
	assert_eq!(web.versioned_files.len(), 1);
	assert_eq!(
		web.versioned_files
			.first()
			.map(|definition| definition.path.as_str()),
		Some("package.json")
	);
}

#[test]
fn load_workspace_configuration_rejects_globs_that_match_unsupported_files_for_an_ecosystem() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	write_npm_package(tempdir.path(), "packages/web", "web");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"
versioned_files = [{ path = "**/*", type = "cargo" }]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("matched unsupported file"));
	assert!(rendered.contains("narrow the glob"));
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
	assert_eq!(signal.explicit_version, None);
	assert_eq!(signal.notes.as_deref(), Some("public API addition"));
	assert_eq!(signal.source_path, tempdir.path().join("change.md"));
	assert_eq!(
		signal.evidence_refs,
		vec!["rust-semver:major:public API break detected"]
	);
}

#[test]
fn load_change_signals_parses_explicit_versions_and_infers_bumps() {
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
core:
  version: 1.2.0
---

#### explicit version
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

	let changeset =
		load_changeset_file(&tempdir.path().join("change.md"), &configuration, &packages)
			.unwrap_or_else(|error| panic!("changeset file: {error}"));
	let target = changeset
		.targets
		.first()
		.unwrap_or_else(|| panic!("expected target"));
	let signal = changeset
		.signals
		.first()
		.unwrap_or_else(|| panic!("expected signal"));

	assert_eq!(target.bump, Some(BumpSeverity::Minor));
	assert_eq!(target.explicit_version, Some(Version::new(1, 2, 0)));
	assert_eq!(signal.requested_bump, Some(BumpSeverity::Minor));
	assert_eq!(signal.explicit_version, Some(Version::new(1, 2, 0)));
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
	assert_eq!(signal.source_path, tempdir.path().join("change.md"));
	assert_eq!(
		signal.details.as_deref(),
		Some("Roll the signing key before the release window closes.")
	);
}

#[test]
fn load_change_signals_rejects_markdown_without_frontmatter() {
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
		"#### missing frontmatter\n",
	)
	.unwrap_or_else(|error| panic!("changes write: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		tempdir.path().join("crates/core/Cargo.toml"),
		tempdir.path().to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&tempdir.path().join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(error.to_string().contains("missing markdown frontmatter"));
}

#[test]
fn load_change_signals_rejects_unterminated_markdown_frontmatter() {
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
	fs::write(tempdir.path().join("change.md"), "---\ncore: patch\n")
		.unwrap_or_else(|error| panic!("changes write: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		tempdir.path().join("crates/core/Cargo.toml"),
		tempdir.path().to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&tempdir.path().join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(error
		.to_string()
		.contains("unterminated markdown frontmatter"));
}

#[test]
fn load_change_signals_rejects_invalid_markdown_bumps() {
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
core: note
---

#### invalid bump
",
	)
	.unwrap_or_else(|error| panic!("changes write: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		tempdir.path().join("crates/core/Cargo.toml"),
		tempdir.path().to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&tempdir.path().join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(error
		.to_string()
		.contains("must map to `patch`, `minor`, or `major`"));
}

#[test]
fn load_change_signals_rejects_duplicate_package_entries() {
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
		tempdir.path().join("change.toml"),
		r#"
[[changes]]
package = "core"
bump = "patch"

[[changes]]
package = "core"
bump = "minor"
"#,
	)
	.unwrap_or_else(|error| panic!("changes write: {error}"));
	let mut packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		tempdir.path().join("crates/core/Cargo.toml"),
		tempdir.path().to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let rendered = load_change_signals(
		&tempdir.path().join("change.toml"),
		&configuration,
		&packages,
	)
	.err()
	.unwrap_or_else(|| panic!("expected duplicate entry error"))
	.render();
	assert!(rendered.contains("duplicate change entry"));
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
fn load_change_signals_rejects_invalid_explicit_versions() {
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
core:
  bump: minor
  version: nope
---

#### invalid version
",
	)
	.unwrap_or_else(|error| panic!("changes write: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		tempdir.path().join("crates/core/Cargo.toml"),
		tempdir.path().to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&tempdir.path().join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected invalid version error"));
	assert!(error.to_string().contains("invalid version `nope`"));
}

#[test]
fn load_changeset_file_preserves_group_targets_and_source_paths() {
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
origin:
  sdk: pull-request
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

	let changeset =
		load_changeset_file(&tempdir.path().join("change.md"), &configuration, &packages)
			.unwrap_or_else(|error| panic!("changeset file: {error}"));

	assert_eq!(changeset.path, tempdir.path().join("change.md"));
	assert_eq!(changeset.summary.as_deref(), Some("grouped release"));
	assert_eq!(changeset.targets.len(), 1);
	let target = changeset
		.targets
		.first()
		.unwrap_or_else(|| panic!("expected one changeset target"));
	assert_eq!(target.id, "sdk");
	assert_eq!(target.kind.as_str(), "group");
	assert_eq!(target.origin, "pull-request");
	assert_eq!(target.explicit_version, None);
	assert_eq!(changeset.signals.len(), 2);
	assert!(changeset
		.signals
		.iter()
		.all(|signal| signal.source_path == tempdir.path().join("change.md")));
}

#[test]
fn resolve_package_reference_rejects_ambiguous_package_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"shared",
			tempdir.path().join("crates/core/Cargo.toml"),
			tempdir.path().to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Npm,
			"shared",
			tempdir.path().join("packages/shared/package.json"),
			tempdir.path().to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let error = resolve_package_reference("shared", tempdir.path(), &packages)
		.err()
		.unwrap_or_else(|| panic!("expected ambiguous package error"));
	assert!(error.to_string().contains("matched multiple packages"));
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

[cli.publish]

[[cli.publish.steps]]
type = "PrepareRelease"

[[cli.publish.steps]]
type = "PublishRelease"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected github CLI command config error"));
	assert!(error
		.to_string()
		.contains("uses `PublishRelease` but `[source]` is not configured"));
}

#[test]
fn load_workspace_configuration_rejects_open_release_pull_request_without_github_config() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[cli.release-pr]

[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected github CLI command config error"));
	assert!(error
		.to_string()
		.contains("uses `OpenReleaseRequest` but `[source]` is not configured"));
}

#[test]
fn load_workspace_configuration_rejects_comment_released_issues_for_unsupported_provider() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[source]
provider = "gitlab"
owner = "group"
repo = "monochange"

[cli.comment]

[[cli.comment.steps]]
type = "PrepareRelease"

[[cli.comment.steps]]
type = "CommentReleasedIssues"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected provider capability error"));
	assert!(error.to_string().contains(
		"uses `CommentReleasedIssues` but `[source].provider = \"gitlab\"` does not support released-issue comments"
	));

	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[source]
provider = "gitea"
owner = "org"
repo = "monochange"
host = "https://codeberg.org"

[cli.comment]

[[cli.comment.steps]]
type = "PrepareRelease"

[[cli.comment.steps]]
type = "CommentReleasedIssues"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected provider capability error"));
	assert!(error.to_string().contains(
		"uses `CommentReleasedIssues` but `[source].provider = \"gitea\"` does not support released-issue comments"
	));
}

#[test]
fn load_workspace_configuration_accepts_comment_released_issues_for_github() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[cli.comment]

[[cli.comment.steps]]
type = "PrepareRelease"

[[cli.comment.steps]]
type = "CommentReleasedIssues"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.cli.len(), 1);
}

#[test]
fn load_workspace_configuration_rejects_enforce_changeset_policy_without_github_bot_config() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[changesets.verify]
enabled = false

[cli.affected]

[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.affected.steps]]
type = "AffectedPackages"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected verification CLI command config error"));
	assert!(error
		.to_string()
		.contains("uses `AffectedPackages` but `[changesets.verify].enabled` is false"));
}

#[test]
fn load_workspace_configuration_rejects_affected_packages_without_path_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[changesets.verify]
enabled = true

[cli.affected]

[[cli.affected.steps]]
type = "AffectedPackages"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected verification CLI command config error"));
	assert!(error
		.to_string()
		.contains("declares neither a `changed_paths` nor a `since` input"));
}

#[test]
fn load_workspace_configuration_accepts_affected_packages_step_input_overrides() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[changesets.verify]
enabled = true

[cli.pr-check]
help_text = "Verify attached changesets"

[[cli.pr-check.steps]]
type = "AffectedPackages"
inputs = { changed_paths = ["crates/core/src/lib.rs"], verify = true }
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.cli.len(), 1);
}

#[test]
fn load_workspace_configuration_accepts_affected_packages_with_since_in_step_override() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[changesets.verify]
enabled = true

[cli.pr-check]

[[cli.pr-check.steps]]
type = "AffectedPackages"
inputs = { since = "origin/main" }
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.cli.len(), 1);
}

#[test]
fn load_workspace_configuration_rejects_affected_packages_when_step_override_provides_no_path_source(
) {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[changesets.verify]
enabled = true

[cli.pr-check]

[[cli.pr-check.steps]]
type = "AffectedPackages"
inputs = { verify = true }
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(error
		.to_string()
		.contains("declares neither a `changed_paths` nor a `since` input"));
}

#[test]
fn load_workspace_configuration_rejects_step_override_with_boolean_for_non_boolean_input() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[changesets.verify]
enabled = true

[cli.pr-check]

[[cli.pr-check.inputs]]
name = "changed_paths"
type = "string_list"

[[cli.pr-check.steps]]
type = "AffectedPackages"
inputs = { changed_paths = true }
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("override `changed_paths` must use a"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_step_override_with_list_for_boolean_input() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[changesets.verify]
enabled = true

[cli.pr-check]

[[cli.pr-check.inputs]]
name = "changed_paths"
type = "string_list"

[[cli.pr-check.steps]]
type = "AffectedPackages"
inputs = { changed_paths = ["crates/core/src/lib.rs"], verify = ["true"] }
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error.to_string().contains("override `verify` must use a"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_unknown_step_input_override() {
	let root = fixture_path("validate-step-inputs/unknown-input-on-discover");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("unknown input override `nonexistent`"),
		"error was: {error}"
	);
	assert!(
		error.to_string().contains("valid inputs: format"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_unknown_input_on_validate_step() {
	let root = fixture_path("validate-step-inputs/unknown-input-on-validate");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("unknown input override `format`"),
		"error was: {error}"
	);
	assert!(
		error.to_string().contains("this step accepts no inputs"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_allows_any_input_on_command_step() {
	let root = fixture_path("validate-step-inputs/any-input-on-command");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let build_cmd = configuration
		.cli
		.iter()
		.find(|c| c.name == "build")
		.unwrap_or_else(|| panic!("expected build command"));
	assert_eq!(build_cmd.steps.len(), 1);
}

#[test]
fn load_workspace_configuration_rejects_wrong_type_format_override_on_discover() {
	let root = fixture_path("validate-step-inputs/wrong-type-format-on-discover");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("override `format` must use a string value"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_list_for_string_input_on_change_step() {
	let root = fixture_path("validate-step-inputs/list-for-string-on-change");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("override `reason` must use a string value"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_accepts_valid_diagnose_changesets_step_inputs() {
	let root = fixture_path("validate-step-inputs/valid-diagnose-inputs");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let diag_cmd = configuration
		.cli
		.iter()
		.find(|c| c.name == "diag")
		.unwrap_or_else(|| panic!("expected diag command"));
	assert_eq!(diag_cmd.steps.len(), 1);
}

#[test]
fn load_workspace_configuration_rejects_unknown_input_on_diagnose_changesets() {
	let root = fixture_path("validate-step-inputs/unknown-input-on-diagnose");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("unknown input override `verbose`"),
		"error was: {error}"
	);
	assert!(
		error
			.to_string()
			.contains("valid inputs: format, changeset"),
		"error was: {error}"
	);
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
change_templates = [
    "#### {{ summary }} ({{ package }} {{ bump }})\n\n{{ details }}\n\n{{ context }}",
    "#### {{ summary }}\n\n_Owner:_ {{ change_owner_link }}\n_PR:_ {{ review_request_link }}\n_Closed issues:_ {{ closed_issue_links }}",
    "- {{ summary }}"
]

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

	assert_eq!(configuration.release_notes.change_templates.len(), 3);
	assert_eq!(package.extra_changelog_sections.len(), 1);
	let extra_section = package
		.extra_changelog_sections
		.first()
		.unwrap_or_else(|| panic!("expected extra changelog section"));
	assert_eq!(extra_section.name, "Security");
	assert_eq!(extra_section.types, vec!["security"]);
}

#[test]
fn load_workspace_configuration_rejects_empty_extra_changelog_section_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"
extra_changelog_sections = [{ name = "", types = ["security"] }]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let rendered = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.render();
	assert!(rendered.contains("empty `name`"));
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
fn load_workspace_configuration_rejects_empty_extra_changelog_section_type_values() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"
extra_changelog_sections = [{ name = "Security", types = [""] }]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let rendered = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.render();
	assert!(rendered.contains("must not include empty types"));
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
change_templates = ["- {{ summary }} ({{ commit_hash }})"]

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
fn load_workspace_configuration_rejects_reserved_cli_command_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[cli.init]

[[cli.init.steps]]
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
fn load_workspace_configuration_rejects_duplicate_cli_command_tables() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[cli.release]

[[cli.release.steps]]
type = "PrepareRelease"

[cli.release]

[[cli.release.steps]]
type = "Command"
command = "cargo check"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(error.to_string().contains("failed to parse"));
}

#[test]
fn load_workspace_configuration_rejects_legacy_workflows_namespace() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(error
		.to_string()
		.contains("legacy `[[workflows]]` configuration is no longer supported"));
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

#[test]
fn load_workspace_configuration_rejects_both_source_and_legacy_github_config() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[source]
provider = "github"
owner = "example"
repo = "repo"

[github]
owner = "example"
repo = "repo"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(error
		.to_string()
		.contains("configure either `[source]` or legacy `[github]`"));
}

#[test]
fn load_change_signals_infers_group_bump_from_member_explicit_version() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	write_cargo_package(tempdir.path(), "crates/app", "app");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[package.app]
path = "crates/app"

[group.sdk]
packages = ["core", "app"]
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));

	fs::write(
		tempdir.path().join("change.md"),
		r"---
sdk:
  version: 2.0.0
---

#### major bump via explicit version
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
			Ecosystem::Cargo,
			"app",
			tempdir.path().join("crates/app/Cargo.toml"),
			tempdir.path().to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let changeset =
		load_changeset_file(&tempdir.path().join("change.md"), &configuration, &packages)
			.unwrap_or_else(|error| panic!("changeset file: {error}"));

	let target = changeset
		.targets
		.first()
		.unwrap_or_else(|| panic!("expected target"));
	assert_eq!(target.bump, Some(BumpSeverity::Major));
	assert_eq!(target.explicit_version, Some(Version::new(2, 0, 0)));
}

#[test]
fn load_workspace_configuration_accepts_detailed_and_enabled_true_changelog_in_defaults() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"
changelog = true

[package.core]
path = "crates/core"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert!(!configuration.packages.is_empty());
}

#[test]
fn load_workspace_configuration_accepts_detailed_changelog_disabled_in_defaults() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"
changelog = { enabled = false }

[package.core]
path = "crates/core"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert!(!configuration.packages.is_empty());
}

#[test]
fn load_workspace_configuration_accepts_detailed_changelog_enabled_with_no_path_in_defaults() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_cargo_package(tempdir.path(), "crates/core", "core");
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"
changelog = { enabled = true }

[package.core]
path = "crates/core"
"#,
	)
	.unwrap_or_else(|error| panic!("config write: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert!(!configuration.packages.is_empty());
}

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn write_cargo_package(root: &Path, relative_dir: &str, name: &str) {
	let dir = root.join(relative_dir);
	fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create cargo dir: {error}"));
	fs::write(
		dir.join("Cargo.toml"),
		format!("[package]\nname = \"{name}\"\nversion = \"1.0.0\"\n"),
	)
	.unwrap_or_else(|error| panic!("cargo manifest: {error}"));
}

fn write_npm_package(root: &Path, relative_dir: &str, name: &str) {
	let dir = root.join(relative_dir);
	fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create npm dir: {error}"));
	fs::write(
		dir.join("package.json"),
		format!("{{\"name\":\"{name}\",\"version\":\"1.0.0\"}}"),
	)
	.unwrap_or_else(|error| panic!("package.json: {error}"));
}
