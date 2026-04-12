#![allow(unused_assignments)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_config`
//!
//! <!-- {=monochangeConfigCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_config` parses and validates the inputs that drive planning and release commands.
//!
//! Reach for this crate when you need to load `monochange.toml`, resolve package references, or turn `.changeset/*.md` files into validated change signals for the planner.
//!
//! ## Why use it?
//!
//! - centralize config parsing and validation rules in one place
//! - resolve package references against discovered workspace packages
//! - keep CLI command definitions, version groups, and change files aligned with the planner's expectations
//!
//! ## Best for
//!
//! - validating configuration before handing it to planning code
//! - parsing and resolving change files in custom automation
//! - keeping package-reference rules consistent across tools
//!
//! ## Public entry points
//!
//! - `load_workspace_configuration(root)` loads and validates `monochange.toml`
//! - `load_change_signals(root, changes_dir, packages)` parses markdown change files into change signals
//! - `resolve_package_reference(reference, workspace_root, packages)` maps package names, ids, and paths to discovered packages
//! - `apply_version_groups(packages, configuration)` attaches configured version groups to discovered packages
//!
//! ## Responsibilities
//!
//! - load `monochange.toml`
//! - validate version groups and CLI commands
//! - resolve package references against discovered packages
//! - parse change-input files, evidence, release-note `type` / `details` fields, changelog paths, changelog format overrides, source-provider config, changeset-bot policy config, and command release/manifest/policy steps
//!
//! ## Example
//!
//! ```rust
//! use monochange_config::load_workspace_configuration;
//! use monochange_core::ChangelogFormat;
//!
//! let root = std::env::temp_dir().join("monochange-config-changelog-format-docs");
//! let _ = std::fs::remove_dir_all(&root);
//! std::fs::create_dir_all(root.join("crates/core")).unwrap();
//! std::fs::write(
//!     root.join("crates/core/Cargo.toml"),
//!     "[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
//! )
//! .unwrap();
//! std::fs::write(
//!     root.join("monochange.toml"),
//!     r#"
//! [defaults]
//! package_type = "cargo"
//!
//! [defaults.changelog]
//! path = "{{ path }}/CHANGELOG.md"
//! format = "keep_a_changelog"
//!
//! [package.core]
//! path = "crates/core"
//! "#,
//! )
//! .unwrap();
//!
//! let configuration = load_workspace_configuration(&root).unwrap();
//! let package = configuration.package_by_id("core").unwrap();
//!
//! assert_eq!(configuration.defaults.changelog_format, ChangelogFormat::KeepAChangelog);
//! assert_eq!(package.changelog.as_ref().unwrap().format, ChangelogFormat::KeepAChangelog);
//! assert_eq!(package.changelog.as_ref().unwrap().path, std::path::PathBuf::from("crates/core/CHANGELOG.md"));
//!
//! let _ = std::fs::remove_dir_all(&root);
//! ```
//! <!-- {/monochangeConfigCrateDocs} -->

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use glob::Pattern;
use miette::Diagnostic;
use miette::LabeledSpan;
use miette::NamedSource;
use miette::Report;
use miette::SourceSpan;
use minijinja::Environment;
use minijinja::UndefinedBehavior;
use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::ChangelogDefinition;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogTarget;
use monochange_core::ChangesetSettings;
use monochange_core::ChangesetTargetKind;
use monochange_core::ChangesetVerificationSettings;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputDefinition;
use monochange_core::CliInputKind;
use monochange_core::CliStepDefinition;
use monochange_core::CliStepInputValue;
use monochange_core::Ecosystem;
use monochange_core::EcosystemSettings;
use monochange_core::EcosystemType;
use monochange_core::ExtraChangelogSection;
use monochange_core::GitHubConfiguration;
use monochange_core::GroupChangelogInclude;
use monochange_core::GroupDefinition;
use monochange_core::LockfileCommandDefinition;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageDefinition;
use monochange_core::PackageRecord;
use monochange_core::PackageType;
use monochange_core::ProviderBotSettings;
use monochange_core::ProviderChangesetBotSettings;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseNotesSource;
use monochange_core::ProviderReleaseSettings;
use monochange_core::ReleaseNotesSettings;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::VersionFormat;
use monochange_core::VersionGroup;
use monochange_core::VersionedFileDefinition;
use monochange_core::WorkspaceConfiguration;
use monochange_core::WorkspaceDefaults;
use monochange_core::default_cli_commands;
use monochange_core::relative_to_root;
use regex::Regex;
use semver::Version;
use serde::Deserialize;
use serde_yaml_ng::Mapping;

const CONFIG_FILE: &str = "monochange.toml";
const RESERVED_CLI_COMMAND_NAMES: &[&str] = &["assist", "help", "init", "mcp", "version"];
const SUPPORTED_CHANGE_TEMPLATE_VARIABLES: &[&str] = &[
	"summary",
	"details",
	"package",
	"version",
	"target_id",
	"bump",
	"type",
	"context",
	"context",
	"changeset_path",
	"change_owner",
	"change_owner_link",
	"review_request",
	"review_request_link",
	"introduced_commit",
	"introduced_commit_link",
	"last_updated_commit",
	"last_updated_commit_link",
	"related_issues",
	"related_issue_links",
	"closed_issues",
	"closed_issue_links",
];

#[derive(Debug, Deserialize, Default)]
struct RawWorkspaceConfiguration {
	#[serde(default)]
	defaults: RawWorkspaceDefaults,
	#[serde(default)]
	release_notes: RawReleaseNotesSettings,
	#[serde(default)]
	package: BTreeMap<String, RawPackageDefinition>,
	#[serde(default)]
	group: BTreeMap<String, RawGroupDefinition>,
	#[serde(default)]
	cli: BTreeMap<String, RawCliCommandDefinition>,
	#[serde(default)]
	workflows: Vec<CliCommandDefinition>,
	#[serde(default)]
	changesets: RawChangesetSettings,
	#[serde(default)]
	source: Option<RawSourceConfiguration>,
	#[serde(default)]
	github: Option<RawGitHubConfiguration>,
	#[serde(default)]
	ecosystems: RawEcosystems,
}

#[derive(Debug, Deserialize)]
struct RawWorkspaceDefaults {
	#[serde(default = "default_parent_bump")]
	parent_bump: BumpSeverity,
	#[serde(default)]
	include_private: bool,
	#[serde(default = "default_warn_on_group_mismatch")]
	warn_on_group_mismatch: bool,
	#[serde(default)]
	strict_version_conflicts: bool,
	#[serde(default)]
	package_type: Option<PackageType>,
	#[serde(default)]
	changelog: Option<RawChangelogConfig>,
	#[serde(default)]
	extra_changelog_sections: Vec<ExtraChangelogSection>,
	#[serde(default)]
	empty_update_message: Option<String>,
	#[serde(default)]
	release_title: Option<String>,
	#[serde(default)]
	changelog_version_title: Option<String>,
}

