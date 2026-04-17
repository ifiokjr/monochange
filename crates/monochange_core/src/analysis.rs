use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::Ecosystem;
use crate::MonochangeResult;
use crate::PackageRecord;

/// Level of detail requested from semantic analyzers.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DetectionLevel {
	/// Fastest mode. Prefer lightweight structural extraction.
	Basic,
	/// Extract before/after signatures when possible.
	Signature,
	/// Perform the richest semantic extraction available for the ecosystem.
	Semantic,
}

/// How a file changed between the analyzed revisions.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FileChangeKind {
	Added,
	Modified,
	Deleted,
}

/// One file that changed for the analyzed package.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzedFileChange {
	/// Repository-relative path.
	pub path: PathBuf,
	/// Package-relative path.
	pub package_path: PathBuf,
	/// Change kind.
	pub kind: FileChangeKind,
	/// File contents before the change, when available and text-decodable.
	pub before_contents: Option<String>,
	/// File contents after the change, when available and text-decodable.
	pub after_contents: Option<String>,
}

/// One text file captured in a package snapshot.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSnapshotFile {
	/// Package-relative path.
	pub path: PathBuf,
	/// UTF-8-decoded file contents.
	pub contents: String,
}

/// A package snapshot at one side of the comparison.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSnapshot {
	/// Human-readable label for this snapshot.
	pub label: String,
	/// Text files available to analyzers.
	pub files: Vec<PackageSnapshotFile>,
}

impl PackageSnapshot {
	/// Look up one file by package-relative path.
	#[must_use]
	pub fn file(&self, path: &Path) -> Option<&PackageSnapshotFile> {
		self.files.iter().find(|file| file.path == path)
	}
}

/// High-level semantic change category shared across ecosystems.
#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SemanticChangeCategory {
	PublicApi,
	Export,
	Dependency,
	Metadata,
}

/// Whether an entity was added, removed, or modified.
#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SemanticChangeKind {
	Added,
	Removed,
	Modified,
}

/// One semantic diff record emitted by an ecosystem analyzer.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticChange {
	/// Broad category of change.
	pub category: SemanticChangeCategory,
	/// Whether the item was added, removed, or modified.
	pub kind: SemanticChangeKind,
	/// Ecosystem-specific item kind such as `function`, `struct`, `class`, or `dependency`.
	pub item_kind: String,
	/// Stable symbol or item path, such as `crate::api::render` or `serde`.
	pub item_path: String,
	/// Human-readable explanation of the change.
	pub summary: String,
	/// Package-relative file path that contributed the evidence.
	pub file_path: PathBuf,
	/// Signature or descriptor before the change, when available.
	pub before_signature: Option<String>,
	/// Signature or descriptor after the change, when available.
	pub after_signature: Option<String>,
}

/// Analyzer output for one package.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageAnalysisResult {
	/// Unique analyzer identifier.
	pub analyzer_id: String,
	/// Package identifier used in reports.
	pub package_id: String,
	/// Package ecosystem.
	pub ecosystem: Ecosystem,
	/// Package-relative files that contributed to the analysis.
	pub changed_files: Vec<PathBuf>,
	/// Structured semantic diffs.
	pub semantic_changes: Vec<SemanticChange>,
	/// Non-fatal warnings from the analyzer.
	pub warnings: Vec<String>,
}

/// Input context passed to an ecosystem analyzer.
#[derive(Debug)]
pub struct PackageAnalysisContext<'a> {
	/// Repository root.
	pub repo_root: &'a Path,
	/// Discovered package being analyzed.
	pub package: &'a PackageRecord,
	/// Requested detection level.
	pub detection_level: DetectionLevel,
	/// File deltas for this package.
	pub changed_files: &'a [AnalyzedFileChange],
	/// Package snapshot before the change, when available.
	pub before_snapshot: Option<&'a PackageSnapshot>,
	/// Package snapshot after the change, when available.
	pub after_snapshot: Option<&'a PackageSnapshot>,
}

impl PackageAnalysisContext<'_> {
	/// Return the package root directory.
	#[must_use]
	pub fn package_root(&self) -> &Path {
		self.package
			.manifest_path
			.parent()
			.unwrap_or(&self.package.workspace_root)
	}
}

/// Ecosystem-specific semantic analyzer contract.
pub trait SemanticAnalyzer: Send + Sync {
	/// Stable analyzer identifier.
	fn analyzer_id(&self) -> &'static str;

	/// Return `true` when this analyzer can handle the package.
	fn applies_to(&self, package: &PackageRecord) -> bool;

	/// Analyze one package and return semantic diffs.
	fn analyze_package(
		&self,
		context: &PackageAnalysisContext<'_>,
	) -> MonochangeResult<PackageAnalysisResult>;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::PackageRecord;
	use crate::PublishState;

	#[test]
	fn package_snapshot_file_lookup_finds_matching_paths() {
		let snapshot = PackageSnapshot {
			label: "after".to_string(),
			files: vec![PackageSnapshotFile {
				path: PathBuf::from("src/lib.rs"),
				contents: "pub fn greet() {}".to_string(),
			}],
		};

		let file = snapshot
			.file(Path::new("src/lib.rs"))
			.unwrap_or_else(|| panic!("expected file in snapshot"));
		assert_eq!(file.contents, "pub fn greet() {}");
	}

	#[test]
	fn package_analysis_context_exposes_package_root() {
		let package = PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			PathBuf::from("/repo/crates/core/Cargo.toml"),
			PathBuf::from("/repo"),
			None,
			PublishState::Public,
		);
		let context = PackageAnalysisContext {
			repo_root: Path::new("/repo"),
			package: &package,
			detection_level: DetectionLevel::Signature,
			changed_files: &[],
			before_snapshot: None,
			after_snapshot: None,
		};

		assert_eq!(context.package_root(), Path::new("/repo/crates/core"));
	}
}
