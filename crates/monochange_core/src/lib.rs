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
//! - release-plan domain types
//! - shared error and result types
//!
//! ## Example
//!
//! ```rust
//! use monochange_core::Ecosystem;
//! use monochange_core::PackageRecord;
//! use monochange_core::PublishState;
//! use semver::Version;
//! use std::path::PathBuf;
//!
//! let package = PackageRecord::new(
//!     Ecosystem::Cargo,
//!     "demo",
//!     PathBuf::from("crates/demo/Cargo.toml"),
//!     PathBuf::from("."),
//!     Some(Version::new(1, 2, 3)),
//!     PublishState::Public,
//! );
//!
//! assert_eq!(package.name, "demo");
//! assert_eq!(package.current_version, Some(Version::new(1, 2, 3)));
//! ```
//! <!-- {/monochangeCoreCrateDocs} -->

use std::collections::BTreeMap;
use std::fmt;
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
		let id = format!("{}:{}", ecosystem.as_str(), manifest_path.to_string_lossy());

		Self {
			id,
			name,
			ecosystem,
			manifest_path,
			workspace_root,
			current_version,
			publish_state,
			version_group_id: None,
			metadata: BTreeMap::new(),
			declared_dependencies: Vec::new(),
		}
	}

	#[must_use]
	pub fn relative_manifest_path(&self, root: &Path) -> Option<PathBuf> {
		self.manifest_path
			.strip_prefix(root)
			.ok()
			.map(Path::to_path_buf)
	}
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
pub struct PackageDefinition {
	pub id: String,
	pub path: PathBuf,
	pub package_type: PackageType,
	pub changelog: Option<PathBuf>,
	pub versioned_files: Vec<VersionedFileDefinition>,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupDefinition {
	pub id: String,
	pub packages: Vec<String>,
	pub changelog: Option<PathBuf>,
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
}

impl Default for WorkspaceDefaults {
	fn default() -> Self {
		Self {
			parent_bump: BumpSeverity::Patch,
			include_private: false,
			warn_on_group_mismatch: true,
			package_type: None,
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkflowStepDefinition {
	PrepareRelease,
	Command { command: String },
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
	pub name: String,
	#[serde(default)]
	pub steps: Vec<WorkflowStepDefinition>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseOwnerKind {
	Package,
	Group,
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
	pub packages: Vec<PackageDefinition>,
	pub groups: Vec<GroupDefinition>,
	pub workflows: Vec<WorkflowDefinition>,
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