impl Default for RawWorkspaceDefaults {
	fn default() -> Self {
		Self {
			parent_bump: default_parent_bump(),
			include_private: false,
			warn_on_group_mismatch: default_warn_on_group_mismatch(),
			strict_version_conflicts: false,
			package_type: None,
			changelog: None,
			extra_changelog_sections: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawChangelogDefinition {
	Enabled(bool),
	Path(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawChangelogConfig {
	Legacy(RawChangelogDefinition),
	Detailed(RawChangelogTable),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawGroupChangelogInclude {
	Mode(String),
	Packages(Vec<String>),
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawChangelogTable {
	#[serde(default)]
	enabled: Option<bool>,
	#[serde(default)]
	path: Option<String>,
	#[serde(default)]
	format: Option<ChangelogFormat>,
	#[serde(default)]
	include: Option<RawGroupChangelogInclude>,
}

#[derive(Debug, Deserialize)]
struct RawPackageDefinition {
	path: PathBuf,
	#[serde(rename = "type")]
	package_type: Option<PackageType>,
	#[serde(default)]
	changelog: Option<RawChangelogConfig>,
	#[serde(default)]
	extra_changelog_sections: Vec<ExtraChangelogSection>,
	#[serde(default)]
	empty_update_message: Option<String>,
	#[serde(default)]
	release_title: Option<String>,
	#[serde(default)]
	changelog_version_title: Option<String>,
	#[serde(default)]
	versioned_files: Vec<RawVersionedFileDefinition>,
	#[serde(default)]
	ignore_ecosystem_versioned_files: bool,
	#[serde(default)]
	ignored_paths: Vec<String>,
	#[serde(default)]
	additional_paths: Vec<String>,
	#[serde(default)]
	tag: bool,
	#[serde(default)]
	release: bool,
	#[serde(default)]
	version_format: VersionFormat,
}

#[derive(Debug, Deserialize)]
struct RawGroupDefinition {
	packages: Vec<String>,
	#[serde(default)]
	changelog: Option<RawChangelogConfig>,
	#[serde(default)]
	extra_changelog_sections: Vec<ExtraChangelogSection>,
	#[serde(default)]
	empty_update_message: Option<String>,
	#[serde(default)]
	release_title: Option<String>,
	#[serde(default)]
	changelog_version_title: Option<String>,
	#[serde(default)]
	versioned_files: Vec<RawVersionedFileDefinition>,
	#[serde(default)]
	tag: bool,
	#[serde(default)]
	release: bool,
	#[serde(default)]
	version_format: VersionFormat,
}

#[derive(Debug, Deserialize, Default)]
struct RawCliCommandDefinition {
	#[serde(default)]
	help_text: Option<String>,
	#[serde(default)]
	inputs: Vec<CliInputDefinition>,
	#[serde(default)]
	steps: Vec<CliStepDefinition>,
}

#[derive(Debug, Deserialize, Default)]
struct RawEcosystems {
	#[serde(default)]
	cargo: RawEcosystemSettings,
	#[serde(default)]
	npm: RawEcosystemSettings,
	#[serde(default)]
	deno: RawEcosystemSettings,
	#[serde(default)]
	dart: RawEcosystemSettings,
}

#[derive(Debug, Deserialize, Default)]
struct RawEcosystemSettings {
	#[serde(default)]
	enabled: Option<bool>,
	#[serde(default)]
	roots: Vec<String>,
	#[serde(default)]
	exclude: Vec<String>,
	#[serde(default)]
	dependency_version_prefix: Option<String>,
	#[serde(default)]
	versioned_files: Vec<RawVersionedFileDefinition>,
	#[serde(default)]
	lockfile_commands: Vec<LockfileCommandDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawVersionedFileDefinition {
	Path(String),
	Detailed(VersionedFileDefinition),
}

#[derive(Debug, Deserialize, Default)]
struct RawReleaseNotesSettings {
	#[serde(default)]
	change_templates: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawChangesetSettings {
	#[serde(default)]
	verify: RawChangesetVerificationSettings,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Deserialize)]
struct RawChangesetVerificationSettings {
	#[serde(default = "default_true")]
	enabled: bool,
	#[serde(default = "default_true")]
	required: bool,
	#[serde(default)]
	skip_labels: Vec<String>,
	#[serde(default = "default_true")]
	comment_on_failure: bool,
}

impl Default for RawChangesetVerificationSettings {
	fn default() -> Self {
		Self {
			enabled: default_true(),
			required: default_true(),
			skip_labels: Vec::new(),
			comment_on_failure: default_true(),
		}
	}
}

#[derive(Debug, Deserialize)]
struct RawSourceConfiguration {
	#[serde(default)]
	provider: SourceProvider,
	owner: String,
	repo: String,
	#[serde(default)]
	host: Option<String>,
	#[serde(default)]
	api_url: Option<String>,
	#[serde(default)]
	releases: RawProviderReleaseSettings,
	#[serde(default)]
	pull_requests: RawProviderMergeRequestSettings,
	#[serde(default)]
	bot: RawProviderBotSettings,
}

#[derive(Debug, Deserialize)]
struct RawGitHubConfiguration {
	owner: String,
	repo: String,
	#[serde(default)]
	releases: RawProviderReleaseSettings,
	#[serde(default)]
	pull_requests: RawProviderMergeRequestSettings,
	#[serde(default)]
	bot: RawProviderBotSettings,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Deserialize)]
struct RawProviderReleaseSettings {
	#[serde(default = "default_true")]
	enabled: bool,
	#[serde(default)]
	draft: bool,
	#[serde(default)]
	prerelease: bool,
	#[serde(default)]
	generate_notes: bool,
	#[serde(default)]
	source: ProviderReleaseNotesSource,
}

impl Default for RawProviderReleaseSettings {
	fn default() -> Self {
		Self {
			enabled: default_true(),
			draft: false,
			prerelease: false,
			generate_notes: false,
			source: ProviderReleaseNotesSource::Monochange,
		}
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Deserialize)]
struct RawProviderMergeRequestSettings {
	#[serde(default = "default_true")]
	enabled: bool,
	#[serde(default = "default_pull_request_branch_prefix")]
	branch_prefix: String,
	#[serde(default = "default_pull_request_base")]
	base: String,
	#[serde(default = "default_pull_request_title")]
	title: String,
	#[serde(default = "default_pull_request_labels")]
	labels: Vec<String>,
	#[serde(default)]
	auto_merge: bool,
}

impl Default for RawProviderMergeRequestSettings {
	fn default() -> Self {
		Self {
			enabled: default_true(),
			branch_prefix: default_pull_request_branch_prefix(),
			base: default_pull_request_base(),
			title: default_pull_request_title(),
			labels: default_pull_request_labels(),
			auto_merge: false,
		}
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Deserialize)]
struct RawProviderChangesetBotSettings {
	#[serde(default)]
	enabled: bool,
	#[serde(default = "default_true")]
	required: bool,
	#[serde(default)]
	skip_labels: Vec<String>,
	#[serde(default = "default_true")]
	comment_on_failure: bool,
	#[serde(default)]
	changed_paths: Vec<String>,
	#[serde(default)]
	ignored_paths: Vec<String>,
}

impl Default for RawProviderChangesetBotSettings {
	fn default() -> Self {
		Self {
			enabled: false,
			required: default_true(),
			skip_labels: Vec::new(),
			comment_on_failure: default_true(),
			changed_paths: Vec::new(),
			ignored_paths: Vec::new(),
		}
	}
}

#[derive(Debug, Deserialize, Default)]
struct RawProviderBotSettings {
	#[serde(default)]
	changesets: RawProviderChangesetBotSettings,
}

#[derive(Debug, Deserialize, Default)]
struct RawChangeFile {
	#[serde(default)]
	changes: Vec<RawChangeEntry>,
}

#[derive(Debug, Deserialize)]
struct RawChangeEntry {
	package: String,
	#[serde(default)]
	bump: Option<BumpSeverity>,
	#[serde(default)]
	version: Option<Version>,
	#[serde(default)]
	reason: Option<String>,
	#[serde(default)]
	details: Option<String>,
	#[serde(rename = "type", default)]
	change_type: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LoadedChangesetTarget {
	pub id: String,
	pub kind: ChangesetTargetKind,
	pub bump: Option<BumpSeverity>,
	pub explicit_version: Option<Version>,
	pub origin: String,
	pub evidence_refs: Vec<String>,
	pub change_type: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LoadedChangesetFile {
	pub path: PathBuf,
	pub summary: Option<String>,
	pub details: Option<String>,
	pub targets: Vec<LoadedChangesetTarget>,
	pub signals: Vec<ChangeSignal>,
}

#[allow(unused_assignments)]
#[derive(Debug, thiserror::Error, Diagnostic)]
#[error("{message}")]
struct SourceDiagnostic {
	message: String,
	#[source_code]
	source_code: NamedSource<String>,
	#[label(collection)]
	labels: Vec<LabeledSpan>,
	#[help]
	help: Option<String>,
}

fn default_parent_bump() -> BumpSeverity {
	BumpSeverity::Patch
}

fn default_warn_on_group_mismatch() -> bool {
	true
}

fn merge_cli_commands(cli: BTreeMap<String, RawCliCommandDefinition>) -> Vec<CliCommandDefinition> {
	let mut merged = default_cli_commands();
	for (name, definition) in cli {
		let command = CliCommandDefinition {
			name: name.clone(),
			help_text: definition.help_text,
			inputs: definition.inputs,
			steps: definition.steps,
		};
		if let Some(existing) = merged
			.iter_mut()
			.find(|cli_command| cli_command.name == name)
		{
			*existing = command;
		} else {
			merged.push(command);
		}
	}
	merged
}

fn default_true() -> bool {
	true
}

fn default_pull_request_branch_prefix() -> String {
	"monochange/release".to_string()
}

fn default_pull_request_base() -> String {
	"main".to_string()
}

fn default_pull_request_title() -> String {
	"chore(release): prepare release".to_string()
}

fn default_pull_request_labels() -> Vec<String> {
	vec!["release".to_string(), "automated".to_string()]
}

fn render_changelog_path_template(template: &str, package_path: &Path) -> String {
	let package_path_str = package_path.to_string_lossy();
	let mut env = Environment::new();
	env.set_undefined_behavior(UndefinedBehavior::Lenient);
	let context = minijinja::context! { path => package_path_str.as_ref() };
	env.render_str(template, context)
		.unwrap_or_else(|_| template.replace("{{ path }}", &package_path_str))
}

impl RawChangelogConfig {
	fn as_defaults_definition(&self) -> ChangelogDefinition {
		match self {
			Self::Legacy(definition) => {
				match definition {
					RawChangelogDefinition::Enabled(false) => ChangelogDefinition::Disabled,
					RawChangelogDefinition::Enabled(true) => ChangelogDefinition::PackageDefault,
					RawChangelogDefinition::Path(path_pattern) => {
						ChangelogDefinition::PathPattern(path_pattern.clone())
					}
				}
			}
			Self::Detailed(table) => {
				match (table.enabled.unwrap_or(true), &table.path) {
					(false, _) => ChangelogDefinition::Disabled,
					(true, Some(path_pattern)) => {
						ChangelogDefinition::PathPattern(path_pattern.clone())
					}
					(true, None) => ChangelogDefinition::PackageDefault,
				}
			}
		}
	}

	fn format(&self) -> Option<ChangelogFormat> {
		match self {
			Self::Legacy(_) => None,
			Self::Detailed(table) => table.format,
		}
	}

	fn include(&self) -> Option<&RawGroupChangelogInclude> {
		match self {
			Self::Legacy(_) => None,
			Self::Detailed(table) => table.include.as_ref(),
		}
	}

	fn is_disabled(&self) -> bool {
		match self {
			Self::Legacy(definition) => {
				matches!(definition, RawChangelogDefinition::Enabled(false))
			}
			Self::Detailed(table) => matches!(table.enabled, Some(false)),
		}
	}

	fn resolve_for_package(
		&self,
		package_path: &Path,
		treat_string_as_pattern: bool,
	) -> Option<PathBuf> {
		match self {
			Self::Legacy(definition) => {
				match definition {
					RawChangelogDefinition::Enabled(false) => None,
					RawChangelogDefinition::Enabled(true) => {
						Some(package_path.join("CHANGELOG.md"))
					}
					RawChangelogDefinition::Path(path) => {
						if treat_string_as_pattern {
							Some(PathBuf::from(render_changelog_path_template(
								path,
								package_path,
							)))
						} else {
							Some(PathBuf::from(path))
						}
					}
				}
			}
			Self::Detailed(table) => {
				if matches!(table.enabled, Some(false)) {
					return None;
				}
				match &table.path {
					Some(path) => {
						if treat_string_as_pattern {
							Some(PathBuf::from(render_changelog_path_template(
								path,
								package_path,
							)))
						} else {
							Some(PathBuf::from(path))
						}
					}
					None => Some(package_path.join("CHANGELOG.md")),
				}
			}
		}
	}

	fn resolve_for_group(&self) -> Option<PathBuf> {
		match self {
			Self::Legacy(definition) => {
				match definition {
					RawChangelogDefinition::Enabled(false | true) => None,
					RawChangelogDefinition::Path(path) => Some(PathBuf::from(path)),
				}
			}
			Self::Detailed(table) => {
				if matches!(table.enabled, Some(false)) {
					return None;
				}
				table.path.as_ref().map(PathBuf::from)
			}
		}
	}
}

fn parse_group_changelog_include(
	config_contents: &str,
	group_id: &str,
	group_packages: &[String],
	include: Option<&RawGroupChangelogInclude>,
) -> MonochangeResult<GroupChangelogInclude> {
	let Some(include) = include else {
		return Ok(GroupChangelogInclude::All);
	};
	match include {
		RawGroupChangelogInclude::Mode(mode) => match mode.as_str() {
			"all" => Ok(GroupChangelogInclude::All),
			"group-only" => Ok(GroupChangelogInclude::GroupOnly),
			_ => Err(config_diagnostic(
				config_contents,
				format!(
					"group `{group_id}` changelog include must be `\"all\"`, `\"group-only\"`, or an array of member package ids"
				),
				vec![config_field_label(
					config_contents,
					"group",
					&format!("{group_id}.changelog"),
					"include",
					"group changelog include",
				)],
				Some(
					"use `include = \"all\"`, `include = \"group-only\"`, or `include = [\"member-id\"]`"
						.to_string(),
				),
			)),
		},
		RawGroupChangelogInclude::Packages(package_ids) => {
			let mut selected = BTreeSet::new();
			for package_id in package_ids {
				if package_id.trim().is_empty() {
					return Err(config_diagnostic(
						config_contents,
						format!(
							"group `{group_id}` changelog include entries must not be empty"
						),
						vec![config_field_label(
							config_contents,
							"group",
							&format!("{group_id}.changelog"),
							"include",
							"group changelog include member",
						)],
						Some(
							"remove the empty value or replace it with a package id declared in the group"
								.to_string(),
						),
					));
				}
				if !group_packages.iter().any(|member| member == package_id) {
					return Err(config_diagnostic(
						config_contents,
						format!(
							"group `{group_id}` changelog include entry `{package_id}` must reference a package declared in that group"
						),
						vec![config_field_label(
							config_contents,
							"group",
							&format!("{group_id}.changelog"),
							"include",
							"group changelog include member",
						)],
						Some(
							"list only package ids from `group.<id>.packages` in `group.<id>.changelog.include`"
								.to_string(),
						),
					));
				}
				selected.insert(package_id.clone());
			}
			if selected.is_empty() {
				Ok(GroupChangelogInclude::GroupOnly)
			} else {
				Ok(GroupChangelogInclude::Selected(selected))
			}
		}
	}
}

#[must_use]
pub fn config_path(root: &Path) -> PathBuf {
	root.join(CONFIG_FILE)
}

fn package_type_to_ecosystem_type(package_type: PackageType) -> EcosystemType {
	match package_type {
		PackageType::Cargo => EcosystemType::Cargo,
		PackageType::Npm => EcosystemType::Npm,
		PackageType::Deno => EcosystemType::Deno,
		PackageType::Dart | PackageType::Flutter => EcosystemType::Dart,
	}
}

fn normalize_versioned_files(
	contents: &str,
	versioned_files: Vec<RawVersionedFileDefinition>,
	inferred_ecosystem_type: EcosystemType,
	owner_kind: &str,
	owner_id: &str,
	allow_shorthand: bool,
) -> MonochangeResult<Vec<VersionedFileDefinition>> {
	versioned_files
		.into_iter()
		.map(|versioned_file| match versioned_file {
			RawVersionedFileDefinition::Detailed(definition) => Ok(definition),
			RawVersionedFileDefinition::Path(path) if allow_shorthand => {
				Ok(VersionedFileDefinition {
					path,
					ecosystem_type: Some(inferred_ecosystem_type),
					prefix: None,
					fields: None,
					name: None,
					regex: None,
				})
			}
			RawVersionedFileDefinition::Path(_) => Err(config_diagnostic(
				contents,
				format!(
					"{owner_kind} `{owner_id}` uses bare-string `versioned_files`, but the ecosystem cannot be inferred here"
				),
				vec![config_section_label(
					contents,
					owner_kind,
					owner_id,
					"bare-string versioned_files not allowed here",
				)],
				Some(
					"use `versioned_files = [{ path = \"...\", type = \"cargo\" }]` (or another explicit ecosystem type) for groups"
						.to_string(),
				),
			)),
		})
		.collect()
}

fn normalize_ecosystem_settings(
	contents: &str,
	owner_id: &str,
	inferred_ecosystem_type: EcosystemType,
	raw: RawEcosystemSettings,
) -> MonochangeResult<EcosystemSettings> {
	Ok(EcosystemSettings {
		enabled: raw.enabled,
		roots: raw.roots,
		exclude: raw.exclude,
		dependency_version_prefix: raw.dependency_version_prefix,
		versioned_files: normalize_versioned_files(
			contents,
			raw.versioned_files,
			inferred_ecosystem_type,
			"ecosystems",
			owner_id,
			true,
		)?,
		lockfile_commands: raw.lockfile_commands,
	})
}

#[tracing::instrument(skip_all)]
pub fn load_workspace_configuration(root: &Path) -> MonochangeResult<WorkspaceConfiguration> {
	let path = config_path(root);
	let contents = if path.exists() {
		fs::read_to_string(&path).map_err(|error| {
			MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
		})?
	} else {
		String::new()
	};
	let raw = if path.exists() {
		toml::from_str::<RawWorkspaceConfiguration>(&contents).map_err(|error| {
			MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
		})?
	} else {
		RawWorkspaceConfiguration::default()
	};

	let RawWorkspaceConfiguration {
		defaults,
		release_notes,
		package,
		group,
		cli,
		workflows,
		changesets,
		source,
		github,
		ecosystems,
	} = raw;
	if !workflows.is_empty() {
		return Err(MonochangeError::Config(
			"legacy `[[workflows]]` configuration is no longer supported; use `[cli.<command>]` with `[[cli.<command>.steps]]` instead".to_string(),
		));
	}
	let cli = merge_cli_commands(cli);
	let default_package_type = defaults.package_type;
	let default_package_changelog = defaults.changelog.clone();
	let default_extra_changelog_sections = defaults.extra_changelog_sections.clone();
	let cargo_ecosystem =
		normalize_ecosystem_settings(&contents, "cargo", EcosystemType::Cargo, ecosystems.cargo)?;
	let npm_ecosystem =
		normalize_ecosystem_settings(&contents, "npm", EcosystemType::Npm, ecosystems.npm)?;
	let deno_ecosystem =
		normalize_ecosystem_settings(&contents, "deno", EcosystemType::Deno, ecosystems.deno)?;
	let dart_ecosystem =
		normalize_ecosystem_settings(&contents, "dart", EcosystemType::Dart, ecosystems.dart)?;
	let defaults_changelog_policy = defaults
		.changelog
		.as_ref()
		.map(RawChangelogConfig::as_defaults_definition);
	let default_changelog_format = defaults
		.changelog
		.as_ref()
		.and_then(RawChangelogConfig::format)
		.unwrap_or_default();
	let packages = package
		.into_iter()
		.map(|(id, package)| {
			let package_type = package.package_type.or(default_package_type).ok_or_else(|| {
				config_diagnostic(
					&contents,
					format!(
						"package `{id}` must declare `type` or set `[defaults].package_type`"
					),
					vec![config_section_label(
						&contents,
						"package",
						&id,
						"package missing type",
					)],
					Some(
						"set `type = \"cargo\"` (or another supported type) on the package, or set `[defaults].package_type` for a single-ecosystem repository"
							.to_string(),
					),
				)
			})?;
			let changelog = package
				.changelog
				.as_ref()
				.and_then(|definition| {
					definition.resolve_for_package(&package.path, false).map(|path| ChangelogTarget {
						path,
						format: definition.format().unwrap_or(default_changelog_format),
					})
				})
				.or_else(|| {
					default_package_changelog.as_ref().and_then(|definition| {
						definition.resolve_for_package(&package.path, true).map(|path| ChangelogTarget {
							path,
							format: definition.format().unwrap_or(default_changelog_format),
						})
					})
				});
			let inferred_ecosystem_type = package_type_to_ecosystem_type(package_type);
			let inherited_versioned_files = if package.ignore_ecosystem_versioned_files {
				Vec::new()
			} else {
				match inferred_ecosystem_type {
					EcosystemType::Cargo => cargo_ecosystem.versioned_files.clone(),
					EcosystemType::Npm => npm_ecosystem.versioned_files.clone(),
					EcosystemType::Deno => deno_ecosystem.versioned_files.clone(),
					EcosystemType::Dart => dart_ecosystem.versioned_files.clone(),
				}
			};
			let mut versioned_files = inherited_versioned_files;
			versioned_files.extend(normalize_versioned_files(
				&contents,
				package.versioned_files,
				inferred_ecosystem_type,
				"package",
				&id,
				true,
			)?);
			Ok::<_, MonochangeError>(PackageDefinition {
				id,
				path: package.path,
				package_type,
				changelog,
				extra_changelog_sections: merge_extra_changelog_sections(
					&default_extra_changelog_sections,
					package.extra_changelog_sections,
				),
				empty_update_message: package.empty_update_message,
				release_title: package.release_title,
				changelog_version_title: package.changelog_version_title,
				versioned_files,
				ignore_ecosystem_versioned_files: package.ignore_ecosystem_versioned_files,
				ignored_paths: package.ignored_paths,
				additional_paths: package.additional_paths,
				tag: package.tag,
				release: package.release,
				version_format: package.version_format,
			})
		})
		.collect::<Result<Vec<_>, _>>()?;
	let groups = group
		.into_iter()
		.map(|(id, group)| {
			let changelog = match group.changelog.as_ref() {
				None => None,
				Some(definition) => match definition.resolve_for_group() {
					Some(path) => Some(ChangelogTarget {
						path,
						format: definition.format().unwrap_or(default_changelog_format),
					}),
					None if definition.is_disabled() => None,
					None => {
						return Err(config_diagnostic(
							&contents,
							format!(
								"group `{id}` changelog must declare a `path` when changelog output is enabled"
							),
							vec![config_section_label(
								&contents,
								"group",
								&id,
								"group changelog missing path",
							)],
							Some(
								"set `changelog = \"changelog.md\"` or `[group.<id>.changelog].path` when enabling grouped changelog output"
									.to_string(),
							),
						));
					}
				},
			};
			let changelog_include = parse_group_changelog_include(
				&contents,
				&id,
				&group.packages,
				group.changelog.as_ref().and_then(RawChangelogConfig::include),
			)?;
			Ok::<_, MonochangeError>(GroupDefinition {
				id: id.clone(),
				packages: group.packages,
				changelog,
				changelog_include,
				extra_changelog_sections: merge_extra_changelog_sections(
					&default_extra_changelog_sections,
					group.extra_changelog_sections,
				),
				empty_update_message: group.empty_update_message,
				release_title: group.release_title,
				changelog_version_title: group.changelog_version_title,
				versioned_files: normalize_versioned_files(
					&contents,
					group.versioned_files,
					EcosystemType::Cargo,
					"group",
					&id,
					false,
				)?,
				tag: group.tag,
				release: group.release,
				version_format: group.version_format,
			})
		})
		.collect::<Result<Vec<_>, _>>()?;
	if source.is_some() && github.is_some() {
		return Err(MonochangeError::Config(
			"configure either `[source]` or legacy `[github]`, but not both".to_string(),
		));
	}
	let changesets = ChangesetSettings {
		verify: ChangesetVerificationSettings {
			enabled: changesets.verify.enabled,
			required: changesets.verify.required,
			skip_labels: changesets.verify.skip_labels,
			comment_on_failure: changesets.verify.comment_on_failure,
		},
	};
	let source = source.map(|source| {
		SourceConfiguration {
			provider: source.provider,
			owner: source.owner,
			repo: source.repo,
			host: source.host,
			api_url: source.api_url,
			releases: ProviderReleaseSettings {
				enabled: source.releases.enabled,
				draft: source.releases.draft,
				prerelease: source.releases.prerelease,
				generate_notes: source.releases.generate_notes,
				source: source.releases.source,
			},
			pull_requests: ProviderMergeRequestSettings {
				enabled: source.pull_requests.enabled,
				branch_prefix: source.pull_requests.branch_prefix,
				base: source.pull_requests.base,
				title: source.pull_requests.title,
				labels: source.pull_requests.labels,
				auto_merge: source.pull_requests.auto_merge,
			},
			bot: ProviderBotSettings {
				changesets: ProviderChangesetBotSettings {
					enabled: source.bot.changesets.enabled,
					required: source.bot.changesets.required,
					skip_labels: source.bot.changesets.skip_labels,
					comment_on_failure: source.bot.changesets.comment_on_failure,
					changed_paths: source.bot.changesets.changed_paths,
					ignored_paths: source.bot.changesets.ignored_paths,
				},
			},
		}
	});
	let legacy_github = if let Some(source) = &source {
		(source.provider == SourceProvider::GitHub).then(|| {
			GitHubConfiguration {
				owner: source.owner.clone(),
				repo: source.repo.clone(),
				releases: source.releases.clone(),
				pull_requests: source.pull_requests.clone(),
				bot: source.bot.clone(),
			}
		})
	} else {
		github.map(|github| {
			GitHubConfiguration {
				owner: github.owner,
				repo: github.repo,
				releases: ProviderReleaseSettings {
					enabled: github.releases.enabled,
					draft: github.releases.draft,
					prerelease: github.releases.prerelease,
					generate_notes: github.releases.generate_notes,
					source: github.releases.source,
				},
				pull_requests: ProviderMergeRequestSettings {
					enabled: github.pull_requests.enabled,
					branch_prefix: github.pull_requests.branch_prefix,
					base: github.pull_requests.base,
					title: github.pull_requests.title,
					labels: github.pull_requests.labels,
					auto_merge: github.pull_requests.auto_merge,
				},
				bot: ProviderBotSettings {
					changesets: ProviderChangesetBotSettings {
						enabled: github.bot.changesets.enabled,
						required: github.bot.changesets.required,
						skip_labels: github.bot.changesets.skip_labels,
						comment_on_failure: github.bot.changesets.comment_on_failure,
						changed_paths: github.bot.changesets.changed_paths,
						ignored_paths: github.bot.changesets.ignored_paths,
					},
				},
			}
		})
	};
	let source = source.or_else(|| {
		legacy_github.as_ref().map(|github| {
			SourceConfiguration {
				provider: SourceProvider::GitHub,
				owner: github.owner.clone(),
				repo: github.repo.clone(),
				host: None,
				api_url: None,
				releases: github.releases.clone(),
				pull_requests: github.pull_requests.clone(),
				bot: github.bot.clone(),
			}
		})
	});

	validate_cli(&cli)?;
	validate_release_notes_configuration(
		&contents,
		&release_notes,
		&defaults.extra_changelog_sections,
		&packages,
		&groups,
	)?;
	validate_changesets_configuration(&changesets, &packages)?;
	validate_github_configuration(legacy_github.as_ref())?;
	validate_source_configuration(source.as_ref())?;
	for (ecosystem_id, ecosystem_settings) in [
		("cargo", &cargo_ecosystem),
		("npm", &npm_ecosystem),
		("deno", &deno_ecosystem),
		("dart", &dart_ecosystem),
	] {
		let declared_packages = packages
			.iter()
			.map(|package| package.id.as_str())
			.collect::<BTreeSet<_>>();
		validate_versioned_files(
			root,
			&contents,
			&ecosystem_settings.versioned_files,
			&declared_packages,
			"ecosystems",
			ecosystem_id,
		)?;
		validate_lockfile_commands(root, ecosystem_id, &ecosystem_settings.lockfile_commands)?;
	}
	validate_package_and_group_definitions(root, &contents, &packages, &groups)?;
	validate_cli_runtime_requirements(&cli, &changesets, source.as_ref())?;

	Ok(WorkspaceConfiguration {
		root_path: root.to_path_buf(),
		defaults: WorkspaceDefaults {
			parent_bump: defaults.parent_bump,
			include_private: defaults.include_private,
			warn_on_group_mismatch: defaults.warn_on_group_mismatch,
			strict_version_conflicts: defaults.strict_version_conflicts,
			package_type: defaults.package_type,
			changelog: defaults_changelog_policy,
			changelog_format: default_changelog_format,
			extra_changelog_sections: defaults.extra_changelog_sections,
			empty_update_message: defaults.empty_update_message,
			release_title: defaults.release_title,
			changelog_version_title: defaults.changelog_version_title,
		},
		release_notes: ReleaseNotesSettings {
			change_templates: release_notes.change_templates,
		},
		packages,
		groups,
		cli,
		changesets,
		source,
		cargo: cargo_ecosystem,
		npm: npm_ecosystem,
		deno: deno_ecosystem,
		dart: dart_ecosystem,
	})
}

#[derive(Debug)]
struct ChangeTypeLookup {
	valid_types: Vec<String>,
	default_bumps: HashMap<String, BumpSeverity>,
}

#[derive(Debug)]
pub struct ChangesetLoadContext<'a> {
	package_ids: HashSet<&'a str>,
	groups_by_id: HashMap<&'a str, &'a GroupDefinition>,
	package_reference_matches: HashMap<String, Vec<&'a str>>,
	package_versions: HashMap<&'a str, &'a Version>,
	change_types_by_target: HashMap<&'a str, ChangeTypeLookup>,
}

/// Build reusable lookup tables for loading many `.changeset/*.md` files.
///
/// Performance note:
/// release planning often parses dozens or hundreds of changesets in one run.
/// The older path rebuilt package/group lookup maps for every file and resolved
/// package references by rescanning the full discovered package list each time.
/// On the monochange repo that repeated work dominated `mc release --dry-run`
/// once the obvious git/network costs were removed.
///
/// This context shifts that cost to a single up-front pass so each changeset can
/// reuse the same reference indexes, version lookups, and configured change-type
/// metadata. Keeping the optimization centralized here also makes it harder for a
/// future call site to accidentally fall back to the slow per-file rebuild.
#[must_use]
pub fn build_changeset_load_context<'a>(
	configuration: &'a WorkspaceConfiguration,
	packages: &'a [PackageRecord],
) -> ChangesetLoadContext<'a> {
	let package_ids = configuration
		.packages
		.iter()
		.map(|package| package.id.as_str())
		.collect::<HashSet<_>>();
	let groups_by_id = configuration
		.groups
		.iter()
		.map(|group| (group.id.as_str(), group))
		.collect::<HashMap<_, _>>();
	let package_versions = packages
		.iter()
		.filter_map(|package| {
			package
				.current_version
				.as_ref()
				.map(|version| (package.id.as_str(), version))
		})
		.collect::<HashMap<_, _>>();
	let mut package_reference_matches = HashMap::<String, Vec<&'a str>>::new();
	for package in packages {
		for reference in changeset_package_references(configuration.root_path.as_path(), package) {
			package_reference_matches
				.entry(reference)
				.or_default()
				.push(package.id.as_str());
		}
	}
	let mut change_types_by_target = HashMap::new();
	for package in &configuration.packages {
		change_types_by_target.insert(
			package.id.as_str(),
			build_change_type_lookup(&package.extra_changelog_sections),
		);
	}
	for group in &configuration.groups {
		change_types_by_target.insert(
			group.id.as_str(),
			build_change_type_lookup(&group.extra_changelog_sections),
		);
	}
	ChangesetLoadContext {
		package_ids,
		groups_by_id,
		package_reference_matches,
		package_versions,
		change_types_by_target,
	}
}

fn build_change_type_lookup(sections: &[ExtraChangelogSection]) -> ChangeTypeLookup {
	let mut valid_types = sections
		.iter()
		.flat_map(|section| section.types.iter())
		.map(|value| value.trim())
		.filter(|value| !value.is_empty())
		.map(ToString::to_string)
		.collect::<Vec<_>>();
	valid_types.sort();
	valid_types.dedup();
	let default_bumps = sections
		.iter()
		.flat_map(|section| {
			section.types.iter().map(|change_type| {
				(
					change_type.trim().to_string(),
					section.default_bump.unwrap_or(BumpSeverity::None),
				)
			})
		})
		.filter(|(change_type, _)| !change_type.is_empty())
		.collect::<HashMap<_, _>>();
	ChangeTypeLookup {
		valid_types,
		default_bumps,
	}
}

fn changeset_package_references(root: &Path, package: &PackageRecord) -> Vec<String> {
	let mut references = vec![package.name.clone(), package.id.clone()];
	if let Some(config_id) = package.metadata.get("config_id") {
		references.push(config_id.clone());
	}
	if let Some(manifest_path) = relative_to_root(root, &package.manifest_path)
		.and_then(|path| path.to_str().map(ToString::to_string))
	{
		references.push(manifest_path);
	}
	if let Some(directory_path) = package
		.manifest_path
		.parent()
		.and_then(|path| relative_to_root(root, path))
		.and_then(|path| path.to_str().map(ToString::to_string))
	{
		references.push(directory_path);
	}
	references.sort();
	references.dedup();
	references
}

pub fn load_change_signals(
	changes_path: &Path,
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> MonochangeResult<Vec<ChangeSignal>> {
	let context = build_changeset_load_context(configuration, packages);
	Ok(load_changeset_file_with_context(changes_path, &context)?.signals)
}

pub fn load_changeset_file(
	changes_path: &Path,
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> MonochangeResult<LoadedChangesetFile> {
	let context = build_changeset_load_context(configuration, packages);
	load_changeset_file_with_context(changes_path, &context)
}

/// Load a changeset file with precomputed package/group indexes.
///
/// Performance note:
/// this is the hot path for `mc release --dry-run` on repositories that keep a
/// large `.changeset/` queue. The slow version repeated all of the following for
/// every file:
///
/// - rebuild the package-id set
/// - rebuild the group lookup map
/// - rescan discovered packages for every package reference
/// - rescan package/group changelog metadata for every `type = ...` lookup
///
/// Those tiny costs multiplied by every pending changeset. Keeping the fast path
/// in a dedicated function with an explicit `ChangesetLoadContext` makes the
/// intended usage obvious and gives future maintainers one place to extend the
/// shared indexes instead of accidentally reintroducing per-file recomputation.
pub fn load_changeset_file_with_context(
	changes_path: &Path,
	context: &ChangesetLoadContext<'_>,
) -> MonochangeResult<LoadedChangesetFile> {
	let contents = fs::read_to_string(changes_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			changes_path.display()
		))
	})?;
	load_changeset_contents_with_context(changes_path, &contents, context)
}

/// Parse already-loaded changeset text with the shared lookup context.
///
/// Performance note:
/// release planning can batch-read many tiny files much faster than it can let
/// worker threads fight over opening them one by one. This helper keeps the
/// actual parse/validation logic separate from file I/O so callers can choose a
/// better read strategy without duplicating the parser itself.
pub fn load_changeset_contents_with_context(
	changes_path: &Path,
	contents: &str,
	context: &ChangesetLoadContext<'_>,
) -> MonochangeResult<LoadedChangesetFile> {
	let raw = if changes_path.extension().and_then(|value| value.to_str()) == Some("md") {
		parse_markdown_change_file_with_context(contents, changes_path, context)?
	} else {
		toml::from_str::<RawChangeFile>(contents).map_err(|error| {
			MonochangeError::Config(format!(
				"failed to parse {}: {error}",
				changes_path.display()
			))
		})?
	};

	let referenced_packages: HashSet<String> = raw
		.changes
		.iter()
		.filter(|change| context.package_ids.contains(change.package.as_str()))
		.map(|change| change.package.clone())
		.collect();

	for change in &raw.changes {
		if !context.package_ids.contains(change.package.as_str())
			&& !context.groups_by_id.contains_key(change.package.as_str())
		{
			return Err(changeset_diagnostic(
				contents,
				changes_path,
				format!(
					"changeset `{}` references unknown package or group `{}`",
					changes_path.display(),
					change.package,
				),
				vec![changeset_key_label(
					contents,
					&change.package,
					"unknown package or group",
				)],
				Some("declare the package or group id in monochange.toml before referencing it in a changeset".to_string()),
			));
		}
	}

	let summary = raw.changes.first().and_then(|change| change.reason.clone());
	let details = raw
		.changes
		.first()
		.and_then(|change| change.details.clone());
	let mut seen_package_ids = HashSet::new();
	let mut signals = Vec::new();
	let mut targets = Vec::new();
	for change in raw.changes {
		if let Some(group) = context.groups_by_id.get(change.package.as_str()) {
			let explicit_version = change.version.clone();
			let inferred_bump = match change.bump {
				Some(bump) => Some(bump),
				None => {
					infer_group_bump_from_explicit_version_with_context(
						group,
						context,
						explicit_version.as_ref(),
					)?
				}
			};
			targets.push(LoadedChangesetTarget {
				id: change.package.clone(),
				kind: ChangesetTargetKind::Group,
				bump: inferred_bump,
				explicit_version: explicit_version.clone(),
				origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				change_type: change.change_type.clone(),
			});
			for member_id in &group.packages {
				if referenced_packages.contains(member_id.as_str()) {
					continue;
				}
				let package_id = resolve_package_reference_with_context(member_id, context)?;
				if !seen_package_ids.insert(package_id.clone()) {
					return Err(changeset_diagnostic(
						contents,
						changes_path,
						format!(
							"duplicate change entry for `{package_id}` in {}",
							changes_path.display()
						),
						vec![changeset_key_label(
							contents,
							member_id,
							"duplicate package target",
						)],
						Some("keep one change entry per effective package target".to_string()),
					));
				}
				signals.push(ChangeSignal {
					package_id,
					requested_bump: inferred_bump,
					explicit_version: explicit_version.clone(),
					change_origin: "direct-change".to_string(),
					evidence_refs: Vec::new(),
					notes: change.reason.clone(),
					details: change.details.clone(),
					change_type: change.change_type.clone(),
					source_path: changes_path.to_path_buf(),
				});
			}
		} else {
			let package_id = resolve_package_reference_with_context(&change.package, context)?;
			let explicit_version = change.version;
			let inferred_bump = change.bump.or_else(|| {
				infer_package_bump_from_explicit_version_with_context(
					&package_id,
					context,
					explicit_version.as_ref(),
				)
			});
			targets.push(LoadedChangesetTarget {
				id: change.package.clone(),
				kind: ChangesetTargetKind::Package,
				bump: inferred_bump,
				explicit_version: explicit_version.clone(),
				origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				change_type: change.change_type.clone(),
			});
			if !seen_package_ids.insert(package_id.clone()) {
				return Err(changeset_diagnostic(
					contents,
					changes_path,
					format!(
						"duplicate change entry for `{package_id}` in {}",
						changes_path.display()
					),
					vec![changeset_key_label(
						contents,
						&change.package,
						"duplicate package target",
					)],
					Some("keep one change entry per effective package target".to_string()),
				));
			}
			signals.push(ChangeSignal {
				package_id,
				requested_bump: inferred_bump,
				explicit_version,
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: change.reason,
				details: change.details,
				change_type: change.change_type,
				source_path: changes_path.to_path_buf(),
			});
		}
	}

	Ok(LoadedChangesetFile {
		path: changes_path.to_path_buf(),
		summary,
		details,
		targets,
		signals,
	})
}

fn infer_package_bump_from_explicit_version_with_context(
	package_id: &str,
	context: &ChangesetLoadContext<'_>,
	explicit_version: Option<&Version>,
) -> Option<BumpSeverity> {
	let explicit_version = explicit_version?;
	context
		.package_versions
		.get(package_id)
		.map(|current_version| infer_bump_from_versions(current_version, explicit_version))
}

fn infer_group_bump_from_explicit_version_with_context(
	group: &GroupDefinition,
	context: &ChangesetLoadContext<'_>,
	explicit_version: Option<&Version>,
) -> MonochangeResult<Option<BumpSeverity>> {
	let Some(explicit_version) = explicit_version else {
		return Ok(None);
	};
	let mut max_version: Option<&Version> = None;
	for member_id in &group.packages {
		let package_id = resolve_package_reference_with_context(member_id, context)?;
		if let Some(current_version) = context.package_versions.get(package_id.as_str()) {
			max_version = Some(match max_version {
				Some(current_max) if *current_version > current_max => current_version,
				Some(current_max) => current_max,
				None => current_version,
			});
		}
	}
	Ok(max_version
		.map(|current_version| infer_bump_from_versions(current_version, explicit_version)))
}

fn resolve_package_reference_with_context(
	reference: &str,
	context: &ChangesetLoadContext<'_>,
) -> MonochangeResult<String> {
	match context
		.package_reference_matches
		.get(reference)
		.map(Vec::as_slice)
		.unwrap_or_default()
	{
		[] => {
			Err(MonochangeError::Config(format!(
				"change package reference `{reference}` did not match any discovered package"
			)))
		}
		[package_id] => Ok((*package_id).to_string()),
		package_ids => {
			Err(MonochangeError::Config(format!(
				"change package reference `{reference}` matched multiple packages: {}",
				package_ids.join(", ")
			)))
		}
	}
}

fn configured_change_type_default_bump_with_context(
	context: &ChangesetLoadContext<'_>,
	target: &str,
	change_type: &str,
) -> Option<BumpSeverity> {
	context
		.change_types_by_target
		.get(target)
		.and_then(|lookup| lookup.default_bumps.get(change_type))
		.copied()
}

fn configured_change_types_with_context(
	context: &ChangesetLoadContext<'_>,
	target: &str,
) -> Vec<String> {
	context
		.change_types_by_target
		.get(target)
		.map(|lookup| lookup.valid_types.clone())
		.unwrap_or_default()
}

fn parse_markdown_change_file_with_context(
	contents: &str,
	changes_path: &Path,
	context: &ChangesetLoadContext<'_>,
) -> MonochangeResult<RawChangeFile> {
	let contents = &contents.replace("\r\n", "\n").replace('\r', "\n");
	let Some(without_opening) = contents.strip_prefix("---") else {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: missing markdown frontmatter",
			changes_path.display()
		)));
	};
	let Some((frontmatter, body_with_separator)) = without_opening.split_once("\n---\n") else {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: unterminated markdown frontmatter",
			changes_path.display()
		)));
	};
	let body = body_with_separator.trim();
	let mapping = serde_yaml_ng::from_str::<Mapping>(frontmatter).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse {} frontmatter: {error}",
			changes_path.display()
		))
	})?;
	let (reason, details) = markdown_change_text(body);
	let mut changes = Vec::new();

	for (key, value) in &mapping {
		let Some(package) = key.as_str() else {
			continue;
		};
		let (requested_bump, explicit_version, change_type) =
			parse_markdown_change_target_with_context(value, changes_path, package, context)?;
		changes.push(RawChangeEntry {
			package: package.to_string(),
			bump: requested_bump,
			version: explicit_version,
			reason: reason.clone(),
			details: details.clone(),
			change_type,
		});
	}

	Ok(RawChangeFile { changes })
}

