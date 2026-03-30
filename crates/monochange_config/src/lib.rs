#![allow(unused_assignments)]
#![deny(clippy::all)]
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
//! - parse change-input files, evidence, release-note `type` / `details` fields, changelog paths, changelog format overrides, GitHub release config, GitHub changeset-bot policy config, and command GitHub/manifest/policy steps
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
//! path = "{path}/CHANGELOG.md"
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
use monochange_core::default_cli_commands;
use monochange_core::relative_to_root;
use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::ChangelogDefinition;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogTarget;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputDefinition;
use monochange_core::CliInputKind;
use monochange_core::CliStepDefinition;
use monochange_core::DeploymentDefinition;
use monochange_core::Ecosystem;
use monochange_core::EcosystemSettings;
use monochange_core::ExtraChangelogSection;
use monochange_core::GitHubBotSettings;
use monochange_core::GitHubChangesetBotSettings;
use monochange_core::GitHubConfiguration;
use monochange_core::GitHubPullRequestSettings;
use monochange_core::GitHubReleaseNotesSource;
use monochange_core::GitHubReleaseSettings;
use monochange_core::GroupDefinition;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageDefinition;
use monochange_core::PackageRecord;
use monochange_core::PackageType;
use monochange_core::ReleaseNotesSettings;
use monochange_core::VersionFormat;
use monochange_core::VersionGroup;
use monochange_core::VersionedFileDefinition;
use monochange_core::WorkspaceConfiguration;
use monochange_core::WorkspaceDefaults;
use serde::Deserialize;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value as YamlValue;

const CONFIG_FILE: &str = "monochange.toml";
const RESERVED_CLI_COMMAND_NAMES: &[&str] = &["init", "help", "version"];
const SUPPORTED_CHANGE_TEMPLATE_VARIABLES: &[&str] = &[
	"summary",
	"details",
	"package",
	"version",
	"target_id",
	"bump",
	"type",
];

#[derive(Debug, Deserialize, Default)]
struct RawWorkspaceConfiguration {
	#[serde(default)]
	defaults: RawWorkspaceDefaults,
	#[serde(default)]
	release_notes: RawReleaseNotesSettings,
	#[serde(default)]
	deployments: Vec<DeploymentDefinition>,
	#[serde(default)]
	package: BTreeMap<String, RawPackageDefinition>,
	#[serde(default)]
	group: BTreeMap<String, RawGroupDefinition>,
	#[serde(default)]
	cli: BTreeMap<String, RawCliCommandDefinition>,
	#[serde(default)]
	workflows: Vec<CliCommandDefinition>,
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
	package_type: Option<PackageType>,
	#[serde(default)]
	changelog: Option<RawChangelogConfig>,
	#[serde(default)]
	empty_update_message: Option<String>,
}

