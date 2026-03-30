#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_core`
//!
//! <!-- {=monochangeCoreCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_core` is the shared vocabulary for the `monochange` workspace.
//!
//! Reach for this crate when you are building ecosystem adapters, release planners, or custom automation and need one set of types for packages, dependency edges, version groups, change signals, and release plans.
//!
//! ## Why use it?
//!
//! - avoid redefining package and release domain models in each crate
//! - share one error and result surface across discovery, planning, and workflow layers
//! - pass normalized workspace data between adapters and planners without extra translation
//!
//! ## Best for
//!
//! - implementing new ecosystem adapters against the shared `EcosystemAdapter` contract
//! - moving normalized package or release data between crates without custom conversion code
//! - depending on the workspace domain model without pulling in discovery or planning behavior
//!
//! ## What it provides
//!
//! - normalized package and dependency records
//! - version-group definitions and planned group outcomes
//! - change signals and compatibility assessments
//! - changelog formats, changelog targets, structured release-note types, release-manifest types, and GitHub automation config types
//! - shared error and result types
//!
//! ## Example
//!
//! ```rust
//! use monochange_core::render_release_notes;
//! use monochange_core::ChangelogFormat;
//! use monochange_core::ReleaseNotesDocument;
//! use monochange_core::ReleaseNotesSection;
//!
//! let notes = ReleaseNotesDocument {
//!     title: "1.2.3".to_string(),
//!     summary: vec!["Grouped release for `sdk`.".to_string()],
//!     sections: vec![ReleaseNotesSection {
//!         title: "Features".to_string(),
//!         entries: vec!["- add keep-a-changelog output".to_string()],
//!     }],
//! };
//!
//! let rendered = render_release_notes(ChangelogFormat::KeepAChangelog, &notes);
//!
//! assert!(rendered.contains("## [1.2.3]"));
//! assert!(rendered.contains("### Features"));
//! assert!(rendered.contains("- add keep-a-changelog output"));
//! ```
//! <!-- {/monochangeCoreCrateDocs} -->

use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use semver::Version;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

pub type MonochangeResult<T> = Result<T, MonochangeError>;

#[derive(Debug, Error)]
pub enum MonochangeError {
	#[error("io error: {0}")]
	Io(String),
	#[error("config error: {0}")]
	Config(String),
	#[error("discovery error: {0}")]
	Discovery(String),
	#[error("{0}")]
	Diagnostic(String),
}

impl MonochangeError {
	#[must_use]
	pub fn render(&self) -> String {
		match self {
			Self::Diagnostic(report) => report.clone(),
			_ => self.to_string(),
		}
	}
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BumpSeverity {
	None,
	#[default]
	Patch,
	Minor,
	Major,
}

impl BumpSeverity {
	#[must_use]
	pub fn is_release(self) -> bool {
		self != Self::None
	}

	#[must_use]
	pub fn apply_to_version(self, version: &Version) -> Version {
		let mut next = version.clone();
		match self {
			Self::None => next,
			Self::Patch => {
				next.patch += 1;
				next.pre = semver::Prerelease::EMPTY;
				next.build = semver::BuildMetadata::EMPTY;
				next
			}
			Self::Minor => {
				next.minor += 1;
				next.patch = 0;
				next.pre = semver::Prerelease::EMPTY;
				next.build = semver::BuildMetadata::EMPTY;
				next
			}
			Self::Major => {
				next.major += 1;
				next.minor = 0;
				next.patch = 0;
				next.pre = semver::Prerelease::EMPTY;
				next.build = semver::BuildMetadata::EMPTY;
				next
			}
		}
	}
}

impl fmt::Display for BumpSeverity {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(match self {
			Self::None => "none",
			Self::Patch => "patch",
			Self::Minor => "minor",
			Self::Major => "major",
		})
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ecosystem {
	Cargo,
	Npm,
	Deno,
	Dart,
	Flutter,
}

impl Ecosystem {
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Cargo => "cargo",
			Self::Npm => "npm",
			Self::Deno => "deno",
			Self::Dart => "dart",
			Self::Flutter => "flutter",
		}
	}
}