fn parse_markdown_change_target_with_context(
	value: &serde_yaml_ng::Value,
	changes_path: &Path,
	package: &str,
	context: &ChangesetLoadContext<'_>,
) -> MonochangeResult<(Option<BumpSeverity>, Option<Version>, Option<String>)> {
	if let Some(token) = value
		.as_str()
		.map(str::trim)
		.filter(|value| !value.is_empty())
	{
		if let Some(bump) = parse_bump_severity(token) {
			return Ok((Some(bump), None, None));
		}
		if let Some(default_bump) =
			configured_change_type_default_bump_with_context(context, package, token)
		{
			return Ok((Some(default_bump), None, Some(token.to_string())));
		}
		if context.package_ids.contains(package) || context.groups_by_id.contains_key(package) {
			let valid_types = configured_change_types_with_context(context, package);
			let valid_types_help = if valid_types.is_empty() {
				String::new()
			} else {
				format!(
					" or one of the configured types: {}",
					valid_types.join(", ")
				)
			};
			return Err(MonochangeError::Config(format!(
				"failed to parse {}: target `{package}` has invalid scalar value `{token}`; expected one of `none`, `patch`, `minor`, `major`{valid_types_help}`",
				changes_path.display()
			)));
		}
		return Ok((None, None, Some(token.to_string())));
	}

	let Some(mapping) = value.as_mapping() else {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: target `{package}` must map to `none`, `patch`, `minor`, `major`, a configured change type, or to a table with `bump`, `version`, and/or `type`",
			changes_path.display()
		)));
	};

	let allowed_keys = ["bump", "version", "type"];
	let unknown_keys = mapping
		.keys()
		.filter_map(serde_yaml_ng::Value::as_str)
		.filter(|key| !allowed_keys.contains(key))
		.collect::<Vec<_>>();
	if !unknown_keys.is_empty() {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: target `{package}` uses unsupported field(s): {}",
			changes_path.display(),
			unknown_keys.join(", ")
		)));
	}

	let requested_bump = mapping
		.get(serde_yaml_ng::Value::String("bump".to_string()))
		.and_then(serde_yaml_ng::Value::as_str)
		.map(|value| {
			parse_bump_severity(value).ok_or_else(|| {
				MonochangeError::Config(format!(
					"failed to parse {}: target `{package}` has invalid bump `{value}`; expected `none`, `patch`, `minor`, or `major`",
					changes_path.display()
				))
			})
		})
		.transpose()?;
	let explicit_version = mapping
		.get(serde_yaml_ng::Value::String("version".to_string()))
		.and_then(serde_yaml_ng::Value::as_str)
		.map(|value| {
			Version::parse(value).map_err(|error| {
				MonochangeError::Config(format!(
					"failed to parse {}: target `{package}` has invalid version `{value}`: {error}",
					changes_path.display()
				))
			})
		})
		.transpose()?;
	let change_type = mapping
		.get(serde_yaml_ng::Value::String("type".to_string()))
		.and_then(serde_yaml_ng::Value::as_str)
		.map(str::trim)
		.filter(|value| !value.is_empty())
		.map(ToString::to_string);
	if let Some(change_type) = change_type.as_deref() {
		validate_configured_change_type_with_context(context, changes_path, package, change_type)?;
	}
	let requested_bump = requested_bump.or_else(|| {
		change_type.as_deref().and_then(|change_type| {
			configured_change_type_default_bump_with_context(context, package, change_type)
		})
	});
	if requested_bump.is_none() && explicit_version.is_none() && change_type.is_none() {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: target `{package}` must declare `bump`, `version`, `type`, or a valid scalar shorthand",
			changes_path.display()
		)));
	}
	if requested_bump == Some(BumpSeverity::None)
		&& explicit_version.is_none()
		&& change_type.is_none()
	{
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: target `{package}` must not use `bump = \"none\"` without also declaring `type` or `version`",
			changes_path.display()
		)));
	}
	Ok((requested_bump, explicit_version, change_type))
}