impl Default for RawWorkspaceDefaults {
	fn default() -> Self {
		Self {
			parent_bump: default_parent_bump(),
			include_private: false,
			warn_on_group_mismatch: default_warn_on_group_mismatch(),
			package_type: None,
			changelog: None,
			empty_update_message: None,
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

#[derive(Debug, Clone, Deserialize, Default)]
struct RawChangelogTable {
	#[serde(default)]
	enabled: Option<bool>,
	#[serde(default)]
	path: Option<String>,
	#[serde(default)]
	format: Option<ChangelogFormat>,
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
	versioned_files: Vec<VersionedFileDefinition>,
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
	versioned_files: Vec<VersionedFileDefinition>,
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
	cargo: EcosystemSettings,
	#[serde(default)]
	npm: EcosystemSettings,
	#[serde(default)]
	deno: EcosystemSettings,
	#[serde(default)]
	dart: EcosystemSettings,
}

#[derive(Debug, Deserialize, Default)]
struct RawReleaseNotesSettings {
	#[serde(default)]
	change_templates: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawGitHubConfiguration {
	owner: String,
	repo: String,
	#[serde(default)]
	releases: RawGitHubReleaseSettings,
	#[serde(default)]
	pull_requests: RawGitHubPullRequestSettings,
	#[serde(default)]
	bot: RawGitHubBotSettings,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Deserialize)]
struct RawGitHubReleaseSettings {
	#[serde(default = "default_true")]
	enabled: bool,
	#[serde(default)]
	draft: bool,
	#[serde(default)]
	prerelease: bool,
	#[serde(default)]
	generate_notes: bool,
	#[serde(default)]
	source: GitHubReleaseNotesSource,
}

impl Default for RawGitHubReleaseSettings {
	fn default() -> Self {
		Self {
			enabled: default_true(),
			draft: false,
			prerelease: false,
			generate_notes: false,
			source: GitHubReleaseNotesSource::Monochange,
		}
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Deserialize)]
struct RawGitHubPullRequestSettings {
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

impl Default for RawGitHubPullRequestSettings {
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
struct RawGitHubChangesetBotSettings {
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

impl Default for RawGitHubChangesetBotSettings {
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
struct RawGitHubBotSettings {
	#[serde(default)]
	changesets: RawGitHubChangesetBotSettings,
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
	reason: Option<String>,
	#[serde(default)]
	details: Option<String>,
	#[serde(rename = "type", default)]
	change_type: Option<String>,
	#[serde(default = "default_change_origin")]
	origin: String,
	#[serde(default)]
	evidence: Vec<String>,
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

fn default_change_origin() -> String {
	"direct-change".to_string()
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

impl RawChangelogConfig {
	fn as_defaults_definition(&self) -> ChangelogDefinition {
		match self {
			Self::Legacy(definition) => match definition {
				RawChangelogDefinition::Enabled(false) => ChangelogDefinition::Disabled,
				RawChangelogDefinition::Enabled(true) => ChangelogDefinition::PackageDefault,
				RawChangelogDefinition::Path(path_pattern) => {
					ChangelogDefinition::PathPattern(path_pattern.clone())
				}
			},
			Self::Detailed(table) => match (table.enabled.unwrap_or(true), &table.path) {
				(false, _) => ChangelogDefinition::Disabled,
				(true, Some(path_pattern)) => {
					ChangelogDefinition::PathPattern(path_pattern.clone())
				}
				(true, None) => ChangelogDefinition::PackageDefault,
			},
		}
	}

	fn format(&self) -> Option<ChangelogFormat> {
		match self {
			Self::Legacy(_) => None,
			Self::Detailed(table) => table.format,
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
			Self::Legacy(definition) => match definition {
				RawChangelogDefinition::Enabled(false) => None,
				RawChangelogDefinition::Enabled(true) => Some(package_path.join("CHANGELOG.md")),
				RawChangelogDefinition::Path(path) => {
					if treat_string_as_pattern {
						let package_path = package_path.to_string_lossy();
						Some(PathBuf::from(path.replace("{path}", &package_path)))
					} else {
						Some(PathBuf::from(path))
					}
				}
			},
			Self::Detailed(table) => {
				if matches!(table.enabled, Some(false)) {
					return None;
				}
				match &table.path {
					Some(path) => {
						if treat_string_as_pattern {
							let package_path = package_path.to_string_lossy();
							Some(PathBuf::from(path.replace("{path}", &package_path)))
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
			Self::Legacy(definition) => match definition {
				RawChangelogDefinition::Enabled(false | true) => None,
				RawChangelogDefinition::Path(path) => Some(PathBuf::from(path)),
			},
			Self::Detailed(table) => {
				if matches!(table.enabled, Some(false)) {
					return None;
				}
				table.path.as_ref().map(PathBuf::from)
			}
		}
	}
}

#[must_use]
pub fn config_path(root: &Path) -> PathBuf {
	root.join(CONFIG_FILE)
}

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
		deployments,
		package,
		group,
		cli,
		workflows,
		github,
		ecosystems,
	} = raw;
	if !workflows.is_empty() {
		return Err(MonochangeError::Config(
			"legacy `[[workflows]]` configuration is no longer supported; use `[cli.<command>]` with `[[cli.<command>.steps]]` instead".to_string(),
		));
	}
	let cli = if cli.is_empty() {
		default_cli_commands()
	} else {
		cli.into_iter()
			.map(|(name, definition)| CliCommandDefinition {
				name,
				help_text: definition.help_text,
				inputs: definition.inputs,
				steps: definition.steps,
			})
			.collect::<Vec<_>>()
	};
	let default_package_type = defaults.package_type;
	let default_package_changelog = defaults.changelog.clone();
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
			Ok::<_, MonochangeError>(PackageDefinition {
				id,
				path: package.path,
				package_type,
				changelog,
				extra_changelog_sections: package.extra_changelog_sections,
				empty_update_message: package.empty_update_message,
				versioned_files: package.versioned_files,
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
			Ok::<_, MonochangeError>(GroupDefinition {
				id,
				packages: group.packages,
				changelog,
				extra_changelog_sections: group.extra_changelog_sections,
				empty_update_message: group.empty_update_message,
				versioned_files: group.versioned_files,
				tag: group.tag,
				release: group.release,
				version_format: group.version_format,
			})
		})
		.collect::<Result<Vec<_>, _>>()?;
	let github = github.map(|github| GitHubConfiguration {
		owner: github.owner,
		repo: github.repo,
		releases: GitHubReleaseSettings {
			enabled: github.releases.enabled,
			draft: github.releases.draft,
			prerelease: github.releases.prerelease,
			generate_notes: github.releases.generate_notes,
			source: github.releases.source,
		},
		pull_requests: GitHubPullRequestSettings {
			enabled: github.pull_requests.enabled,
			branch_prefix: github.pull_requests.branch_prefix,
			base: github.pull_requests.base,
			title: github.pull_requests.title,
			labels: github.pull_requests.labels,
			auto_merge: github.pull_requests.auto_merge,
		},
		bot: GitHubBotSettings {
			changesets: GitHubChangesetBotSettings {
				enabled: github.bot.changesets.enabled,
				required: github.bot.changesets.required,
				skip_labels: github.bot.changesets.skip_labels,
				comment_on_failure: github.bot.changesets.comment_on_failure,
				changed_paths: github.bot.changesets.changed_paths,
				ignored_paths: github.bot.changesets.ignored_paths,
			},
		},
	});

	validate_cli(&cli)?;
	validate_release_notes_configuration(&contents, &release_notes, &packages, &groups)?;
	validate_deployments_configuration(&contents, &deployments)?;
	validate_github_configuration(github.as_ref())?;
	validate_package_and_group_definitions(root, &contents, &packages, &groups)?;
	validate_cli_runtime_requirements(&cli, github.as_ref(), &deployments)?;

	Ok(WorkspaceConfiguration {
		root_path: root.to_path_buf(),
		defaults: WorkspaceDefaults {
			parent_bump: defaults.parent_bump,
			include_private: defaults.include_private,
			warn_on_group_mismatch: defaults.warn_on_group_mismatch,
			package_type: defaults.package_type,
			changelog: defaults_changelog_policy,
			changelog_format: default_changelog_format,
			empty_update_message: defaults.empty_update_message,
		},
		release_notes: ReleaseNotesSettings {
			change_templates: release_notes.change_templates,
		},
		deployments,
		packages,
		groups,
		cli,
		github,
		cargo: ecosystems.cargo,
		npm: ecosystems.npm,
		deno: ecosystems.deno,
		dart: ecosystems.dart,
	})
}

pub fn load_change_signals(
	changes_path: &Path,
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> MonochangeResult<Vec<ChangeSignal>> {
	let contents = fs::read_to_string(changes_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			changes_path.display()
		))
	})?;
	let raw = if changes_path.extension().and_then(|value| value.to_str()) == Some("md") {
		parse_markdown_change_file(&contents, changes_path)?
	} else {
		toml::from_str::<RawChangeFile>(&contents).map_err(|error| {
			MonochangeError::Config(format!(
				"failed to parse {}: {error}",
				changes_path.display()
			))
		})?
	};

	let package_ids = configuration
		.packages
		.iter()
		.map(|package| package.id.as_str())
		.collect::<BTreeSet<_>>();
	let groups_by_id = configuration
		.groups
		.iter()
		.map(|group| (group.id.as_str(), group))
		.collect::<BTreeMap<_, _>>();
	let referenced_packages = raw
		.changes
		.iter()
		.filter(|change| package_ids.contains(change.package.as_str()))
		.map(|change| change.package.as_str())
		.collect::<BTreeSet<_>>();
	let referenced_groups = raw
		.changes
		.iter()
		.filter(|change| groups_by_id.contains_key(change.package.as_str()))
		.map(|change| change.package.as_str())
		.collect::<BTreeSet<_>>();

	for change in &raw.changes {
		if !package_ids.contains(change.package.as_str())
			&& !groups_by_id.contains_key(change.package.as_str())
		{
			return Err(changeset_diagnostic(
				&contents,
				changes_path,
				format!(
					"changeset `{}` references unknown package or group `{}`",
					changes_path.display(),
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

	for group_id in referenced_groups {
		let Some(group) = groups_by_id.get(group_id) else {
			continue;
		};
		if let Some(member_id) = group
			.packages
			.iter()
			.find(|member_id| referenced_packages.contains(member_id.as_str()))
		{
			return Err(changeset_diagnostic(
				&contents,
				changes_path,
				format!(
					"changeset `{}` references both group `{group_id}` and member package `{member_id}`",
					changes_path.display(),
				),
				vec![
					changeset_key_label(&contents, group_id, "group target"),
					changeset_key_label(&contents, member_id, "member package target"),
				],
				Some("reference either the group or one of its member packages, but not both in the same changeset".to_string()),
			));
		}
	}

	let mut seen_package_ids = BTreeSet::new();
	let mut signals = Vec::new();
	for change in raw.changes {
		if let Some(group) = groups_by_id.get(change.package.as_str()) {
			for member_id in &group.packages {
				let package_id =
					resolve_package_reference(member_id, &configuration.root_path, packages)?;
				if !seen_package_ids.insert(package_id.clone()) {
					return Err(changeset_diagnostic(
						&contents,
						changes_path,
						format!(
							"duplicate change entry for `{package_id}` in {}",
							changes_path.display()
						),
						vec![changeset_key_label(
							&contents,
							member_id,
							"duplicate package target",
						)],
						Some("keep one change entry per effective package target".to_string()),
					));
				}
				signals.push(ChangeSignal {
					package_id,
					requested_bump: change.bump,
					change_origin: change.origin.clone(),
					evidence_refs: change.evidence.clone(),
					notes: change.reason.clone(),
					details: change.details.clone(),
					change_type: change.change_type.clone(),
				});
			}
		} else {
			let package_id =
				resolve_package_reference(&change.package, &configuration.root_path, packages)?;
			if !seen_package_ids.insert(package_id.clone()) {
				return Err(changeset_diagnostic(
					&contents,
					changes_path,
					format!(
						"duplicate change entry for `{package_id}` in {}",
						changes_path.display()
					),
					vec![changeset_key_label(
						&contents,
						&change.package,
						"duplicate package target",
					)],
					Some("keep one change entry per effective package target".to_string()),
				));
			}
			signals.push(ChangeSignal {
				package_id,
				requested_bump: change.bump,
				change_origin: change.origin,
				evidence_refs: change.evidence,
				notes: change.reason,
				details: change.details,
				change_type: change.change_type,
			});
		}
	}

	Ok(signals)
}

pub fn resolve_package_reference(
	reference: &str,
	workspace_root: &Path,
	packages: &[PackageRecord],
) -> MonochangeResult<String> {
	let matching_package_ids = find_matching_package_ids(reference, workspace_root, packages);
	match matching_package_ids.as_slice() {
		[] => Err(MonochangeError::Config(format!(
			"change package reference `{reference}` did not match any discovered package"
		))),
		[package_id] => Ok(package_id.clone()),
		_ => Err(MonochangeError::Config(format!(
			"change package reference `{reference}` matched multiple packages: {}",
			matching_package_ids.join(", ")
		))),
	}
}

fn parse_markdown_change_file(
	contents: &str,
	changes_path: &Path,
) -> MonochangeResult<RawChangeFile> {
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
	let evidence_mapping = yaml_mapping(&mapping, "evidence");
	let origin_mapping = yaml_mapping(&mapping, "origin");
	let type_mapping = yaml_mapping(&mapping, "type");
	let (reason, details) = markdown_change_text(body);
	let mut changes = Vec::new();

	for (key, value) in &mapping {
		let Some(package) = key.as_str() else {
			continue;
		};
		if matches!(package, "evidence" | "origin" | "type") {
			continue;
		}
		let requested_bump = value
			.as_str()
			.and_then(parse_bump_severity)
			.ok_or_else(|| {
				MonochangeError::Config(format!(
					"failed to parse {}: package `{package}` must map to `patch`, `minor`, or `major`",
					changes_path.display()
				))
			})?;
		changes.push(RawChangeEntry {
			package: package.to_string(),
			bump: Some(requested_bump),
			reason: reason.clone(),
			details: details.clone(),
			change_type: type_mapping.and_then(|mapping| yaml_string(mapping, package)),
			origin: origin_mapping
				.and_then(|mapping| yaml_string(mapping, package))
				.unwrap_or_else(default_change_origin),
			evidence: evidence_mapping
				.and_then(|mapping| yaml_array_strings(mapping, package))
				.unwrap_or_default(),
		});
	}

	Ok(RawChangeFile { changes })
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
	let summary = summary_line.strip_prefix('#').map_or_else(
		|| summary_line.to_string(),
		|value| value.trim_start_matches('#').trim().to_string(),
	);
	let details = lines
		.iter()
		.skip(summary_index + 1)
		.copied()
		.collect::<Vec<_>>()
		.join("\n")
		.trim()
		.to_string();
	(
		Some(summary),
		if details.is_empty() {
			None
		} else {
			Some(details)
		},
	)
}

fn parse_bump_severity(value: &str) -> Option<BumpSeverity> {
	match value {
		"major" => Some(BumpSeverity::Major),
		"minor" => Some(BumpSeverity::Minor),
		"patch" => Some(BumpSeverity::Patch),
		_ => None,
	}
}

fn yaml_mapping<'map>(mapping: &'map Mapping, key: &str) -> Option<&'map Mapping> {
	mapping
		.get(YamlValue::String(key.to_string()))
		.and_then(YamlValue::as_mapping)
}

fn yaml_string(mapping: &Mapping, key: &str) -> Option<String> {
	mapping
		.get(YamlValue::String(key.to_string()))
		.and_then(YamlValue::as_str)
		.map(ToString::to_string)
}

fn yaml_array_strings(mapping: &Mapping, key: &str) -> Option<Vec<String>> {
	mapping
		.get(YamlValue::String(key.to_string()))
		.and_then(YamlValue::as_sequence)
		.map(|items| {
			items
				.iter()
				.filter_map(YamlValue::as_str)
				.map(ToString::to_string)
				.collect::<Vec<_>>()
		})
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

fn validate_versioned_files(
	config_contents: &str,
	versioned_files: &[VersionedFileDefinition],
	declared_packages: &BTreeSet<&str>,
	owner_kind: &str,
	owner_id: &str,
) -> MonochangeResult<()> {
	for versioned_file in versioned_files {
		if let VersionedFileDefinition::Dependency { dependency, .. } = versioned_file {
			if !declared_packages.contains(dependency.as_str()) {
				return Err(config_diagnostic(
					config_contents,
					format!(
						"{owner_id} references unknown versioned file dependency `{dependency}`"
					),
					vec![config_dependency_label(
						config_contents,
						owner_kind,
						owner_id,
						dependency,
						"unknown versioned file dependency",
					)],
					Some("reference a declared package id from `versioned_files` or remove the dependency entry".to_string()),
				));
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

fn validate_release_notes_configuration(
	contents: &str,
	release_notes: &RawReleaseNotesSettings,
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
	for extra_section in extra_sections {
		if extra_section.name.trim().is_empty() {
			return Err(config_diagnostic(
				contents,
				format!(
					"{section_kind} `{section_id}` has an extra changelog section with an empty `name`"
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
					"{section_kind} `{section_id}` extra changelog section `{}` must declare at least one type",
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
					"{section_kind} `{section_id}` extra changelog section `{}` must not include empty types",
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

fn validate_deployments_configuration(
	contents: &str,
	deployments: &[DeploymentDefinition],
) -> MonochangeResult<()> {
	let mut seen_names = BTreeSet::new();
	for deployment in deployments {
		if deployment.name.trim().is_empty() {
			return Err(MonochangeError::Config(
				"deployment names must not be empty".to_string(),
			));
		}
		if !seen_names.insert(deployment.name.clone()) {
			return Err(MonochangeError::Config(format!(
				"duplicate deployment `{}`",
				deployment.name
			)));
		}
		if deployment.workflow.trim().is_empty() {
			return Err(config_diagnostic(
				contents,
				format!(
					"deployment `{}` must declare a non-empty workflow",
					deployment.name
				),
				Vec::new(),
				Some(
					"set `workflow = \"deploy-production\"` or remove the deployment entry"
						.to_string(),
				),
			));
		}
		if deployment
			.requires
			.iter()
			.any(|required| required.trim().is_empty())
		{
			return Err(MonochangeError::Config(format!(
				"deployment `{}` must not include empty `requires` entries",
				deployment.name
			)));
		}
		if deployment
			.release_targets
			.iter()
			.any(|target| target.trim().is_empty())
		{
			return Err(MonochangeError::Config(format!(
				"deployment `{}` must not include empty `release_targets` entries",
				deployment.name
			)));
		}
	}
	Ok(())
}

fn change_template_variables(template: &str) -> Vec<String> {
	let mut variables = BTreeSet::new();
	let mut characters = template.chars().peekable();
	while let Some(character) = characters.next() {
		if character != '$' {
			continue;
		}
		let mut variable = String::new();
		while let Some(next) =
			characters.next_if(|next| next.is_ascii_alphanumeric() || *next == '_')
		{
			variable.push(next);
		}
		if !variable.is_empty() {
			variables.insert(variable);
		}
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
	if github.releases.generate_notes
		&& matches!(github.releases.source, GitHubReleaseNotesSource::Monochange)
	{
		return Err(MonochangeError::Config(
			"[github.releases].generate_notes cannot be true when `source = \"monochange\"`; choose one release-note source"
				.to_string(),
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
			}
		}

		for step in &cli_command.steps {
			match step {
				CliStepDefinition::Command {
					command,
					dry_run_command,
					..
				} => {
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
				CliStepDefinition::RenderReleaseManifest { path } => {
					if matches!(path, Some(path) if path.as_os_str().is_empty()) {
						return Err(MonochangeError::Config(format!(
							"CLI command `{}` render-manifest steps must provide a non-empty path when `path` is set",
							cli_command.name
						)));
					}
				}
				CliStepDefinition::Validate
				| CliStepDefinition::Discover
				| CliStepDefinition::CreateChangeFile
				| CliStepDefinition::PrepareRelease
				| CliStepDefinition::PublishGitHubRelease
				| CliStepDefinition::OpenReleasePullRequest
				| CliStepDefinition::Deploy { .. }
				| CliStepDefinition::EnforceChangesetPolicy => {}
			}
		}
	}

	Ok(())
}

fn validate_cli_runtime_requirements(
	cli: &[CliCommandDefinition],
	github: Option<&GitHubConfiguration>,
	deployments: &[DeploymentDefinition],
) -> MonochangeResult<()> {
	for cli_command in cli {
		if cli_command
			.steps
			.iter()
			.any(|step| matches!(step, CliStepDefinition::PublishGitHubRelease))
		{
			let github = github.ok_or_else(|| {
				MonochangeError::Config(format!(
					"CLI command `{}` uses `PublishGitHubRelease` but `[github]` is not configured",
					cli_command.name
				))
			})?;
			if !github.releases.enabled {
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` uses `PublishGitHubRelease` but `[github.releases].enabled` is false",
					cli_command.name
				)));
			}
		}
		if cli_command
			.steps
			.iter()
			.any(|step| matches!(step, CliStepDefinition::OpenReleasePullRequest))
		{
			let github = github.ok_or_else(|| {
				MonochangeError::Config(format!(
					"CLI command `{}` uses `OpenReleasePullRequest` but `[github]` is not configured",
					cli_command.name
				))
			})?;
			if !github.pull_requests.enabled {
				return Err(MonochangeError::Config(format!(
					"CLI command `{}` uses `OpenReleasePullRequest` but `[github.pull_requests].enabled` is false",
					cli_command.name
				)));
			}
		}
		for step in &cli_command.steps {
			match step {
				CliStepDefinition::Deploy { names } => {
					if deployments.is_empty() {
						return Err(MonochangeError::Config(format!(
							"CLI command `{}` uses `Deploy` but no `[[deployments]]` are configured",
							cli_command.name
						)));
					}
					for name in names {
						if !deployments
							.iter()
							.any(|deployment| deployment.name == *name)
						{
							return Err(MonochangeError::Config(format!(
								"CLI command `{}` deploy step references unknown deployment `{name}`",
								cli_command.name
							)));
						}
					}
				}
				CliStepDefinition::EnforceChangesetPolicy => {
					let github = github.ok_or_else(|| {
						MonochangeError::Config(format!(
							"CLI command `{}` uses `EnforceChangesetPolicy` but `[github]` is not configured",
							cli_command.name
						))
					})?;
					if !github.bot.changesets.enabled {
						return Err(MonochangeError::Config(format!(
							"CLI command `{}` uses `EnforceChangesetPolicy` but `[github.bot.changesets].enabled` is false",
							cli_command.name
						)));
					}
					let changed_path_input = cli_command_input(cli_command, "changed_path")
						.ok_or_else(|| {
							MonochangeError::Config(format!(
								"CLI command `{}` uses `EnforceChangesetPolicy` but does not declare a `changed_path` input",
								cli_command.name
							))
						})?;
					if !matches!(changed_path_input.kind, CliInputKind::StringList) {
						return Err(MonochangeError::Config(format!(
							"CLI command `{}` input `changed_path` must use type `string_list` for `EnforceChangesetPolicy`",
							cli_command.name
						)));
					}
					if let Some(label_input) = cli_command_input(cli_command, "label") {
						if !matches!(label_input.kind, CliInputKind::StringList) {
							return Err(MonochangeError::Config(format!(
								"CLI command `{}` input `label` must use type `string_list` when used with `EnforceChangesetPolicy`",
								cli_command.name
							)));
						}
					}
				}
				_ => {}
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
		parse_markdown_change_file(&contents, changeset_path)?
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
	let referenced_packages = raw
		.changes
		.iter()
		.filter(|change| package_ids.contains(change.package.as_str()))
		.map(|change| change.package.as_str())
		.collect::<BTreeSet<_>>();
	let referenced_groups = raw
		.changes
		.iter()
		.filter(|change| group_members.contains_key(change.package.as_str()))
		.map(|change| change.package.as_str())
		.collect::<BTreeSet<_>>();

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

	for group_id in referenced_groups {
		let Some(members) = group_members.get(group_id) else {
			continue;
		};
		if let Some(member_id) = referenced_packages
			.iter()
			.find(|member_id| members.contains(**member_id))
		{
			return Err(changeset_diagnostic(
				&contents,
				changeset_path,
				format!(
					"changeset `{}` references both group `{group_id}` and member package `{member_id}`",
					changeset_path.display(),
				),
				vec![
					changeset_key_label(&contents, group_id, "group target"),
					changeset_key_label(&contents, member_id, "member package target"),
				],
				Some("reference either the group or one of its member packages, but not both in the same changeset".to_string()),
			));
		}
	}

	Ok(())
}

#[cfg(test)]
mod __tests;