impl fmt::Display for Ecosystem {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishState {
	Public,
	Private,
	Unpublished,
	Excluded,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
	Runtime,
	Development,
	Build,
	Peer,
	Workspace,
	Unknown,
}

impl fmt::Display for DependencyKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(match self {
			Self::Runtime => "runtime",
			Self::Development => "development",
			Self::Build => "build",
			Self::Peer => "peer",
			Self::Workspace => "workspace",
			Self::Unknown => "unknown",
		})
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencySourceKind {
	Manifest,
	Workspace,
	Transitive,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageDependency {
	pub name: String,
	pub kind: DependencyKind,
	pub version_constraint: Option<String>,
	pub optional: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageRecord {
	pub id: String,
	pub name: String,
	pub ecosystem: Ecosystem,
	pub manifest_path: PathBuf,
	pub workspace_root: PathBuf,
	pub current_version: Option<Version>,
	pub publish_state: PublishState,
	pub version_group_id: Option<String>,
	pub metadata: BTreeMap<String, String>,
	pub declared_dependencies: Vec<PackageDependency>,
}

impl PackageRecord {
	#[allow(clippy::needless_pass_by_value)]
	#[must_use]
	pub fn new(
		ecosystem: Ecosystem,
		name: impl Into<String>,
		manifest_path: PathBuf,
		workspace_root: PathBuf,
		current_version: Option<Version>,
		publish_state: PublishState,
	) -> Self {
		let name = name.into();
		let normalized_workspace_root = normalize_path(&workspace_root);
		let normalized_manifest_path = normalize_path(&manifest_path);
		let id_path = relative_to_root(&normalized_workspace_root, &normalized_manifest_path)
			.unwrap_or_else(|| normalized_manifest_path.clone());
		let id = format!("{}:{}", ecosystem.as_str(), id_path.to_string_lossy());

		Self {
			id,
			name,
			ecosystem,
			manifest_path: normalized_manifest_path,
			workspace_root: normalized_workspace_root,
			current_version,
			publish_state,
			version_group_id: None,
			metadata: BTreeMap::new(),
			declared_dependencies: Vec::new(),
		}
	}

	#[must_use]
	pub fn relative_manifest_path(&self, root: &Path) -> Option<PathBuf> {
		relative_to_root(root, &self.manifest_path)
	}
}

#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
	let absolute = if path.is_absolute() {
		path.to_path_buf()
	} else {
		env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
	};
	fs::canonicalize(&absolute).unwrap_or(absolute)
}

#[must_use]
pub fn relative_to_root(root: &Path, path: &Path) -> Option<PathBuf> {
	let normalized_root = normalize_path(root);
	let normalized_path = normalize_path(path);
	normalized_path
		.strip_prefix(&normalized_root)
		.ok()
		.map(Path::to_path_buf)
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyEdge {
	pub from_package_id: String,
	pub to_package_id: String,
	pub dependency_kind: DependencyKind,
	pub source_kind: DependencySourceKind,
	pub version_constraint: Option<String>,
	pub is_optional: bool,
	pub is_direct: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageType {
	Cargo,
	Npm,
	Deno,
	Dart,
	Flutter,
}

impl PackageType {
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Cargo => "cargo",
			Self::Npm => "npm",
			Self::Deno => "deno",
			Self::Dart => "dart",
			Self::Flutter => "flutter",
		}
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VersionFormat {
	#[default]
	Namespaced,
	Primary,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VersionedFileDefinition {
	Path(PathBuf),
	Dependency { path: PathBuf, dependency: String },
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChangelogDefinition {
	Disabled,
	PackageDefault,
	PathPattern(String),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChangelogFormat {
	#[default]
	Monochange,
	KeepAChangelog,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangelogTarget {
	pub path: PathBuf,
	#[serde(default)]
	pub format: ChangelogFormat,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleaseNotesSection {
	pub title: String,
	pub entries: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleaseNotesDocument {
	pub title: String,
	pub summary: Vec<String>,
	pub sections: Vec<ReleaseNotesSection>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExtraChangelogSection {
	pub name: String,
	#[serde(default)]
	pub types: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct ReleaseNotesSettings {
	#[serde(default)]
	pub change_templates: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageDefinition {
	pub id: String,
	pub path: PathBuf,
	pub package_type: PackageType,
	pub changelog: Option<ChangelogTarget>,
	pub extra_changelog_sections: Vec<ExtraChangelogSection>,
	pub versioned_files: Vec<VersionedFileDefinition>,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupDefinition {
	pub id: String,
	pub packages: Vec<String>,
	pub changelog: Option<ChangelogTarget>,
	pub extra_changelog_sections: Vec<ExtraChangelogSection>,
	pub versioned_files: Vec<VersionedFileDefinition>,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceDefaults {
	pub parent_bump: BumpSeverity,
	pub include_private: bool,
	pub warn_on_group_mismatch: bool,
	pub package_type: Option<PackageType>,
	pub changelog: Option<ChangelogDefinition>,
	pub changelog_format: ChangelogFormat,
}

impl Default for WorkspaceDefaults {
	fn default() -> Self {
		Self {
			parent_bump: BumpSeverity::Patch,
			include_private: false,
			warn_on_group_mismatch: true,
			package_type: None,
			changelog: None,
			changelog_format: ChangelogFormat::Monochange,
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct EcosystemSettings {
	#[serde(default)]
	pub enabled: Option<bool>,
	#[serde(default)]
	pub roots: Vec<String>,
	#[serde(default)]
	pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowInputKind {
	String,
	StringList,
	Path,
	Choice,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowInputDefinition {
	pub name: String,
	#[serde(rename = "type")]
	pub kind: WorkflowInputKind,
	#[serde(default)]
	pub help_text: Option<String>,
	#[serde(default)]
	pub required: bool,
	#[serde(default)]
	pub default: Option<String>,
	#[serde(default)]
	pub choices: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandVariable {
	Version,
	GroupVersion,
	ReleasedPackages,
	ChangedFiles,
	Changesets,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkflowStepDefinition {
	Validate,
	Discover,
	CreateChangeFile,
	PrepareRelease,
	RenderReleaseManifest {
		#[serde(default)]
		path: Option<PathBuf>,
	},
	PublishGitHubRelease,
	Command {
		command: String,
		#[serde(default)]
		dry_run: Option<String>,
		#[serde(default)]
		shell: bool,
		#[serde(default)]
		variables: Option<BTreeMap<String, CommandVariable>>,
	},
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
	pub name: String,
	#[serde(default)]
	pub help_text: Option<String>,
	#[serde(default)]
	pub inputs: Vec<WorkflowInputDefinition>,
	#[serde(default)]
	pub steps: Vec<WorkflowStepDefinition>,
}

#[must_use]
pub fn render_release_notes(format: ChangelogFormat, document: &ReleaseNotesDocument) -> String {
	match format {
		ChangelogFormat::Monochange => render_monochange_release_notes(document),
		ChangelogFormat::KeepAChangelog => render_keep_a_changelog_release_notes(document),
	}
}

fn render_monochange_release_notes(document: &ReleaseNotesDocument) -> String {
	let mut lines = vec![format!("## {}", document.title), String::new()];
	for (index, paragraph) in document.summary.iter().enumerate() {
		if index > 0 {
			lines.push(String::new());
		}
		lines.push(paragraph.clone());
	}
	let include_section_headings = document.sections.len() > 1
		|| document
			.sections
			.iter()
			.any(|section| section.title != "Changed");
	for section in &document.sections {
		if section.entries.is_empty() {
			continue;
		}
		if !lines.last().is_some_and(String::is_empty) {
			lines.push(String::new());
		}
		if include_section_headings {
			lines.push(format!("### {}", section.title));
			lines.push(String::new());
		}
		push_release_note_entries(&mut lines, &section.entries);
	}
	lines.join("\n")
}

fn render_keep_a_changelog_release_notes(document: &ReleaseNotesDocument) -> String {
	let mut lines = vec![format!("## [{}]", document.title), String::new()];
	for (index, paragraph) in document.summary.iter().enumerate() {
		if index > 0 {
			lines.push(String::new());
		}
		lines.push(paragraph.clone());
	}
	for section in &document.sections {
		if section.entries.is_empty() {
			continue;
		}
		if !lines.last().is_some_and(String::is_empty) {
			lines.push(String::new());
		}
		lines.push(format!("### {}", section.title));
		lines.push(String::new());
		push_release_note_entries(&mut lines, &section.entries);
	}
	lines.join("\n")
}

fn push_release_note_entries(lines: &mut Vec<String>, entries: &[String]) {
	for (index, entry) in entries.iter().enumerate() {
		let trimmed = entry.trim();
		if trimmed.contains('\n') {
			lines.extend(trimmed.lines().map(ToString::to_string));
			if index + 1 < entries.len() {
				lines.push(String::new());
			}
			continue;
		}
		if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with('#') {
			lines.push(trimmed.to_string());
		} else {
			lines.push(format!("- {trimmed}"));
		}
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseOwnerKind {
	Package,
	Group,
}

impl ReleaseOwnerKind {
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Package => "package",
			Self::Group => "group",
		}
	}
}

impl fmt::Display for ReleaseOwnerKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestTarget {
	pub id: String,
	pub kind: ReleaseOwnerKind,
	pub version: String,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
	pub tag_name: String,
	pub members: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestChangelog {
	pub owner_id: String,
	pub owner_kind: ReleaseOwnerKind,
	pub path: PathBuf,
	pub format: ChangelogFormat,
	pub notes: ReleaseNotesDocument,
	pub rendered: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestPlanDecision {
	pub package: String,
	pub bump: BumpSeverity,
	pub trigger: String,
	pub planned_version: Option<String>,
	pub reasons: Vec<String>,
	pub upstream_sources: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestPlanGroup {
	pub id: String,
	pub planned_version: Option<String>,
	pub members: Vec<String>,
	pub bump: BumpSeverity,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestCompatibilityEvidence {
	pub package: String,
	pub provider: String,
	pub severity: BumpSeverity,
	pub summary: String,
	pub confidence: String,
	pub evidence_location: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestPlan {
	pub workspace_root: PathBuf,
	pub decisions: Vec<ReleaseManifestPlanDecision>,
	pub groups: Vec<ReleaseManifestPlanGroup>,
	pub warnings: Vec<String>,
	pub unresolved_items: Vec<String>,
	pub compatibility_evidence: Vec<ReleaseManifestCompatibilityEvidence>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDeploymentIntent {
	pub name: String,
	#[serde(default)]
	pub environment: Option<String>,
	#[serde(default)]
	pub release_targets: Vec<String>,
	#[serde(default)]
	pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifest {
	pub workflow: String,
	pub dry_run: bool,
	#[serde(default)]
	pub version: Option<String>,
	#[serde(default)]
	pub group_version: Option<String>,
	pub release_targets: Vec<ReleaseManifestTarget>,
	pub released_packages: Vec<String>,
	pub changed_files: Vec<PathBuf>,
	pub changelogs: Vec<ReleaseManifestChangelog>,
	#[serde(default)]
	pub deleted_changesets: Vec<PathBuf>,
	#[serde(default)]
	pub deployments: Vec<ReleaseDeploymentIntent>,
	pub plan: ReleaseManifestPlan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GitHubReleaseNotesSource {
	#[default]
	Monochange,
	#[serde(rename = "github_generated")]
	GitHubGenerated,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct GitHubReleaseSettings {
	pub enabled: bool,
	pub draft: bool,
	pub prerelease: bool,
	pub generate_notes: bool,
	pub source: GitHubReleaseNotesSource,
}

impl Default for GitHubReleaseSettings {
	fn default() -> Self {
		Self {
			enabled: true,
			draft: false,
			prerelease: false,
			generate_notes: false,
			source: GitHubReleaseNotesSource::Monochange,
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct GitHubConfiguration {
	pub owner: String,
	pub repo: String,
	#[serde(default)]
	pub releases: GitHubReleaseSettings,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectiveReleaseIdentity {
	pub owner_id: String,
	pub owner_kind: ReleaseOwnerKind,
	pub group_id: Option<String>,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
	pub members: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfiguration {
	pub root_path: PathBuf,
	pub defaults: WorkspaceDefaults,
	pub release_notes: ReleaseNotesSettings,
	pub packages: Vec<PackageDefinition>,
	pub groups: Vec<GroupDefinition>,
	pub workflows: Vec<WorkflowDefinition>,
	pub github: Option<GitHubConfiguration>,
	pub cargo: EcosystemSettings,
	pub npm: EcosystemSettings,
	pub deno: EcosystemSettings,
	pub dart: EcosystemSettings,
}

impl WorkspaceConfiguration {
	#[must_use]
	pub fn package_by_id(&self, package_id: &str) -> Option<&PackageDefinition> {
		self.packages
			.iter()
			.find(|package| package.id == package_id)
	}

	#[must_use]
	pub fn group_by_id(&self, group_id: &str) -> Option<&GroupDefinition> {
		self.groups.iter().find(|group| group.id == group_id)
	}

	#[must_use]
	pub fn group_for_package(&self, package_id: &str) -> Option<&GroupDefinition> {
		self.groups
			.iter()
			.find(|group| group.packages.iter().any(|member| member == package_id))
	}

	#[must_use]
	pub fn effective_release_identity(&self, package_id: &str) -> Option<EffectiveReleaseIdentity> {
		let package = self.package_by_id(package_id)?;
		if let Some(group) = self.group_for_package(package_id) {
			return Some(EffectiveReleaseIdentity {
				owner_id: group.id.clone(),
				owner_kind: ReleaseOwnerKind::Group,
				group_id: Some(group.id.clone()),
				tag: group.tag,
				release: group.release,
				version_format: group.version_format,
				members: group.packages.clone(),
			});
		}

		Some(EffectiveReleaseIdentity {
			owner_id: package.id.clone(),
			owner_kind: ReleaseOwnerKind::Package,
			group_id: None,
			tag: package.tag,
			release: package.release,
			version_format: package.version_format,
			members: vec![package.id.clone()],
		})
	}
}

#[must_use]
pub fn default_workflows() -> Vec<WorkflowDefinition> {
	vec![
		WorkflowDefinition {
			name: "validate".to_string(),
			help_text: Some("Validate monochange configuration and changesets".to_string()),
			inputs: Vec::new(),
			steps: vec![WorkflowStepDefinition::Validate],
		},
		WorkflowDefinition {
			name: "discover".to_string(),
			help_text: Some("Discover packages across supported ecosystems".to_string()),
			inputs: vec![WorkflowInputDefinition {
				name: "format".to_string(),
				kind: WorkflowInputKind::Choice,
				help_text: Some("Output format".to_string()),
				required: false,
				default: Some("text".to_string()),
				choices: vec!["text".to_string(), "json".to_string()],
			}],
			steps: vec![WorkflowStepDefinition::Discover],
		},
		WorkflowDefinition {
			name: "change".to_string(),
			help_text: Some("Create a change file for one or more packages".to_string()),
			inputs: vec![
				WorkflowInputDefinition {
					name: "package".to_string(),
					kind: WorkflowInputKind::StringList,
					help_text: Some("Package or group to include in the change".to_string()),
					required: true,
					default: None,
					choices: Vec::new(),
				},
				WorkflowInputDefinition {
					name: "bump".to_string(),
					kind: WorkflowInputKind::Choice,
					help_text: Some("Requested semantic version bump".to_string()),
					required: false,
					default: Some("patch".to_string()),
					choices: vec![
						"patch".to_string(),
						"minor".to_string(),
						"major".to_string(),
					],
				},
				WorkflowInputDefinition {
					name: "reason".to_string(),
					kind: WorkflowInputKind::String,
					help_text: Some("Short release-note summary for this change".to_string()),
					required: true,
					default: None,
					choices: Vec::new(),
				},
				WorkflowInputDefinition {
					name: "type".to_string(),
					kind: WorkflowInputKind::String,
					help_text: Some(
						"Optional release-note type such as `security` or `note`".to_string(),
					),
					required: false,
					default: None,
					choices: Vec::new(),
				},
				WorkflowInputDefinition {
					name: "details".to_string(),
					kind: WorkflowInputKind::String,
					help_text: Some("Optional multi-line release-note details".to_string()),
					required: false,
					default: None,
					choices: Vec::new(),
				},
				WorkflowInputDefinition {
					name: "evidence".to_string(),
					kind: WorkflowInputKind::StringList,
					help_text: Some("Additional evidence strings to include".to_string()),
					required: false,
					default: None,
					choices: Vec::new(),
				},
				WorkflowInputDefinition {
					name: "output".to_string(),
					kind: WorkflowInputKind::Path,
					help_text: Some(
						"Write the generated change file to a specific path".to_string(),
					),
					required: false,
					default: None,
					choices: Vec::new(),
				},
			],
			steps: vec![WorkflowStepDefinition::CreateChangeFile],
		},
		WorkflowDefinition {
			name: "release".to_string(),
			help_text: Some("Prepare a release from discovered change files".to_string()),
			inputs: vec![WorkflowInputDefinition {
				name: "format".to_string(),
				kind: WorkflowInputKind::Choice,
				help_text: Some("Output format".to_string()),
				required: false,
				default: Some("text".to_string()),
				choices: vec!["text".to_string(), "json".to_string()],
			}],
			steps: vec![WorkflowStepDefinition::PrepareRelease],
		},
	]
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct VersionGroup {
	pub group_id: String,
	pub display_name: String,
	pub members: Vec<String>,
	pub mismatch_detected: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlannedVersionGroup {
	pub group_id: String,
	pub display_name: String,
	pub members: Vec<String>,
	pub mismatch_detected: bool,
	pub planned_version: Option<Version>,
	pub recommended_bump: BumpSeverity,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeSignal {
	pub package_id: String,
	pub requested_bump: Option<BumpSeverity>,
	pub change_origin: String,
	pub evidence_refs: Vec<String>,
	pub notes: Option<String>,
	pub details: Option<String>,
	pub change_type: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompatibilityAssessment {
	pub package_id: String,
	pub provider_id: String,
	pub severity: BumpSeverity,
	pub confidence: String,
	pub summary: String,
	pub evidence_location: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleaseDecision {
	pub package_id: String,
	pub trigger_type: String,
	pub recommended_bump: BumpSeverity,
	pub planned_version: Option<Version>,
	pub group_id: Option<String>,
	pub reasons: Vec<String>,
	pub upstream_sources: Vec<String>,
	pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleasePlan {
	pub workspace_root: PathBuf,
	pub decisions: Vec<ReleaseDecision>,
	pub groups: Vec<PlannedVersionGroup>,
	pub warnings: Vec<String>,
	pub unresolved_items: Vec<String>,
	pub compatibility_evidence: Vec<CompatibilityAssessment>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryReport {
	pub workspace_root: PathBuf,
	pub packages: Vec<PackageRecord>,
	pub dependencies: Vec<DependencyEdge>,
	pub version_groups: Vec<VersionGroup>,
	pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AdapterDiscovery {
	pub packages: Vec<PackageRecord>,
	pub warnings: Vec<String>,
}

pub trait EcosystemAdapter {
	fn ecosystem(&self) -> Ecosystem;

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery>;
}

#[must_use]
pub fn materialize_dependency_edges(packages: &[PackageRecord]) -> Vec<DependencyEdge> {
	let mut package_ids_by_name = BTreeMap::<String, Vec<String>>::new();
	for package in packages {
		package_ids_by_name
			.entry(package.name.clone())
			.or_default()
			.push(package.id.clone());
	}

	let mut edges = Vec::new();
	for package in packages {
		for dependency in &package.declared_dependencies {
			if let Some(target_package_ids) = package_ids_by_name.get(&dependency.name) {
				for target_package_id in target_package_ids {
					edges.push(DependencyEdge {
						from_package_id: package.id.clone(),
						to_package_id: target_package_id.clone(),
						dependency_kind: dependency.kind,
						source_kind: DependencySourceKind::Manifest,
						version_constraint: dependency.version_constraint.clone(),
						is_optional: dependency.optional,
						is_direct: true,
					});
				}
			}
		}
	}

	edges
}

#[cfg(test)]
mod __tests;