fn validate_configured_change_type_with_context(
	context: &ChangesetLoadContext<'_>,
	changes_path: &Path,
	target: &str,
	change_type: &str,
) -> MonochangeResult<()> {
	if !context.package_ids.contains(target) && !context.groups_by_id.contains_key(target) {
		return Ok(());
	}
	let valid_types = configured_change_types_with_context(context, target);
	if valid_types.iter().any(|candidate| candidate == change_type) {
		return Ok(());
	}
	let valid_types_help = if valid_types.is_empty() {
		"no configured types are available for this target".to_string()
	} else {
		format!("valid types: {}", valid_types.join(", "))
	};
	Err(MonochangeError::Config(format!(
		"failed to parse {}: target `{target}` has invalid type `{change_type}`; {valid_types_help}",
		changes_path.display()
	)))
}

#[cfg(test)]
fn infer_package_bump_from_explicit_version(
	package_id: &str,
	packages: &[PackageRecord],
	explicit_version: Option<&Version>,
) -> Option<BumpSeverity> {
	let explicit_version = explicit_version?;
	packages
		.iter()
		.find(|package| package.id == package_id)
		.and_then(|package| package.current_version.as_ref())
		.map(|current_version| infer_bump_from_versions(current_version, explicit_version))
}

#[cfg(test)]
fn infer_group_bump_from_explicit_version(
	group: &GroupDefinition,
	workspace_root: &Path,
	packages: &[PackageRecord],
	explicit_version: Option<&Version>,
) -> MonochangeResult<Option<BumpSeverity>> {
	let Some(explicit_version) = explicit_version else {
		return Ok(None);
	};
	let mut max_version: Option<&Version> = None;
	for member_id in &group.packages {
		let package_id = resolve_package_reference(member_id, workspace_root, packages)?;
		if let Some(current_version) = packages
			.iter()
			.find(|package| package.id == package_id)
			.and_then(|package| package.current_version.as_ref())
		{
			max_version = Some(match max_version {
				Some(current_max) if current_version > current_max => current_version,
				Some(current_max) => current_max,
				None => current_version,
			});
		}
	}
	Ok(max_version
		.map(|current_version| infer_bump_from_versions(current_version, explicit_version)))
}

fn infer_bump_from_versions(current_version: &Version, explicit_version: &Version) -> BumpSeverity {
	if explicit_version.major > current_version.major {
		BumpSeverity::Major
	} else if explicit_version.minor > current_version.minor {
		BumpSeverity::Minor
	} else if explicit_version.patch > current_version.patch
		|| explicit_version.pre != current_version.pre
		|| explicit_version.build != current_version.build
	{
		BumpSeverity::Patch
	} else {
		BumpSeverity::None
	}
}

pub fn resolve_package_reference(
	reference: &str,
	workspace_root: &Path,
	packages: &[PackageRecord],
) -> MonochangeResult<String> {
	let matching_package_ids = find_matching_package_ids(reference, workspace_root, packages);
	match matching_package_ids.as_slice() {
		[] => {
			Err(MonochangeError::Config(format!(
				"change package reference `{reference}` did not match any discovered package"
			)))
		}
		[package_id] => Ok(package_id.clone()),
		_ => {
			Err(MonochangeError::Config(format!(
				"change package reference `{reference}` matched multiple packages: {}",
				matching_package_ids.join(", ")
			)))
		}
	}
}

fn parse_markdown_change_file(
	contents: &str,
	changes_path: &Path,
	configuration: &WorkspaceConfiguration,
) -> MonochangeResult<RawChangeFile> {
	// Normalize all line ending styles to LF: CRLF (Windows), bare CR
	// (classic Mac), and mixed endings.
	let contents = &contents.replace("\r\n", "\n").replace('\r', "\n");
	let Some(without_opening) = contents.strip_prefix("---") else {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: missing markdown frontmatter",
			changes_path.display()
		)));
	};
	let Some((frontmatter, body_with_separator)) = without_opening.split_once("\n---\n") else {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: unterminated markdown frontmatter",
			changes_path.display()
		)));
	};
	let body = body_with_separator.trim();
	let mapping = serde_yaml_ng::from_str::<Mapping>(frontmatter).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse {} frontmatter: {error}",
			changes_path.display()
		))
	})?;
	let (reason, details) = markdown_change_text(body);
	let mut changes = Vec::new();

	for (key, value) in &mapping {
		let Some(package) = key.as_str() else {
			continue;
		};
		let (requested_bump, explicit_version, change_type) =
			parse_markdown_change_target(value, changes_path, package, configuration)?;
		changes.push(RawChangeEntry {
			package: package.to_string(),
			bump: requested_bump,
			version: explicit_version,
			reason: reason.clone(),
			details: details.clone(),
			change_type,
		});
	}

	Ok(RawChangeFile { changes })
}

fn markdown_heading_level(line: &str) -> Option<usize> {
	let trimmed = line.trim_start();
	let level = trimmed
		.chars()
		.take_while(|character| *character == '#')
		.count();
	if !(1..=6).contains(&level) {
		return None;
	}
	let remainder = &trimmed[level..];
	if remainder.is_empty() || remainder.starts_with(char::is_whitespace) {
		Some(level)
	} else {
		None
	}
}

fn normalize_markdown_heading_levels(
	markdown: &str,
	summary_heading_level: Option<usize>,
	summary_render_level: usize,
) -> String {
	let mut in_fenced_code_block = false;
	let mut first_detail_heading_level = None;
	markdown
		.lines()
		.map(|line| {
			let trimmed = line.trim_start();
			if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
				in_fenced_code_block = !in_fenced_code_block;
				return line.to_string();
			}
			if in_fenced_code_block {
				return line.to_string();
			}
			let Some(authored_level) = markdown_heading_level(line) else {
				return line.to_string();
			};
			let summary_context_level = if let Some(summary_heading_level) = summary_heading_level {
				summary_render_level as isize + authored_level as isize
					- summary_heading_level as isize
			} else {
				let baseline = *first_detail_heading_level.get_or_insert(authored_level);
				(summary_render_level + 1) as isize + authored_level as isize - baseline as isize
			};
			let normalized_level = summary_context_level.clamp(1, 6) as usize;
			let text = trimmed.trim_start_matches('#').trim();
			format!("{} {text}", "#".repeat(normalized_level))
		})
		.collect::<Vec<_>>()
		.join("\n")
}

