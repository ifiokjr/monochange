#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

doc_comment::doctest!("../readme.md");

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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct VersionGroupDefinition {
	pub name: String,
	pub members: Vec<String>,
	pub strategy: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceDefaults {
	pub parent_bump: BumpSeverity,
	pub include_private: bool,
	pub warn_on_group_mismatch: bool,
}

impl Default for WorkspaceDefaults {
	fn default() -> Self {
		Self {
			parent_bump: BumpSeverity::Patch,
			include_private: false,
			warn_on_group_mismatch: true,
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
pub struct PackageOverride {
	pub package: String,
	pub changelog: Option<PathBuf>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfiguration {
	pub root_path: PathBuf,
	pub defaults: WorkspaceDefaults,
	pub version_groups: Vec<VersionGroupDefinition>,
	pub package_overrides: Vec<PackageOverride>,
	pub cargo: EcosystemSettings,
	pub npm: EcosystemSettings,
	pub deno: EcosystemSettings,
	pub dart: EcosystemSettings,
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