fn markdown_change_text(body: &str) -> (Option<String>, Option<String>) {
	let trimmed = body.trim();
	if trimmed.is_empty() {
		return (None, None);
	}
	let lines = trimmed.lines().collect::<Vec<_>>();
	let Some((summary_index, summary_line)) = lines.iter().enumerate().find_map(|(index, line)| {
		let candidate = line.trim();
		if candidate.is_empty() {
			None
		} else {
			Some((index, candidate))
		}
	}) else {
		return (None, None);
	};
	let summary_heading_level = markdown_heading_level(summary_line);
	let summary = summary_heading_level.map_or_else(
		|| summary_line.to_string(),
		|_| summary_line.trim_start_matches('#').trim().to_string(),
	);
	let details = lines
		.iter()
		.skip(summary_index + 1)
		.copied()
		.collect::<Vec<_>>()
		.join("\n");
	let normalized_details = normalize_markdown_heading_levels(&details, summary_heading_level, 4)
		.trim()
		.to_string();
	(
		Some(summary),
		if normalized_details.is_empty() {
			None
		} else {
			Some(normalized_details)
		},
	)
}

fn configured_change_sections<'config>(
	configuration: &'config WorkspaceConfiguration,
	target: &str,
) -> &'config [ExtraChangelogSection] {
	if let Some(package) = configuration.package_by_id(target) {
		return package.extra_changelog_sections.as_slice();
	}
	if let Some(group) = configuration.group_by_id(target) {
		return group.extra_changelog_sections.as_slice();
	}
	&[]
}

fn configured_change_type_default_bump(
	configuration: &WorkspaceConfiguration,
	target: &str,
	change_type: &str,
) -> Option<BumpSeverity> {
	configured_change_sections(configuration, target)
		.iter()
		.find(|section| {
			section
				.types
				.iter()
				.any(|candidate| candidate.trim() == change_type)
		})
		.map(|section| section.default_bump.unwrap_or(BumpSeverity::None))
}

fn configured_change_types(configuration: &WorkspaceConfiguration, target: &str) -> Vec<String> {
	configured_change_sections(configuration, target)
		.iter()
		.flat_map(|section| section.types.iter())
		.map(|value| value.trim().to_string())
		.filter(|value| !value.is_empty())
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect()
}

fn validate_configured_change_type(
	configuration: &WorkspaceConfiguration,
	changes_path: &Path,
	target: &str,
	change_type: &str,
) -> MonochangeResult<()> {
	if configuration.package_by_id(target).is_none() && configuration.group_by_id(target).is_none()
	{
		return Ok(());
	}
	let valid_types = configured_change_types(configuration, target);
	if valid_types.iter().any(|candidate| candidate == change_type) {
		return Ok(());
	}
	let valid_types_help = if valid_types.is_empty() {
		"no configured types are available for this target".to_string()
	} else {
		format!("valid types: {}", valid_types.join(", "))
	};
	Err(MonochangeError::Config(format!(
		"failed to parse {}: target `{target}` has invalid type `{change_type}`; {valid_types_help}",
		changes_path.display()
	)))
}

fn parse_markdown_change_target(
	value: &serde_yaml_ng::Value,
	changes_path: &Path,
	package: &str,
	configuration: &WorkspaceConfiguration,
) -> MonochangeResult<(Option<BumpSeverity>, Option<Version>, Option<String>)> {
	if let Some(token) = value
		.as_str()
		.map(str::trim)
		.filter(|value| !value.is_empty())
	{
		if let Some(bump) = parse_bump_severity(token) {
			return Ok((Some(bump), None, None));
		}
		if let Some(default_bump) =
			configured_change_type_default_bump(configuration, package, token)
		{
			return Ok((Some(default_bump), None, Some(token.to_string())));
		}
		if configuration.package_by_id(package).is_some()
			|| configuration.group_by_id(package).is_some()
		{
			let valid_types = configured_change_types(configuration, package);
			let valid_types_help = if valid_types.is_empty() {
				String::new()
			} else {
				format!(
					" or one of the configured types: {}",
					valid_types.join(", ")
				)
			};
			return Err(MonochangeError::Config(format!(
				"failed to parse {}: target `{package}` has invalid scalar value `{token}`; expected one of `none`, `patch`, `minor`, `major`{valid_types_help}`",
				changes_path.display()
			)));
		}
		return Ok((None, None, Some(token.to_string())));
	}

	let Some(mapping) = value.as_mapping() else {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: target `{package}` must map to `none`, `patch`, `minor`, `major`, a configured change type, or to a table with `bump`, `version`, and/or `type`",
			changes_path.display()
		)));
	};

	let allowed_keys = ["bump", "version", "type"];
	let unknown_keys = mapping
		.keys()
		.filter_map(serde_yaml_ng::Value::as_str)
		.filter(|key| !allowed_keys.contains(key))
		.collect::<Vec<_>>();
	if !unknown_keys.is_empty() {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: target `{package}` uses unsupported field(s): {}",
			changes_path.display(),
			unknown_keys.join(", ")
		)));
	}

	let bump = mapping
		.get(serde_yaml_ng::Value::String("bump".to_string()))
		.and_then(serde_yaml_ng::Value::as_str)
		.map(|value| {
			parse_bump_severity(value).ok_or_else(|| {
				MonochangeError::Config(format!(
					"failed to parse {}: target `{package}` has invalid bump `{value}`; expected `none`, `patch`, `minor`, or `major`",
					changes_path.display()
				))
			})
		})
		.transpose()?;
	let version = mapping
		.get(serde_yaml_ng::Value::String("version".to_string()))
		.and_then(serde_yaml_ng::Value::as_str)
		.map(|value| {
			Version::parse(value).map_err(|error| {
				MonochangeError::Config(format!(
					"failed to parse {}: target `{package}` has invalid version `{value}`: {error}",
					changes_path.display()
				))
			})
		})
		.transpose()?;
	let change_type = mapping
		.get(serde_yaml_ng::Value::String("type".to_string()))
		.and_then(serde_yaml_ng::Value::as_str)
		.map(str::trim)
		.filter(|value| !value.is_empty())
		.map(ToString::to_string);

	if let Some(change_type) = change_type.as_deref() {
		validate_configured_change_type(configuration, changes_path, package, change_type)?;
	}

	if bump.is_none() && version.is_none() && change_type.is_none() {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: target `{package}` must declare `bump`, `version`, `type`, or a valid scalar shorthand",
			changes_path.display()
		)));
	}

	if bump == Some(BumpSeverity::None) && version.is_none() && change_type.is_none() {
		return Err(MonochangeError::Config(format!(
			"failed to parse {}: target `{package}` must not use `bump = \"none\"` without also declaring `type` or `version`",
			changes_path.display()
		)));
	}

	Ok((bump, version, change_type))
}

fn parse_bump_severity(value: &str) -> Option<BumpSeverity> {
	match value {
		"none" => Some(BumpSeverity::None),
		"major" => Some(BumpSeverity::Major),
		"minor" => Some(BumpSeverity::Minor),
		"patch" => Some(BumpSeverity::Patch),
		_ => None,
	}
}

fn validate_package_and_group_definitions(
	root: &Path,
	config_contents: &str,
	packages: &[PackageDefinition],
	groups: &[GroupDefinition],
) -> MonochangeResult<()> {
	let mut ids = BTreeSet::new();
	let mut package_paths = BTreeMap::<PathBuf, String>::new();
	let mut primary_owner = Option::<String>::None;
	for package in packages {
		if !ids.insert(package.id.clone()) {
			return Err(config_diagnostic(
				config_contents,
				format!("duplicate package id `{}`", package.id),
				vec![config_section_label(
					config_contents,
					"package",
					&package.id,
					"duplicate package id",
				)],
				Some("rename the package id so every [package.<id>] entry is unique".to_string()),
			));
		}
		let resolved_path = root.join(&package.path);
		if !resolved_path.exists() {
			return Err(config_diagnostic(
				config_contents,
				format!(
					"package `{}` path `{}` does not exist",
					package.id,
					package.path.display()
				),
				vec![config_field_label(
					config_contents,
					"package",
					&package.id,
					"path",
					"missing package path",
				)],
				Some(
					"create the package directory or update `path` to the correct package root"
						.to_string(),
				),
			));
		}
		if let Some(existing_id) = package_paths.insert(package.path.clone(), package.id.clone()) {
			return Err(config_diagnostic(
				config_contents,
				format!(
					"package path `{}` is already used by `{existing_id}`",
					package.path.display()
				),
				vec![
					config_section_label(
						config_contents,
						"package",
						&existing_id,
						"first package using this path",
					),
					config_section_label(
						config_contents,
						"package",
						&package.id,
						"conflicting package declaration",
					),
				],
				Some("declare each package path exactly once".to_string()),
			));
		}
		let expected_manifest = resolved_path.join(expected_manifest_name(package.package_type));
		if !expected_manifest.exists() {
			return Err(config_diagnostic(
				config_contents,
				format!(
					"package `{}` is missing expected {} manifest at {}",
					package.id,
					package.package_type.as_str(),
					expected_manifest.display()
				),
				vec![config_section_label(
					config_contents,
					"package",
					&package.id,
					"declared package",
				)],
				Some(format!(
					"add `{}` under `{}` or change the package type",
					expected_manifest_name(package.package_type),
					package.path.display()
				)),
			));
		}
		if package.version_format == VersionFormat::Primary {
			if let Some(existing_owner) = &primary_owner {
				return Err(config_diagnostic(
					config_contents,
					format!("`version_format = \"primary\"` is already used by `{existing_owner}`"),
					vec![
						config_primary_label(config_contents, existing_owner),
						config_primary_label(config_contents, &package.id),
					],
					Some(
						"choose a single package or group as the primary outward release identity"
							.to_string(),
					),
				));
			}
			primary_owner = Some(package.id.clone());
		}
	}

	let declared_packages = packages
		.iter()
		.map(|package| package.id.as_str())
		.collect::<BTreeSet<_>>();
	for package in packages {
		validate_versioned_files(
			root,
			config_contents,
			&package.versioned_files,
			&declared_packages,
			"package",
			&package.id,
		)?;
	}
	let mut assigned_packages = BTreeMap::<String, String>::new();
	for group in groups {
		validate_versioned_files(
			root,
			config_contents,
			&group.versioned_files,
			&declared_packages,
			"group",
			&group.id,
		)?;
		if !ids.insert(group.id.clone()) {
			return Err(config_diagnostic(
				config_contents,
				format!(
					"group `{}` collides with an existing package or group id",
					group.id
				),
				vec![config_section_label(
					config_contents,
					"group",
					&group.id,
					"conflicting group id",
				)],
				Some("package and group ids share one namespace; rename one of them".to_string()),
			));
		}
		if group.version_format == VersionFormat::Primary {
			if let Some(existing_owner) = &primary_owner {
				return Err(config_diagnostic(
					config_contents,
					format!("`version_format = \"primary\"` is already used by `{existing_owner}`"),
					vec![
						config_primary_label(config_contents, existing_owner),
						config_primary_label(config_contents, &group.id),
					],
					Some(
						"choose a single package or group as the primary outward release identity"
							.to_string(),
					),
				));
			}
			primary_owner = Some(group.id.clone());
		}
		for package_id in &group.packages {
			if !declared_packages.contains(package_id.as_str()) {
				return Err(config_diagnostic(
					config_contents,
					format!("group `{}` references unknown package `{package_id}`", group.id),
					vec![config_group_member_label(
						config_contents,
						&group.id,
						package_id,
						"unknown package reference",
					)],
					Some("declare the package first under [package.<id>] before referencing it from a group".to_string()),
				));
			}
			if let Some(existing_group) =
				assigned_packages.insert(package_id.clone(), group.id.clone())
			{
				return Err(config_diagnostic(
					config_contents,
					format!(
						"package `{package_id}` belongs to multiple groups: `{existing_group}` and `{}`",
						group.id
					),
					vec![
						config_group_member_label(
							config_contents,
							&existing_group,
							package_id,
							"first group membership",
						),
						config_group_member_label(
							config_contents,
							&group.id,
							package_id,
							"conflicting group membership",
						),
					],
					Some("move the package into exactly one [group.<id>] declaration".to_string()),
				));
			}
		}
	}

	Ok(())
}

fn path_uses_glob(path: &str) -> bool {
	path.contains('*') || path.contains('?') || path.contains('[')
}

fn path_is_supported_for_ecosystem(path: &Path, ecosystem_type: EcosystemType) -> bool {
	match ecosystem_type {
		EcosystemType::Cargo => monochange_cargo::supported_versioned_file_kind(path).is_some(),
		EcosystemType::Npm => monochange_npm::supported_versioned_file_kind(path).is_some(),
		EcosystemType::Deno => monochange_deno::supported_versioned_file_kind(path).is_some(),
		EcosystemType::Dart => monochange_dart::supported_versioned_file_kind(path).is_some(),
	}
}

fn source_capabilities(provider: SourceProvider) -> monochange_core::SourceCapabilities {
	match provider {
		SourceProvider::GitHub => monochange_github::source_capabilities(),
		SourceProvider::GitLab => monochange_gitlab::source_capabilities(),
		SourceProvider::Gitea => monochange_gitea::source_capabilities(),
	}
}

fn validate_versioned_files(
	root: &Path,
	config_contents: &str,
	versioned_files: &[VersionedFileDefinition],
	declared_packages: &BTreeSet<&str>,
	owner_kind: &str,
	owner_id: &str,
) -> MonochangeResult<()> {
	for versioned_file in versioned_files {
		if let Some(regex) = &versioned_file.regex {
			if versioned_file.ecosystem_type.is_some() {
				return Err(config_diagnostic(
					config_contents,
					format!(
						"{owner_kind} `{owner_id}` regex versioned_files cannot also set `type`"
					),
					vec![config_section_label(
						config_contents,
						owner_kind,
						owner_id,
						"regex versioned_files cannot set `type`",
					)],
					Some("remove `type` when using `regex`; regex versioned_files operate on plain text files without ecosystem-specific parsing".to_string()),
				));
			}
			if versioned_file.prefix.is_some()
				|| versioned_file.fields.is_some()
				|| versioned_file.name.is_some()
			{
				return Err(config_diagnostic(
					config_contents,
					format!(
						"{owner_kind} `{owner_id}` regex versioned_files cannot also set `prefix`, `fields`, or `name`"
					),
					vec![config_section_label(
						config_contents,
						owner_kind,
						owner_id,
						"regex versioned_files cannot mix text and dependency settings",
					)],
					Some("remove `prefix`, `fields`, and `name` when using `regex`; those options only apply to ecosystem-aware manifest updates".to_string()),
				));
			}
			let compiled = Regex::new(regex).map_err(|error| {
				config_diagnostic(
					config_contents,
					format!(
						"{owner_kind} `{owner_id}` regex versioned_files pattern `{regex}` is invalid"
					),
					vec![config_section_label(
						config_contents,
						owner_kind,
						owner_id,
						"invalid regex versioned_files pattern",
					)],
					Some(error.to_string()),
				)
			})?;
			if !compiled.capture_names().any(|name| name == Some("version")) {
				return Err(config_diagnostic(
					config_contents,
					format!(
						"{owner_kind} `{owner_id}` regex versioned_files pattern `{regex}` must include a named `version` capture"
					),
					vec![config_section_label(
						config_contents,
						owner_kind,
						owner_id,
						"regex versioned_files must capture the version",
					)],
					Some("use a named capture like `(?<version>\\d+\\.\\d+\\.\\d+)` so monochange knows which substring to replace".to_string()),
				));
			}
			continue;
		}

		let Some(ecosystem_type) = versioned_file.ecosystem_type else {
			return Err(config_diagnostic(
				config_contents,
				format!(
					"{owner_kind} `{owner_id}` versioned_files must set `type` unless they use `regex` or package-scoped shorthand"
				),
				vec![config_section_label(
					config_contents,
					owner_kind,
					owner_id,
					"versioned_files entry is missing `type`",
				)],
				Some("set `type = \"cargo\"` (or another ecosystem) for ecosystem-aware file updates, or add `regex = '...'` for plain-text replacement".to_string()),
			));
		};

		if let Some(name) = &versioned_file.name
			&& !declared_packages.contains(name.as_str())
		{
			return Err(config_diagnostic(
					config_contents,
					format!(
						"{owner_id} references unknown versioned file name `{name}`"
					),
					vec![config_dependency_label(
						config_contents,
						owner_kind,
						owner_id,
						name,
						"unknown versioned file name",
					)],
					Some("reference a declared package id from `versioned_files` or remove the name entry".to_string()),
				));
		}
		if path_uses_glob(&versioned_file.path) {
			let pattern = root
				.join(&versioned_file.path)
				.to_string_lossy()
				.to_string();
			let matches = glob::glob(&pattern)
				.map_err(|error| {
					MonochangeError::Config(format!(
						"invalid glob pattern `{}`: {error}",
						versioned_file.path
					))
				})?
				.filter_map(Result::ok)
				.collect::<Vec<_>>();
			if let Some(unsupported_path) = matches
				.into_iter()
				.find(|matched_path| !path_is_supported_for_ecosystem(matched_path, ecosystem_type))
			{
				return Err(config_diagnostic(
					config_contents,
					format!(
						"{owner_kind} `{owner_id}` versioned_files glob `{}` matched unsupported file `{}` for ecosystem `{}`",
						versioned_file.path,
						unsupported_path.display(),
						match ecosystem_type {
							EcosystemType::Cargo => "cargo",
							EcosystemType::Npm => "npm",
							EcosystemType::Deno => "deno",
							EcosystemType::Dart => "dart",
						}
					),
					vec![config_section_label(
						config_contents,
						owner_kind,
						owner_id,
						"versioned_files glob matched unsupported file type",
					)],
					Some("narrow the glob so it only matches files for that ecosystem, or change the `type` to match the files you want to update".to_string()),
				));
			}
		}
	}

	Ok(())
}

fn validate_lockfile_commands(
	root: &Path,
	ecosystem_id: &str,
	lockfile_commands: &[LockfileCommandDefinition],
) -> MonochangeResult<()> {
	for lockfile_command in lockfile_commands {
		if lockfile_command.command.trim().is_empty() {
			return Err(MonochangeError::Config(format!(
				"ecosystem `{ecosystem_id}` lockfile_commands must provide a non-empty command"
			)));
		}
		if let Some(cwd) = &lockfile_command.cwd {
			if cwd.as_os_str().is_empty() {
				return Err(MonochangeError::Config(format!(
					"ecosystem `{ecosystem_id}` lockfile_commands must provide a non-empty cwd when set"
				)));
			}
			let resolved = if cwd.is_absolute() {
				cwd.clone()
			} else {
				root.join(cwd)
			};
			if !resolved.starts_with(root) {
				return Err(MonochangeError::Config(format!(
					"ecosystem `{ecosystem_id}` lockfile_commands cwd `{}` must stay within the workspace root",
					cwd.display()
				)));
			}
			if !resolved.is_dir() {
				return Err(MonochangeError::Config(format!(
					"ecosystem `{ecosystem_id}` lockfile_commands cwd `{}` does not exist or is not a directory",
					cwd.display()
				)));
			}
		}
	}

	Ok(())
}

fn expected_manifest_name(package_type: PackageType) -> &'static str {
	match package_type {
		PackageType::Cargo => "Cargo.toml",
		PackageType::Npm => "package.json",
		PackageType::Deno => "deno.json",
		PackageType::Dart | PackageType::Flutter => "pubspec.yaml",
	}
}

fn merge_extra_changelog_sections(
	defaults: &[ExtraChangelogSection],
	specific: Vec<ExtraChangelogSection>,
) -> Vec<ExtraChangelogSection> {
	let mut sections = specific;
	sections.extend_from_slice(defaults);
	sections
}

fn validate_release_notes_configuration(
	contents: &str,
	release_notes: &RawReleaseNotesSettings,
	defaults: &[ExtraChangelogSection],
	packages: &[PackageDefinition],
	groups: &[GroupDefinition],
) -> MonochangeResult<()> {
	for template in &release_notes.change_templates {
		if template.trim().is_empty() {
			return Err(MonochangeError::Config(
				"[release_notes].change_templates must not include empty templates".to_string(),
			));
		}
		let unsupported_variables = change_template_variables(template)
			.into_iter()
			.filter(|variable| !SUPPORTED_CHANGE_TEMPLATE_VARIABLES.contains(&variable.as_str()))
			.collect::<BTreeSet<_>>();
		if !unsupported_variables.is_empty() {
			return Err(MonochangeError::Config(format!(
				"[release_notes].change_templates uses unsupported variables: {}",
				unsupported_variables
					.into_iter()
					.collect::<Vec<_>>()
					.join(", ")
			)));
		}
	}
	validate_extra_changelog_sections(contents, "defaults", "", defaults)?;
	for package in packages {
		validate_extra_changelog_sections(
			contents,
			"package",
			&package.id,
			&package.extra_changelog_sections,
		)?;
	}
	for group in groups {
		validate_extra_changelog_sections(
			contents,
			"group",
			&group.id,
			&group.extra_changelog_sections,
		)?;
	}
	Ok(())
}

fn validate_extra_changelog_sections(
	contents: &str,
	section_kind: &str,
	section_id: &str,
	extra_sections: &[ExtraChangelogSection],
) -> MonochangeResult<()> {
	let owner_label = if section_id.is_empty() {
		section_kind.to_string()
	} else {
		format!("{section_kind} `{section_id}`")
	};
	for extra_section in extra_sections {
		if extra_section.name.trim().is_empty() {
			return Err(config_diagnostic(
				contents,
				format!(
					"{owner_label} has an extra changelog section with an empty `name`"
				),
				vec![config_section_label(
					contents,
					section_kind,
					section_id,
					"extra changelog section missing name",
				)],
				Some(
					"set `extra_changelog_sections = [{ name = \"Security\", types = [\"security\"] }]` or remove the empty section definition"
						.to_string(),
				),
			));
		}
		if extra_section.types.is_empty() {
			return Err(config_diagnostic(
				contents,
				format!(
					"{owner_label} extra changelog section `{}` must declare at least one type",
					extra_section.name
				),
				vec![config_section_label(
					contents,
					section_kind,
					section_id,
					"extra changelog section missing types",
				)],
				Some(
					"add one or more `types = [\"security\"]` entries so monochange knows which changes belong in that section"
						.to_string(),
				),
			));
		}
		if extra_section
			.types
			.iter()
			.any(|change_type| change_type.trim().is_empty())
		{
			return Err(config_diagnostic(
				contents,
				format!(
					"{owner_label} extra changelog section `{}` must not include empty types",
					extra_section.name
				),
				vec![config_section_label(
					contents,
					section_kind,
					section_id,
					"extra changelog section has an empty type",
				)],
				Some(
					"remove empty values from `types` and keep only named change types".to_string(),
				),
			));
		}
	}
	Ok(())
}

fn change_template_variables(template: &str) -> Vec<String> {
	let mut variables = BTreeSet::new();
	let mut remaining = template;
	while let Some(start) = remaining.find("{{") {
		let after_open = &remaining[start + 2..];
		let Some(end) = after_open.find("}}") else {
			break;
		};
		let expression = after_open[..end].trim();
		// Extract the simple variable name (first identifier in the expression)
		let variable: String = expression
			.chars()
			.take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
			.collect();
		if !variable.is_empty() {
			variables.insert(variable);
		}
		remaining = &after_open[end + 2..];
	}
	variables.into_iter().collect()
}

fn validate_github_configuration(github: Option<&GitHubConfiguration>) -> MonochangeResult<()> {
	let Some(github) = github else {
		return Ok(());
	};
	if github.owner.trim().is_empty() {
		return Err(MonochangeError::Config(
			"[github].owner must not be empty".to_string(),
		));
	}
	if github.repo.trim().is_empty() {
		return Err(MonochangeError::Config(
			"[github].repo must not be empty".to_string(),
		));
	}
	if github.pull_requests.branch_prefix.trim().is_empty() {
		return Err(MonochangeError::Config(
			"[github.pull_requests].branch_prefix must not be empty".to_string(),
		));
	}
	if github.pull_requests.base.trim().is_empty() {
		return Err(MonochangeError::Config(
			"[github.pull_requests].base must not be empty".to_string(),
		));
	}
	if github.pull_requests.title.trim().is_empty() {
		return Err(MonochangeError::Config(
			"[github.pull_requests].title must not be empty".to_string(),
		));
	}
	if github
		.pull_requests
		.labels
		.iter()
		.any(|label| label.trim().is_empty())
	{
		return Err(MonochangeError::Config(
			"[github.pull_requests].labels must not include empty values".to_string(),
		));
	}
	if github
		.bot
		.changesets
		.skip_labels
		.iter()
		.any(|label| label.trim().is_empty())
	{
		return Err(MonochangeError::Config(
			"[github.bot.changesets].skip_labels must not include empty values".to_string(),
		));
	}
	for (field, patterns) in [
		(
			"[github.bot.changesets].changed_paths",
			&github.bot.changesets.changed_paths,
		),
		(
			"[github.bot.changesets].ignored_paths",
			&github.bot.changesets.ignored_paths,
		),
	] {
		for pattern in patterns {
			if pattern.trim().is_empty() {
				return Err(MonochangeError::Config(format!(
					"{field} must not include empty values"
				)));
			}
			Pattern::new(pattern).map_err(|error| {
				MonochangeError::Config(format!(
					"{field} contains invalid glob pattern `{pattern}`: {error}"
				))
			})?;
		}
	}
	let source = SourceConfiguration {
		provider: SourceProvider::GitHub,
		owner: github.owner.clone(),
		repo: github.repo.clone(),
		host: None,
		api_url: None,
		releases: github.releases.clone(),
		pull_requests: github.pull_requests.clone(),
		bot: github.bot.clone(),
	};
	monochange_github::validate_source_configuration(&source)
}

fn validate_source_configuration(source: Option<&SourceConfiguration>) -> MonochangeResult<()> {
	let Some(source) = source else {
		return Ok(());
	};
	if source.owner.trim().is_empty() {
		return Err(MonochangeError::Config(
			"[source].owner must not be empty".to_string(),
		));
	}
	if source.repo.trim().is_empty() {
		return Err(MonochangeError::Config(
			"[source].repo must not be empty".to_string(),
		));
	}
	if let Some(api_url) = &source.api_url {
		validate_api_url_host(api_url, source.provider)?;
	}
	if let Some(host) = &source.host {
		validate_api_url_host(host, source.provider)?;
	}
	match source.provider {
		SourceProvider::GitHub => monochange_github::validate_source_configuration(source),
		SourceProvider::GitLab => monochange_gitlab::validate_source_configuration(source),
		SourceProvider::Gitea => monochange_gitea::validate_source_configuration(source),
	}
}

/// Reject `api_url` or `host` values that use insecure schemes. API tokens are
/// sent as Authorization headers, so an `http://` endpoint would transmit them
/// in cleartext.
fn validate_api_url_host(url: &str, provider: SourceProvider) -> MonochangeResult<()> {
	let lower = url.to_lowercase();
	if lower.starts_with("http://") {
		return Err(MonochangeError::Config(format!(
			"[source] url `{url}` uses an insecure scheme (http://); \
			 API tokens would be transmitted in cleartext — use https:// instead"
		)));
	}
	// Warn about non-standard hosts for GitHub — GitLab and Gitea are commonly
	// self-hosted, so custom hosts are expected for those providers.
	if provider == SourceProvider::GitHub && lower.starts_with("https://") {
		let without_scheme = &lower["https://".len()..];
		let host_part = without_scheme.split('/').next().unwrap_or("");
		let is_standard = host_part == "api.github.com"
			|| host_part.ends_with(".github.com")
			|| host_part.ends_with(".githubusercontent.com");
		if !is_standard {
			eprintln!(
				"warning: [source] url points to non-standard GitHub host `{url}`; \
				 verify this is intentional — API tokens will be sent to this host"
			);
		}
	}
	Ok(())
}

fn validate_changesets_configuration(
	changesets: &ChangesetSettings,
	packages: &[PackageDefinition],
) -> MonochangeResult<()> {
	if changesets
		.verify
		.skip_labels
		.iter()
		.any(|label| label.trim().is_empty())
	{
		return Err(MonochangeError::Config(
			"[changesets.verify].skip_labels must not include empty values".to_string(),
		));
	}
	for package in packages {
		for (field, patterns) in [
			("ignored_paths", &package.ignored_paths),
			("additional_paths", &package.additional_paths),
		] {
			for pattern in patterns {
				if pattern.trim().is_empty() {
					return Err(MonochangeError::Config(format!(
						"[package.{}].{field} must not include empty values",
						package.id
					)));
				}
				Pattern::new(pattern).map_err(|error| {
					MonochangeError::Config(format!(
						"[package.{}].{field} contains invalid glob pattern `{pattern}`: {error}",
						package.id
					))
				})?;
			}
		}
	}
	Ok(())
}

fn validate_cli(cli: &[CliCommandDefinition]) -> MonochangeResult<()> {
	let mut seen_names = BTreeSet::new();

	for cli_command in cli {
		if !seen_names.insert(cli_command.name.clone()) {
			return Err(MonochangeError::Config(format!(
				"duplicate CLI command `{}`",
				cli_command.name
			)));
		}
		if RESERVED_CLI_COMMAND_NAMES.contains(&cli_command.name.as_str()) {
			return Err(MonochangeError::Config(format!(
				"CLI command `{}` collides with a reserved built-in command",
				cli_command.name
			)));
		}
		if cli_command.steps.is_empty() {
			return Err(MonochangeError::Config(format!(
				"CLI command `{}` must define at least one step",
				cli_command.name
			)));
		}

		let mut seen_inputs = BTreeSet::new();
		for input in &cli_command.inputs {
			if input.name.trim().is_empty() {
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` has an input with an empty name",
					cli_command.name
				)));
			}
			if !seen_inputs.insert(input.name.clone()) {
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` defines duplicate input `{}`",
					cli_command.name, input.name
				)));
			}
			if matches!(input.name.as_str(), "help" | "dry-run") {
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` input `{}` collides with an implicit command flag",
					cli_command.name, input.name
				)));
			}
			if matches!(input.kind, CliInputKind::Choice) && input.choices.is_empty() {
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` input `{}` must define at least one choice",
					cli_command.name, input.name
				)));
			}
			if let Some(default) = &input.default {
				if matches!(input.kind, CliInputKind::Choice)
					&& !input.choices.iter().any(|choice| choice == default)
				{
					return Err(MonochangeError::Config(format!(
						"CLI command `{}` input `{}` default `{default}` is not one of the configured choices",
						cli_command.name, input.name
					)));
				}
				if matches!(input.kind, CliInputKind::Boolean)
					&& default != "true"
					&& default != "false"
				{
					return Err(MonochangeError::Config(format!(
						"CLI command `{}` input `{}` boolean default must be `true` or `false`",
						cli_command.name, input.name
					)));
				}
			}
		}

		let mut seen_step_ids: BTreeSet<String> = BTreeSet::new();
		let mut seen_step_names: BTreeSet<String> = BTreeSet::new();
		for step in &cli_command.steps {
			if let Some(condition) = step.when()
				&& condition.trim().is_empty()
			{
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` step `{}` has an empty `when` condition",
					cli_command.name,
					step.kind_name()
				)));
			}
			if let Some(name) = step.name() {
				let trimmed = name.trim();
				if trimmed.is_empty() {
					return Err(MonochangeError::Config(format!(
						"CLI command `{}` step `{}` has an empty `name`",
						cli_command.name,
						step.kind_name()
					)));
				}
				if !seen_step_names.insert(trimmed.to_string()) {
					return Err(MonochangeError::Config(format!(
						"CLI command `{}` has duplicate step name `{trimmed}`",
						cli_command.name
					)));
				}
			}
			for input_name in step.inputs().keys() {
				if input_name.trim().is_empty() {
					return Err(MonochangeError::Config(format!(
						"CLI command `{}` step `{}` has an input override with an empty name",
						cli_command.name,
						step.kind_name()
					)));
				}
			}
			match step {
				CliStepDefinition::Command {
					command,
					dry_run_command,
					id,
					..
				} => {
					if let Some(step_id) = id {
						if step_id.trim().is_empty() {
							return Err(MonochangeError::Config(format!(
								"CLI command `{}` has a command step with an empty id",
								cli_command.name
							)));
						}
						if !seen_step_ids.insert(step_id.clone()) {
							return Err(MonochangeError::Config(format!(
								"CLI command `{}` has duplicate step id `{}`",
								cli_command.name, step_id
							)));
						}
					}
					if command.trim().is_empty() {
						return Err(MonochangeError::Config(format!(
							"CLI command `{}` command steps must provide a non-empty command",
							cli_command.name
						)));
					}
					if matches!(dry_run_command, Some(value) if value.trim().is_empty()) {
						return Err(MonochangeError::Config(format!(
							"CLI command `{}` command steps with `dry_run_command` must provide a non-empty command",
							cli_command.name
						)));
					}
				}
				CliStepDefinition::RenderReleaseManifest { path, .. } => {
					if matches!(path, Some(path) if path.as_os_str().is_empty()) {
						return Err(MonochangeError::Config(format!(
							"CLI command `{}` render-manifest steps must provide a non-empty path when `path` is set",
							cli_command.name
						)));
					}
				}
				CliStepDefinition::Validate { .. }
				| CliStepDefinition::Discover { .. }
				| CliStepDefinition::CreateChangeFile { .. }
				| CliStepDefinition::PrepareRelease { .. }
				| CliStepDefinition::CommitRelease { .. }
				| CliStepDefinition::PublishRelease { .. }
				| CliStepDefinition::OpenReleaseRequest { .. }
				| CliStepDefinition::CommentReleasedIssues { .. }
				| CliStepDefinition::AffectedPackages { .. }
				| CliStepDefinition::DiagnoseChangesets { .. }
				| CliStepDefinition::RetargetRelease { .. } => {}
			}
		}
	}

	Ok(())
}

fn validate_cli_runtime_requirements(
	cli: &[CliCommandDefinition],
	changesets: &ChangesetSettings,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<()> {
	for cli_command in cli {
		if cli_command
			.steps
			.iter()
			.any(|step| matches!(step, CliStepDefinition::PublishRelease { .. }))
		{
			let source = source.ok_or_else(|| {
				MonochangeError::Config(format!(
					"CLI command `{}` uses `PublishRelease` but `[source]` is not configured",
					cli_command.name
				))
			})?;
			if !source.releases.enabled {
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` uses `PublishRelease` but `[source.releases].enabled` is false",
					cli_command.name
				)));
			}
		}
		if cli_command
			.steps
			.iter()
			.any(|step| matches!(step, CliStepDefinition::OpenReleaseRequest { .. }))
		{
			let source = source.ok_or_else(|| {
				MonochangeError::Config(format!(
					"CLI command `{}` uses `OpenReleaseRequest` but `[source]` is not configured",
					cli_command.name
				))
			})?;
			if !source.pull_requests.enabled {
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` uses `OpenReleaseRequest` but `[source.pull_requests].enabled` is false",
					cli_command.name
				)));
			}
		}
		if cli_command
			.steps
			.iter()
			.any(|step| matches!(step, CliStepDefinition::CommentReleasedIssues { .. }))
		{
			let source = source.ok_or_else(|| {
				MonochangeError::Config(format!(
					"CLI command `{}` uses `CommentReleasedIssues` but `[source]` is not configured",
					cli_command.name
				))
			})?;
			if !source_capabilities(source.provider).released_issue_comments {
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` uses `CommentReleasedIssues` but `[source].provider = \"{}\"` does not support released-issue comments",
					cli_command.name, source.provider
				)));
			}
		}
		for step in &cli_command.steps {
			validate_step_input_overrides(cli_command, step)?;
			if let CliStepDefinition::AffectedPackages { inputs, .. } = step {
				if !changesets.verify.enabled {
					return Err(MonochangeError::Config(format!(
						"CLI command `{}` uses `AffectedPackages` but `[changesets.verify].enabled` is false",
						cli_command.name
					)));
				}
				let has_changed_paths = inputs.contains_key("changed_paths")
					|| cli_command_input(cli_command, "changed_paths")
						.is_some_and(|input| matches!(input.kind, CliInputKind::StringList));
				let has_since = inputs.contains_key("since")
					|| cli_command_input(cli_command, "since").is_some();
				if !has_changed_paths && !has_since {
					return Err(MonochangeError::Config(format!(
						"CLI command `{}` uses `AffectedPackages` but declares neither a `changed_paths` nor a `since` input and does not override either on the step",
						cli_command.name
					)));
				}
				if let Some(label_input) = cli_command_input(cli_command, "label")
					&& !matches!(label_input.kind, CliInputKind::StringList)
				{
					return Err(MonochangeError::Config(format!(
						"CLI command `{}` input `label` must use type `string_list` when used with `AffectedPackages`",
						cli_command.name
					)));
				}
				validate_step_override_kind(
					cli_command,
					step,
					"changed_paths",
					inputs.get("changed_paths"),
					false,
				)?;
				validate_step_override_kind(
					cli_command,
					step,
					"since",
					inputs.get("since"),
					false,
				)?;
				validate_step_override_kind(
					cli_command,
					step,
					"label",
					inputs.get("label"),
					false,
				)?;
				validate_step_override_kind(
					cli_command,
					step,
					"verify",
					inputs.get("verify"),
					true,
				)?;
			}
		}
	}

	Ok(())
}

fn cli_command_input<'a>(
	cli_command: &'a CliCommandDefinition,
	name: &str,
) -> Option<&'a CliInputDefinition> {
	cli_command.inputs.iter().find(|input| input.name == name)
}

fn validate_step_override_kind(
	cli_command: &CliCommandDefinition,
	step: &CliStepDefinition,
	input_name: &str,
	value: Option<&CliStepInputValue>,
	expect_boolean: bool,
) -> MonochangeResult<()> {
	let Some(value) = value else {
		return Ok(());
	};
	let valid = if expect_boolean {
		matches!(
			value,
			CliStepInputValue::Boolean(_) | CliStepInputValue::String(_)
		)
	} else {
		matches!(
			value,
			CliStepInputValue::String(_) | CliStepInputValue::List(_)
		)
	};
	if valid {
		return Ok(());
	}
	Err(MonochangeError::Config(format!(
		"CLI command `{}` step `{}` override `{}` must use a {} value",
		cli_command.name,
		step.kind_name(),
		input_name,
		if expect_boolean {
			"boolean or string template"
		} else {
			"string or string_list value"
		}
	)))
}

/// Validate that every input override key on a step is recognised and that
/// its value type matches the expected [`CliInputKind`].
fn validate_step_input_overrides(
	cli_command: &CliCommandDefinition,
	step: &CliStepDefinition,
) -> MonochangeResult<()> {
	let overrides = step.inputs();
	if overrides.is_empty() {
		return Ok(());
	}

	let valid_names = step.valid_input_names();

	for (name, value) in overrides {
		// Reject unknown input names (Command steps accept anything).
		if let Some(names) = valid_names
			&& !names.contains(&name.as_str())
		{
			let available = if names.is_empty() {
				"this step accepts no inputs".to_string()
			} else {
				format!("valid inputs: {}", names.join(", "))
			};
			return Err(MonochangeError::Config(format!(
				"CLI command `{}` step `{}` has unknown input override `{}`; {}",
				cli_command.name,
				step.kind_name(),
				name,
				available,
			)));
		}

		// Validate value type against expected kind.
		if let Some(expected_kind) = step.expected_input_kind(name) {
			let type_ok = match expected_kind {
				CliInputKind::Boolean => {
					matches!(
						value,
						CliStepInputValue::Boolean(_) | CliStepInputValue::String(_)
					)
				}
				CliInputKind::StringList => {
					matches!(
						value,
						CliStepInputValue::String(_) | CliStepInputValue::List(_)
					)
				}
				CliInputKind::String | CliInputKind::Path | CliInputKind::Choice => {
					matches!(value, CliStepInputValue::String(_))
				}
			};
			if !type_ok {
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` step `{}` override `{}` must use a {} value",
					cli_command.name,
					step.kind_name(),
					name,
					match expected_kind {
						CliInputKind::Boolean => "boolean or string template",
						CliInputKind::StringList => "string or string_list",
						CliInputKind::String | CliInputKind::Path | CliInputKind::Choice =>
							"string",
					}
				)));
			}
		}
	}

	Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn config_diagnostic(
	config_contents: &str,
	message: String,
	labels: Vec<LabeledSpan>,
	help: Option<String>,
) -> MonochangeError {
	let report = Report::new(SourceDiagnostic {
		message: message.clone(),
		source_code: NamedSource::new(CONFIG_FILE, config_contents.to_string()),
		labels: labels.clone(),
		help: help.clone(),
	});
	let _ = report;
	MonochangeError::Diagnostic(render_source_diagnostic(
		CONFIG_FILE,
		&message,
		&labels,
		help.as_deref(),
	))
}

fn config_section_label(
	config_contents: &str,
	kind: &str,
	id: &str,
	label: &'static str,
) -> LabeledSpan {
	let span = find_section_header_range(config_contents, kind, id).unwrap_or(0..0);
	LabeledSpan::new_with_span(Some(label.to_string()), range_to_span(span))
}

fn config_field_label(
	config_contents: &str,
	kind: &str,
	id: &str,
	field: &str,
	label: &'static str,
) -> LabeledSpan {
	let span = find_section_field_range(config_contents, kind, id, field)
		.or_else(|| find_section_header_range(config_contents, kind, id))
		.unwrap_or(0..0);
	LabeledSpan::new_with_span(Some(label.to_string()), range_to_span(span))
}

fn config_group_member_label(
	config_contents: &str,
	group_id: &str,
	member_id: &str,
	label: &'static str,
) -> LabeledSpan {
	let span = find_group_member_range(config_contents, group_id, member_id)
		.or_else(|| find_section_header_range(config_contents, "group", group_id))
		.unwrap_or(0..0);
	LabeledSpan::new_with_span(Some(label.to_string()), range_to_span(span))
}

fn config_dependency_label(
	config_contents: &str,
	owner_kind: &str,
	owner_id: &str,
	dependency: &str,
	label: &'static str,
) -> LabeledSpan {
	let span = find_dependency_range(config_contents, owner_kind, owner_id, dependency)
		.or_else(|| find_section_header_range(config_contents, owner_kind, owner_id))
		.unwrap_or(0..0);
	LabeledSpan::new_with_span(Some(label.to_string()), range_to_span(span))
}

fn config_primary_label(config_contents: &str, owner_id: &str) -> LabeledSpan {
	let span = find_section_field_range(config_contents, "package", owner_id, "version_format")
		.or_else(|| find_section_field_range(config_contents, "group", owner_id, "version_format"))
		.or_else(|| find_section_header_range(config_contents, "package", owner_id))
		.or_else(|| find_section_header_range(config_contents, "group", owner_id))
		.unwrap_or(0..0);
	LabeledSpan::new_with_span(
		Some("primary release identity".to_string()),
		range_to_span(span),
	)
}

fn render_source_diagnostic(
	source_name: &str,
	message: &str,
	labels: &[LabeledSpan],
	help: Option<&str>,
) -> String {
	let mut lines = vec![format!("error: {message}"), format!("--> {source_name}")];
	if !labels.is_empty() {
		lines.push("labels:".to_string());
		for label in labels {
			let label_text = label.label().unwrap_or("source");
			let end = label.offset().saturating_add(label.len());
			lines.push(format!("- {label_text} @ bytes {}..{end}", label.offset()));
		}
	}
	if let Some(help) = help {
		lines.push(format!("help: {help}"));
	}
	lines.join("\n")
}

fn range_to_span(range: Range<usize>) -> SourceSpan {
	(range.start, range.end.saturating_sub(range.start)).into()
}

fn find_section_header_range(config_contents: &str, kind: &str, id: &str) -> Option<Range<usize>> {
	section_patterns(kind, id).into_iter().find_map(|pattern| {
		config_contents
			.find(&pattern)
			.map(|start| start..start + pattern.len())
	})
}

fn find_section_field_range(
	config_contents: &str,
	kind: &str,
	id: &str,
	field: &str,
) -> Option<Range<usize>> {
	let section = find_section_range(config_contents, kind, id)?;
	let needle = format!("{field} =");
	config_contents[section.clone()]
		.find(&needle)
		.map(|offset| section.start + offset..section.start + offset + needle.len())
}

fn find_group_member_range(
	config_contents: &str,
	group_id: &str,
	member_id: &str,
) -> Option<Range<usize>> {
	let section = find_section_range(config_contents, "group", group_id)?;
	let needle = format!("\"{member_id}\"");
	config_contents[section.clone()]
		.find(&needle)
		.map(|offset| section.start + offset..section.start + offset + needle.len())
}

fn find_dependency_range(
	config_contents: &str,
	owner_kind: &str,
	owner_id: &str,
	dependency: &str,
) -> Option<Range<usize>> {
	let section = find_section_range(config_contents, owner_kind, owner_id)?;
	let needle = format!("dependency = \"{dependency}\"");
	config_contents[section.clone()]
		.find(&needle)
		.map(|offset| section.start + offset..section.start + offset + needle.len())
}

fn find_section_range(config_contents: &str, kind: &str, id: &str) -> Option<Range<usize>> {
	section_patterns(kind, id).into_iter().find_map(|pattern| {
		config_contents.find(&pattern).map(|start| {
			let rest = &config_contents[start + pattern.len()..];
			let end = rest.find("\n[").map_or(config_contents.len(), |offset| {
				start + pattern.len() + offset + 1
			});
			start..end
		})
	})
}

fn section_patterns(kind: &str, id: &str) -> [String; 2] {
	if id.is_empty() {
		return [format!("[{kind}]"), format!("[{kind}]")];
	}
	[format!("[{kind}.{id}]"), format!("[{kind}.\"{id}\"]")]
}

#[allow(clippy::needless_pass_by_value)]
fn changeset_diagnostic(
	contents: &str,
	changeset_path: &Path,
	message: String,
	labels: Vec<LabeledSpan>,
	help: Option<String>,
) -> MonochangeError {
	let source_name = changeset_path.display().to_string();
	let report = Report::new(SourceDiagnostic {
		message: message.clone(),
		source_code: NamedSource::new(source_name.clone(), contents.to_string()),
		labels: labels.clone(),
		help: help.clone(),
	});
	let _ = report;
	MonochangeError::Diagnostic(render_source_diagnostic(
		&source_name,
		&message,
		&labels,
		help.as_deref(),
	))
}

fn changeset_key_label(contents: &str, key: &str, label: &'static str) -> LabeledSpan {
	let span = find_changeset_key_range(contents, key).unwrap_or(0..0);
	LabeledSpan::new_with_span(Some(label.to_string()), range_to_span(span))
}

fn find_changeset_key_range(contents: &str, key: &str) -> Option<Range<usize>> {
	let frontmatter = extract_frontmatter(contents)?;
	let needle = format!("{key}:");
	frontmatter
		.1
		.find(&needle)
		.map(|offset| frontmatter.0.start + offset..frontmatter.0.start + offset + needle.len())
}

fn extract_frontmatter(contents: &str) -> Option<(Range<usize>, &str)> {
	let without_opening = contents.strip_prefix("---")?;
	let (frontmatter, _) = without_opening.split_once("\n---\n")?;
	let start = 4;
	Some((start..start + frontmatter.len(), frontmatter))
}

pub fn apply_version_groups(
	packages: &mut [PackageRecord],
	configuration: &WorkspaceConfiguration,
) -> MonochangeResult<(Vec<VersionGroup>, Vec<String>)> {
	let mut warnings = Vec::new();
	let mut assigned = BTreeMap::<String, String>::new();
	let mut groups = Vec::new();
	let config_packages_by_id = configuration
		.packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<BTreeMap<_, _>>();
	for package_definition in &configuration.packages {
		for package_index in find_matching_package_indices_for_definition(
			packages,
			&configuration.root_path,
			package_definition,
		) {
			if let Some(package) = packages.get_mut(package_index) {
				package
					.metadata
					.insert("config_id".to_string(), package_definition.id.clone());
			}
		}
	}

	for group in &configuration.groups {
		let group_id = group.id.clone();
		let group_members = group.packages.clone();
		let mut members = Vec::new();
		let mut versions = BTreeSet::new();

		for member in &group_members {
			let matching_indices =
				if let Some(package_definition) = config_packages_by_id.get(member.as_str()) {
					find_matching_package_indices_for_definition(
						packages,
						&configuration.root_path,
						package_definition,
					)
				} else {
					find_matching_package_indices(packages, &configuration.root_path, member)
				};

			if matching_indices.is_empty() {
				warnings.push(format!(
					"version group `{group_id}` member `{member}` did not match any discovered package"
				));
				continue;
			}

			for package_index in matching_indices {
				let package = packages.get_mut(package_index).ok_or_else(|| {
					MonochangeError::Config(format!(
						"matched package index `{package_index}` for version group `{group_id}` is invalid"
					))
				})?;

				if let Some(existing_group) = assigned.get(&package.id) {
					return Err(MonochangeError::Config(format!(
						"package `{}` belongs to conflicting version groups `{existing_group}` and `{group_id}`",
						package.id
					)));
				}

				assigned.insert(package.id.clone(), group_id.clone());
				package.version_group_id = Some(group_id.clone());
				members.push(package.id.clone());

				if let Some(version) = &package.current_version {
					versions.insert(version.to_string());
				}
			}
		}

		let mismatch_detected = versions.len() > 1;
		if mismatch_detected && configuration.defaults.warn_on_group_mismatch {
			warnings.push(format!(
				"version group `{group_id}` contains packages with mismatched versions"
			));
		}

		groups.push(VersionGroup {
			group_id: group_id.clone(),
			display_name: group_id,
			members,
			mismatch_detected,
		});
	}

	Ok((groups, warnings))
}

fn find_matching_package_indices(
	packages: &[PackageRecord],
	root: &Path,
	member: &str,
) -> Vec<usize> {
	packages
		.iter()
		.enumerate()
		.filter_map(|(index, package)| {
			if package_matches_reference(package, root, member) {
				Some(index)
			} else {
				None
			}
		})
		.collect()
}

fn find_matching_package_indices_for_definition(
	packages: &[PackageRecord],
	root: &Path,
	definition: &PackageDefinition,
) -> Vec<usize> {
	packages
		.iter()
		.enumerate()
		.filter_map(|(index, package)| {
			if package_matches_definition(package, root, definition) {
				Some(index)
			} else {
				None
			}
		})
		.collect()
}

fn find_matching_package_ids(
	reference: &str,
	root: &Path,
	packages: &[PackageRecord],
) -> Vec<String> {
	packages
		.iter()
		.filter(|package| package_matches_reference(package, root, reference))
		.map(|package| package.id.clone())
		.collect()
}

fn package_matches_reference(package: &PackageRecord, root: &Path, reference: &str) -> bool {
	let manifest_match = relative_to_root(root, &package.manifest_path)
		.and_then(|path| path.to_str().map(ToString::to_string))
		.is_some_and(|path| path == reference);
	let directory_match = package
		.manifest_path
		.parent()
		.and_then(|path| relative_to_root(root, path))
		.and_then(|path| path.to_str().map(ToString::to_string))
		.is_some_and(|path| path == reference);
	let name_match = package.name == reference;
	let id_match = package.id == reference;
	let config_id_match = package
		.metadata
		.get("config_id")
		.is_some_and(|config_id| config_id == reference);

	manifest_match || directory_match || name_match || id_match || config_id_match
}

fn package_matches_definition(
	package: &PackageRecord,
	root: &Path,
	definition: &PackageDefinition,
) -> bool {
	let Some(directory) = package.manifest_path.parent() else {
		return false;
	};
	let relative_directory = relative_to_root(root, directory);
	relative_directory.as_deref() == Some(definition.path.as_path())
		&& ecosystem_matches_package_type(package.ecosystem, definition.package_type)
}

fn ecosystem_matches_package_type(ecosystem: Ecosystem, package_type: PackageType) -> bool {
	matches!(
		(ecosystem, package_type),
		(Ecosystem::Cargo, PackageType::Cargo)
			| (Ecosystem::Npm, PackageType::Npm)
			| (Ecosystem::Deno, PackageType::Deno)
			| (Ecosystem::Dart, PackageType::Dart)
			| (Ecosystem::Flutter, PackageType::Flutter)
	)
}

pub fn validate_workspace(root: &Path) -> MonochangeResult<()> {
	let configuration = load_workspace_configuration(root)?;
	let changeset_dir = root.join(".changeset");
	if !changeset_dir.exists() {
		return Ok(());
	}

	let changeset_paths = fs::read_dir(&changeset_dir)
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read {}: {error}",
				changeset_dir.display()
			))
		})?
		.filter_map(Result::ok)
		.map(|entry| entry.path())
		.filter(|path| path.extension().and_then(|value| value.to_str()) == Some("md"))
		.collect::<Vec<_>>();
	for changeset_path in changeset_paths {
		validate_changeset_targets(&configuration, &changeset_path)?;
	}

	Ok(())
}

/// Validate that versioned file paths exist on disk, ecosystem-typed files
/// contain a readable version field, and regex patterns match actual file
/// content. Returns a list of non-fatal warnings (e.g. empty glob matches).
///
/// This is separate from the structural validation in
/// `validate_versioned_files()` because it performs file I/O and should only
/// run during the explicit `mc validate` command, not on every config load.
pub fn validate_versioned_files_content(root: &Path) -> MonochangeResult<Vec<String>> {
	let configuration = load_workspace_configuration(root)?;
	let mut warnings = Vec::new();

	// Collect all (owner_kind, owner_id, definitions) triples.
	let mut sources: Vec<(&str, String, &[VersionedFileDefinition])> = Vec::new();

	for package in &configuration.packages {
		sources.push(("package", package.id.clone(), &package.versioned_files));
	}
	for group in &configuration.groups {
		sources.push(("group", group.id.clone(), &group.versioned_files));
	}

	let ecosystem_entries: &[(&str, &EcosystemSettings)] = &[
		("cargo", &configuration.cargo),
		("npm", &configuration.npm),
		("deno", &configuration.deno),
		("dart", &configuration.dart),
	];
	for &(eco_name, settings) in ecosystem_entries {
		if !settings.versioned_files.is_empty() {
			sources.push(("ecosystem", eco_name.to_string(), &settings.versioned_files));
		}
	}

	for (owner_kind, owner_id, definitions) in &sources {
		for definition in *definitions {
			validate_single_versioned_file_content(
				root,
				definition,
				owner_kind,
				owner_id,
				&mut warnings,
			)?;
		}
	}

	Ok(warnings)
}

fn validate_single_versioned_file_content(
	root: &Path,
	definition: &VersionedFileDefinition,
	owner_kind: &str,
	owner_id: &str,
	warnings: &mut Vec<String>,
) -> MonochangeResult<()> {
	if path_uses_glob(&definition.path) {
		// Glob path: warn if zero files match.
		let pattern = root.join(&definition.path).to_string_lossy().to_string();
		let matches = glob::glob(&pattern)
			.map_err(|error| {
				MonochangeError::Config(format!(
					"invalid glob pattern `{}`: {error}",
					definition.path
				))
			})?
			.filter_map(Result::ok)
			.collect::<Vec<_>>();
		if matches.is_empty() {
			warnings.push(format!(
				"{owner_kind} `{owner_id}` versioned file glob `{}` matches no files",
				definition.path
			));
		}
		// For globs we skip per-file content validation since the set of
		// matched files may change between validation and release time.
		return Ok(());
	}

	let full_path = root.join(&definition.path);
	if !full_path.exists() {
		return Err(MonochangeError::Config(format!(
			"{owner_kind} `{owner_id}` versioned file `{}` does not exist",
			definition.path
		)));
	}

	if let Some(regex_pattern) = &definition.regex {
		// Regex versioned file: verify pattern matches file content.
		let contents = fs::read_to_string(&full_path).map_err(|error| {
			MonochangeError::Io(format!("failed to read `{}`: {error}", definition.path))
		})?;
		let compiled = Regex::new(regex_pattern).map_err(|error| {
			MonochangeError::Config(format!(
				"{owner_kind} `{owner_id}` regex `{regex_pattern}` is invalid: {error}"
			))
		})?;
		if !compiled.is_match(&contents) {
			return Err(MonochangeError::Config(format!(
				"{owner_kind} `{owner_id}` versioned file `{}` regex `{regex_pattern}` does not match any content in the file",
				definition.path
			)));
		}
		return Ok(());
	}

	if let Some(ecosystem_type) = definition.ecosystem_type {
		// Ecosystem-typed versioned file: verify version field is readable.
		validate_ecosystem_version_readable(
			&full_path,
			&definition.path,
			ecosystem_type,
			definition.fields.as_deref(),
			owner_kind,
			owner_id,
		)?;
	}

	Ok(())
}

fn validate_ecosystem_version_readable(
	full_path: &Path,
	display_path: &str,
	ecosystem_type: EcosystemType,
	fields: Option<&[String]>,
	owner_kind: &str,
	owner_id: &str,
) -> MonochangeResult<()> {
	let contents = fs::read_to_string(full_path).map_err(|error| {
		MonochangeError::Io(format!("failed to read `{display_path}`: {error}"))
	})?;

	match ecosystem_type {
		EcosystemType::Cargo => {
			let doc: toml::Value = toml::from_str(&contents).map_err(|error| {
				MonochangeError::Config(format!(
					"{owner_kind} `{owner_id}` versioned file `{display_path}` is not valid TOML: {error}"
				))
			})?;

			// Check the configured fields, or fall back to common Cargo version
			// paths including a bare root-level `version` (used by custom TOML
			// files like group.toml).
			let field_paths: Vec<&str> = match fields {
				Some(f) if !f.is_empty() => f.iter().map(String::as_str).collect(),
				_ => vec!["package.version", "workspace.package.version", "version"],
			};

			let found = field_paths.iter().any(|field_path| {
				let parts: Vec<&str> = field_path.split('.').collect();
				let mut current = &doc;
				for part in &parts {
					match current.get(part) {
						Some(next) => current = next,
						None => return false,
					}
				}
				current.is_str()
			});

			if !found {
				return Err(MonochangeError::Config(format!(
					"{owner_kind} `{owner_id}` versioned file `{display_path}` does not contain a readable version field (checked: {})",
					field_paths.join(", ")
				)));
			}
		}
		EcosystemType::Npm | EcosystemType::Deno => {
			let json: serde_json::Value = serde_json::from_str(&contents).map_err(|error| {
				MonochangeError::Config(format!(
					"{owner_kind} `{owner_id}` versioned file `{display_path}` is not valid JSON: {error}"
				))
			})?;

			let field_name = match fields {
				Some(f) if !f.is_empty() => f.first().map_or("version", String::as_str),
				_ => "version",
			};

			if json.get(field_name).and_then(|v| v.as_str()).is_none() {
				return Err(MonochangeError::Config(format!(
					"{owner_kind} `{owner_id}` versioned file `{display_path}` does not contain a `{field_name}` string field"
				)));
			}
		}
		EcosystemType::Dart => {
			let yaml: serde_yaml_ng::Value =
				serde_yaml_ng::from_str(&contents).map_err(|error| {
					MonochangeError::Config(format!(
						"{owner_kind} `{owner_id}` versioned file `{display_path}` is not valid YAML: {error}"
					))
				})?;

			if yaml.get("version").and_then(|v| v.as_str()).is_none() {
				return Err(MonochangeError::Config(format!(
					"{owner_kind} `{owner_id}` versioned file `{display_path}` does not contain a `version` string field"
				)));
			}
		}
	}

	Ok(())
}

fn validate_changeset_targets(
	configuration: &WorkspaceConfiguration,
	changeset_path: &Path,
) -> MonochangeResult<()> {
	let contents = fs::read_to_string(changeset_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			changeset_path.display()
		))
	})?;
	let raw = if changeset_path.extension().and_then(|value| value.to_str()) == Some("md") {
		parse_markdown_change_file(&contents, changeset_path, configuration)?
	} else {
		return Ok(());
	};
	let package_ids = configuration
		.packages
		.iter()
		.map(|package| package.id.as_str())
		.collect::<BTreeSet<_>>();
	let group_members = configuration
		.groups
		.iter()
		.map(|group| {
			(
				group.id.as_str(),
				group
					.packages
					.iter()
					.map(String::as_str)
					.collect::<BTreeSet<_>>(),
			)
		})
		.collect::<BTreeMap<_, _>>();

	for change in &raw.changes {
		if !package_ids.contains(change.package.as_str())
			&& !group_members.contains_key(change.package.as_str())
		{
			return Err(changeset_diagnostic(
				&contents,
				changeset_path,
				format!(
					"changeset `{}` references unknown package or group `{}`",
					changeset_path.display(),
					change.package,
				),
				vec![changeset_key_label(
					&contents,
					&change.package,
					"unknown package or group",
				)],
				Some("declare the package or group id in monochange.toml before referencing it in a changeset".to_string()),
			));
		}
	}

	Ok(())
}

#[cfg(test)]
mod __tests;
