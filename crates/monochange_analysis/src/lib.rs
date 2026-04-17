#![forbid(clippy::indexing_slicing)]

//! # `monochange_analysis`
//!
//! `monochange_analysis` orchestrates ecosystem-specific semantic analyzers over a
//! git change frame.
//!
//! Core contracts and semantic diff types live in `monochange_core`. Ecosystem
//! crates implement analyzers. Cargo, npm, Deno, and Dart/Flutter analyzers all
//! plug into the same contract without moving ecosystem logic back into this
//! crate.
//!
//! This crate is responsible for:
//!
//! - selecting the change frame to inspect
//! - discovering affected packages
//! - loading before/after package snapshots
//! - dispatching to the right ecosystem analyzer
//! - returning structured semantic diffs for MCP and other automation

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

#[cfg(feature = "cargo")]
use monochange_cargo::semantic_analyzer as cargo_semantic_analyzer;
use monochange_config::apply_version_groups;
use monochange_config::load_workspace_configuration;
use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageAnalysisContext;
use monochange_core::PackageRecord;
use monochange_core::SemanticAnalyzer;
use monochange_core::normalize_path;
use monochange_core::relative_to_root;
#[cfg(feature = "dart")]
use monochange_dart::semantic_analyzer as dart_semantic_analyzer;
#[cfg(feature = "deno")]
use monochange_deno::semantic_analyzer as deno_semantic_analyzer;
#[cfg(feature = "npm")]
use monochange_npm::semantic_analyzer as npm_semantic_analyzer;
use serde::Deserialize;
use serde::Serialize;
use walkdir::WalkDir;

pub mod frame;

pub use frame::ChangeFrame;
pub use frame::FrameError;
pub use frame::PrEnvironment;
pub use monochange_core::AnalyzedFileChange;
pub use monochange_core::DetectionLevel;
pub use monochange_core::FileChangeKind;
pub use monochange_core::PackageSnapshot;
pub use monochange_core::PackageSnapshotFile;
pub use monochange_core::SemanticChange;
pub use monochange_core::SemanticChangeCategory;
pub use monochange_core::SemanticChangeKind;

/// Placeholder grouping configuration reserved for future lifecycle tooling.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupingThresholds {
	/// Maximum number of semantic changes to surface before callers may prefer summarization.
	pub max_detailed_changes: usize,
}

impl Default for GroupingThresholds {
	fn default() -> Self {
		Self {
			max_detailed_changes: 50,
		}
	}
}

/// Configuration for semantic change analysis.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisConfig {
	/// Requested detail level.
	pub detection_level: DetectionLevel,
	/// Reserved for future changeset-grouping helpers.
	pub thresholds: GroupingThresholds,
	/// Maximum number of package summaries a downstream caller wants to post-process.
	pub max_suggestions: usize,
}

impl Default for AnalysisConfig {
	fn default() -> Self {
		Self {
			detection_level: DetectionLevel::Signature,
			thresholds: GroupingThresholds::default(),
			max_suggestions: 10,
		}
	}
}

/// Semantic analysis for one package.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageChangeAnalysis {
	/// Preferred package id for display, using the configured package id when available.
	pub package_id: String,
	/// Underlying discovered package record id.
	pub package_record_id: String,
	/// Package manifest name.
	pub package_name: String,
	/// Package ecosystem.
	pub ecosystem: Ecosystem,
	/// Analyzer id that produced the semantic diff.
	pub analyzer_id: Option<String>,
	/// Package-relative changed files.
	pub changed_files: Vec<PathBuf>,
	/// Structured semantic diffs.
	pub semantic_changes: Vec<SemanticChange>,
	/// Non-fatal package analysis warnings.
	pub warnings: Vec<String>,
}

/// Complete semantic analysis for the requested frame.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeAnalysis {
	/// Frame analyzed.
	pub frame: ChangeFrame,
	/// Requested detail level.
	pub detection_level: DetectionLevel,
	/// Semantic diffs grouped by package id.
	pub package_analyses: BTreeMap<String, PackageChangeAnalysis>,
	/// Root-level warnings such as unmatched paths or missing analyzers.
	pub warnings: Vec<String>,
}

#[derive(Default)]
struct AnalyzerRegistry {
	analyzers: Vec<Box<dyn SemanticAnalyzer>>,
}

impl AnalyzerRegistry {
	fn new() -> Self {
		let mut registry = Self::default();

		#[cfg(feature = "cargo")]
		registry.register(Box::new(cargo_semantic_analyzer()));
		#[cfg(feature = "npm")]
		registry.register(Box::new(npm_semantic_analyzer()));
		#[cfg(feature = "deno")]
		registry.register(Box::new(deno_semantic_analyzer()));
		#[cfg(feature = "dart")]
		registry.register(Box::new(dart_semantic_analyzer()));

		registry
	}

	fn register(&mut self, analyzer: Box<dyn SemanticAnalyzer>) {
		self.analyzers.push(analyzer);
	}

	fn analyzer_for(&self, package: &PackageRecord) -> Option<&dyn SemanticAnalyzer> {
		self.analyzers
			.iter()
			.find(|analyzer| analyzer.applies_to(package))
			.map(AsRef::as_ref)
	}
}

#[derive(Debug, Clone)]
struct SnapshotTargets {
	before: SnapshotTarget,
	after: SnapshotTarget,
}

#[derive(Debug, Clone)]
enum SnapshotTarget {
	GitRevision(String),
	WorkingTree,
	GitIndex,
}

#[derive(Debug, Clone)]
struct AnalysisWorkspace {
	packages: Vec<PackageRecord>,
	warnings: Vec<String>,
}

/// Analyze changes for the requested frame.
///
/// # Errors
///
/// Returns an error if workspace discovery, git inspection, or analyzer
/// execution fails.
pub fn analyze_changes(
	repo_root: &Path,
	frame: &ChangeFrame,
	config: &AnalysisConfig,
) -> MonochangeResult<ChangeAnalysis> {
	let repo_root = normalize_path(repo_root);
	let workspace = discover_analysis_workspace(&repo_root)?;
	let changed_paths = frame.changed_files(&repo_root)?;
	let registry = AnalyzerRegistry::new();
	let targets = resolve_snapshot_targets(&repo_root, frame)?;
	let mut warnings = workspace.warnings;
	let mut package_analyses = BTreeMap::new();
	let package_inputs = package_inputs(&repo_root, &workspace.packages, &changed_paths, &targets)?;
	let matched_paths = package_inputs
		.values()
		.flat_map(|value| value.iter().map(|change| change.path.clone()))
		.collect::<BTreeSet<_>>();

	for path in changed_paths {
		if !matched_paths.contains(&path) {
			warnings.push(format!(
				"changed path `{}` did not match any configured package",
				path.display()
			));
		}
	}

	for package in &workspace.packages {
		let Some(changed_files) = package_inputs.get(&package.id) else {
			continue;
		};

		let before_snapshot = snapshot_package(&repo_root, package, &targets.before)?;
		let after_snapshot = snapshot_package(&repo_root, package, &targets.after)?;
		let package_id = preferred_package_id(package);
		let package_changed_files = changed_files
			.iter()
			.map(|file| file.package_path.clone())
			.collect::<Vec<_>>();

		let analyzer = registry.analyzer_for(package).expect(
			"semantic analyzer registry should cover all discovered ecosystems when all default features are enabled",
		);

		let context = PackageAnalysisContext {
			repo_root: &repo_root,
			package,
			detection_level: config.detection_level,
			changed_files,
			before_snapshot: Some(&before_snapshot),
			after_snapshot: Some(&after_snapshot),
		};
		let result = analyzer.analyze_package(&context)?;
		package_analyses.insert(
			package_id.clone(),
			PackageChangeAnalysis {
				package_id,
				package_record_id: package.id.clone(),
				package_name: package.name.clone(),
				ecosystem: package.ecosystem,
				analyzer_id: Some(result.analyzer_id),
				changed_files: package_changed_files,
				semantic_changes: result.semantic_changes,
				warnings: result.warnings,
			},
		);
	}

	Ok(ChangeAnalysis {
		frame: frame.clone(),
		detection_level: config.detection_level,
		package_analyses,
		warnings,
	})
}

fn preferred_package_id(package: &PackageRecord) -> String {
	package
		.metadata
		.get("config_id")
		.cloned()
		.unwrap_or_else(|| package.id.clone())
}

fn discover_analysis_workspace(root: &Path) -> MonochangeResult<AnalysisWorkspace> {
	let configuration = load_workspace_configuration(root)?;
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	#[cfg(feature = "cargo")]
	{
		let discovery = monochange_cargo::discover_cargo_packages(root)?;
		warnings.extend(discovery.warnings);
		packages.extend(discovery.packages);
	}

	#[cfg(feature = "npm")]
	{
		let discovery = monochange_npm::discover_npm_packages(root)?;
		warnings.extend(discovery.warnings);
		packages.extend(discovery.packages);
	}

	#[cfg(feature = "deno")]
	{
		let discovery = monochange_deno::discover_deno_packages(root)?;
		warnings.extend(discovery.warnings);
		packages.extend(discovery.packages);
	}

	#[cfg(feature = "dart")]
	{
		let discovery = monochange_dart::discover_dart_packages(root)?;
		warnings.extend(discovery.warnings);
		packages.extend(discovery.packages);
	}

	normalize_package_ids(root, &mut packages);
	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	let (_, version_group_warnings) = apply_version_groups(&mut packages, &configuration)?;
	warnings.extend(version_group_warnings);

	Ok(AnalysisWorkspace { packages, warnings })
}

fn normalize_package_ids(root: &Path, packages: &mut [PackageRecord]) {
	for package in packages {
		let Some(relative_manifest) = relative_to_root(root, &package.manifest_path) else {
			continue;
		};
		package.id = format!(
			"{}:{}",
			package.ecosystem.as_str(),
			relative_manifest.display()
		);
	}
}

fn package_inputs(
	repo_root: &Path,
	packages: &[PackageRecord],
	changed_paths: &[PathBuf],
	targets: &SnapshotTargets,
) -> MonochangeResult<BTreeMap<String, Vec<AnalyzedFileChange>>> {
	let mut inputs = BTreeMap::<String, Vec<AnalyzedFileChange>>::new();

	for changed_path in changed_paths {
		let package_matches = packages_for_path(repo_root, packages, changed_path);
		for package in package_matches {
			let package_root = package_root_relative(repo_root, package)
				.expect("package path matching should only return packages with a resolvable root");
			let package_path = changed_path
				.strip_prefix(&package_root)
				.map_or_else(|_| changed_path.clone(), Path::to_path_buf);
			let before_contents =
				read_text_file_from_target(repo_root, &targets.before, changed_path)?;
			let after_contents =
				read_text_file_from_target(repo_root, &targets.after, changed_path)?;
			let kind = classify_file_change(before_contents.as_ref(), after_contents.as_ref());
			inputs
				.entry(package.id.clone())
				.or_default()
				.push(AnalyzedFileChange {
					path: changed_path.clone(),
					package_path,
					kind,
					before_contents,
					after_contents,
				});
		}
	}

	for changes in inputs.values_mut() {
		changes.sort_by(|left, right| left.package_path.cmp(&right.package_path));
	}

	Ok(inputs)
}

fn packages_for_path<'a>(
	repo_root: &Path,
	packages: &'a [PackageRecord],
	changed_path: &Path,
) -> Vec<&'a PackageRecord> {
	let mut matches = packages
		.iter()
		.filter_map(|package| {
			let package_root = package_root_relative(repo_root, package)?;
			(changed_path == package_root || changed_path.starts_with(&package_root))
				.then_some((package_root.components().count(), package))
		})
		.collect::<Vec<_>>();

	let Some(longest_match) = matches.iter().map(|(depth, _)| *depth).max() else {
		return Vec::new();
	};

	matches.retain(|(depth, _)| *depth == longest_match);
	matches.into_iter().map(|(_, package)| package).collect()
}

fn package_root_relative(repo_root: &Path, package: &PackageRecord) -> Option<PathBuf> {
	let package_root = package
		.manifest_path
		.parent()
		.unwrap_or(&package.workspace_root);
	relative_to_root(repo_root, package_root)
}

fn classify_file_change(before: Option<&String>, after: Option<&String>) -> FileChangeKind {
	match (before, after) {
		(None, Some(_)) => FileChangeKind::Added,
		(Some(_), None) => FileChangeKind::Deleted,
		_ => FileChangeKind::Modified,
	}
}

fn resolve_snapshot_targets(
	repo_root: &Path,
	frame: &ChangeFrame,
) -> Result<SnapshotTargets, FrameError> {
	match frame {
		ChangeFrame::WorkingDirectory => {
			Ok(SnapshotTargets {
				before: SnapshotTarget::GitRevision("HEAD".to_string()),
				after: SnapshotTarget::WorkingTree,
			})
		}
		ChangeFrame::StagedOnly => {
			Ok(SnapshotTargets {
				before: SnapshotTarget::GitRevision("HEAD".to_string()),
				after: SnapshotTarget::GitIndex,
			})
		}
		ChangeFrame::BranchRange { base, head } | ChangeFrame::CustomRange { base, head } => {
			Ok(SnapshotTargets {
				before: SnapshotTarget::GitRevision(base.clone()),
				after: SnapshotTarget::GitRevision(head.clone()),
			})
		}
		ChangeFrame::PullRequest { target, pr_branch } => {
			Ok(SnapshotTargets {
				before: SnapshotTarget::GitRevision(git_merge_base(repo_root, target, pr_branch)?),
				after: SnapshotTarget::GitRevision(pr_branch.clone()),
			})
		}
	}
}

fn git_merge_base(repo_root: &Path, base: &str, head: &str) -> Result<String, FrameError> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["merge-base", base, head])
		.output()
		.map_err(|error| FrameError::Git(format!("failed to run git merge-base: {error}")))?;

	if !output.status.success() {
		return Err(FrameError::Git(format!(
			"git merge-base {base} {head} failed"
		)));
	}

	String::from_utf8(output.stdout)
		.map_err(|error| FrameError::Git(format!("invalid utf-8 from git merge-base: {error}")))
		.map(|value| value.trim().to_string())
}

fn snapshot_package(
	repo_root: &Path,
	package: &PackageRecord,
	target: &SnapshotTarget,
) -> MonochangeResult<PackageSnapshot> {
	let package_root = package_root_relative(repo_root, package).ok_or_else(|| {
		MonochangeError::Discovery(format!(
			"failed to resolve package root for `{}`",
			package.id
		))
	})?;
	let label = snapshot_label(target);
	let files = match target {
		SnapshotTarget::WorkingTree => snapshot_files_from_working_tree(repo_root, &package_root)?,
		SnapshotTarget::GitRevision(revision) => {
			snapshot_files_from_revision(repo_root, &package_root, revision)?
		}
		SnapshotTarget::GitIndex => snapshot_files_from_index(repo_root, &package_root)?,
	};

	Ok(PackageSnapshot { label, files })
}

fn snapshot_label(target: &SnapshotTarget) -> String {
	match target {
		SnapshotTarget::GitRevision(revision) => revision.clone(),
		SnapshotTarget::WorkingTree => "working_tree".to_string(),
		SnapshotTarget::GitIndex => "index".to_string(),
	}
}

#[allow(clippy::unnecessary_wraps)]
fn snapshot_files_from_working_tree(
	repo_root: &Path,
	package_root: &Path,
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	let absolute_root = repo_root.join(package_root);
	if !absolute_root.exists() {
		return Ok(Vec::new());
	}

	let mut files = Vec::new();
	for entry in WalkDir::new(&absolute_root)
		.into_iter()
		.filter_map(Result::ok)
	{
		let entry_path = entry.path();
		if entry.file_type().is_dir() && should_skip_directory(entry_path) {
			continue;
		}
		if !entry.file_type().is_file() {
			continue;
		}
		let relative_to_package = entry_path
			.strip_prefix(&absolute_root)
			.unwrap_or(entry_path);
		let Some(contents) = read_working_tree_text(entry_path) else {
			continue;
		};
		files.push(PackageSnapshotFile {
			path: relative_to_package.to_path_buf(),
			contents,
		});
	}

	files.sort_by(|left, right| left.path.cmp(&right.path));
	Ok(files)
}

fn should_skip_directory(path: &Path) -> bool {
	path.file_name().is_some_and(|name| {
		matches!(
			name.to_string_lossy().as_ref(),
			".git" | "target" | "node_modules" | "dist" | "build"
		)
	})
}

fn read_working_tree_text(path: &Path) -> Option<String> {
	let metadata = fs::metadata(path).ok()?;
	(metadata.len() <= 256 * 1024).then_some(())?;
	fs::read_to_string(path).ok()
}

fn snapshot_files_from_revision(
	repo_root: &Path,
	package_root: &Path,
	revision: &str,
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	let package_root_text = package_root.to_string_lossy().to_string();
	let args = [
		"ls-tree",
		"-r",
		"--name-only",
		revision,
		"--",
		package_root_text.as_str(),
	];
	let paths = git_list_files(repo_root, &args)?;
	build_snapshot_files_from_paths(
		repo_root,
		package_root,
		&SnapshotTarget::GitRevision(revision.to_string()),
		&paths,
	)
}

fn snapshot_files_from_index(
	repo_root: &Path,
	package_root: &Path,
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	let package_root_text = package_root.to_string_lossy().to_string();
	let args = ["ls-files", "--cached", "--", package_root_text.as_str()];
	let paths = git_list_files(repo_root, &args)?;
	build_snapshot_files_from_paths(repo_root, package_root, &SnapshotTarget::GitIndex, &paths)
}

fn build_snapshot_files_from_paths(
	repo_root: &Path,
	package_root: &Path,
	target: &SnapshotTarget,
	paths: &[PathBuf],
) -> MonochangeResult<Vec<PackageSnapshotFile>> {
	let mut files = Vec::new();
	for path in paths {
		let Some(relative_to_package) = path.strip_prefix(package_root).ok().map(Path::to_path_buf)
		else {
			continue;
		};
		let Some(contents) = read_text_file_from_target(repo_root, target, path)? else {
			continue;
		};
		files.push(PackageSnapshotFile {
			path: relative_to_package,
			contents,
		});
	}
	files.sort_by(|left, right| left.path.cmp(&right.path));
	Ok(files)
}

fn git_list_files(repo_root: &Path, args: &[&str]) -> MonochangeResult<Vec<PathBuf>> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(args)
		.output()
		.map_err(|error| {
			MonochangeError::Discovery(format!("failed to run git {args:?}: {error}"))
		})?;

	if !output.status.success() {
		return Err(MonochangeError::Discovery(format!(
			"git {:?} failed with status {}",
			args, output.status
		)));
	}

	let stdout = String::from_utf8(output.stdout).map_err(|error| {
		MonochangeError::Discovery(format!("git {args:?} returned invalid utf-8: {error}"))
	})?;

	Ok(stdout
		.lines()
		.filter(|line| !line.is_empty())
		.map(PathBuf::from)
		.collect())
}

fn read_text_file_from_target(
	repo_root: &Path,
	target: &SnapshotTarget,
	path: &Path,
) -> MonochangeResult<Option<String>> {
	match target {
		SnapshotTarget::WorkingTree => Ok(read_working_tree_text(&repo_root.join(path))),
		SnapshotTarget::GitRevision(revision) => {
			read_text_file_from_git_object(
				repo_root,
				&format!("{revision}:{}", path.to_string_lossy()),
			)
		}
		SnapshotTarget::GitIndex => {
			read_text_file_from_git_object(repo_root, &format!(":{}", path.to_string_lossy()))
		}
	}
}

fn read_text_file_from_git_object(
	repo_root: &Path,
	object: &str,
) -> MonochangeResult<Option<String>> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["show", object])
		.output()
		.map_err(|error| {
			MonochangeError::Discovery(format!("failed to run git show {object}: {error}"))
		})?;

	if !output.status.success() {
		return Ok(None);
	}

	if output.stdout.len() > 256 * 1024 {
		return Ok(None);
	}

	Ok(String::from_utf8(output.stdout).ok())
}

#[cfg(test)]
mod tests {
	use std::fs;

	use monochange_test_helpers::copy_directory;
	use monochange_test_helpers::git;
	use monochange_test_helpers::git_output_trimmed;
	use tempfile::tempdir;

	use super::*;

	fn fixture_path(relative: &str) -> PathBuf {
		monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
	}

	fn setup_analysis_repo(relative: &str) -> tempfile::TempDir {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		copy_directory(&fixture_path(relative), tempdir.path());
		git(tempdir.path(), &["init"]);
		git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
		git(
			tempdir.path(),
			&["config", "user.email", "monochange-tests@example.com"],
		);
		git(tempdir.path(), &["add", "."]);
		git(tempdir.path(), &["commit", "-m", "base"]);
		git(tempdir.path(), &["branch", "-M", "main"]);
		tempdir
	}

	#[test]
	fn preferred_package_id_uses_config_id_when_available() {
		let mut package = PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			PathBuf::from("/repo/crates/core/Cargo.toml"),
			PathBuf::from("/repo"),
			None,
			monochange_core::PublishState::Public,
		);
		package
			.metadata
			.insert("config_id".to_string(), "core".to_string());

		assert_eq!(preferred_package_id(&package), "core");
	}

	#[test]
	fn classify_file_change_uses_presence_of_before_and_after_contents() {
		assert_eq!(
			classify_file_change(None, Some(&"after".to_string())),
			FileChangeKind::Added
		);
		assert_eq!(
			classify_file_change(Some(&"before".to_string()), None),
			FileChangeKind::Deleted
		);
		assert_eq!(
			classify_file_change(Some(&"before".to_string()), Some(&"after".to_string())),
			FileChangeKind::Modified
		);
	}

	#[test]
	fn normalize_package_ids_skips_manifests_outside_the_repo_root() {
		let root = PathBuf::from("/repo");
		let mut packages = vec![PackageRecord {
			id: "core".to_string(),
			name: "core".to_string(),
			ecosystem: Ecosystem::Cargo,
			manifest_path: PathBuf::from("/outside/Cargo.toml"),
			workspace_root: root.clone(),
			current_version: None,
			publish_state: monochange_core::PublishState::Public,
			version_group_id: None,
			metadata: BTreeMap::new(),
			declared_dependencies: Vec::new(),
		}];

		normalize_package_ids(&root, &mut packages);

		assert_eq!(
			packages
				.first()
				.unwrap_or_else(|| panic!("expected one normalized package"))
				.id,
			"core"
		);
	}

	#[test]
	fn packages_for_path_prefers_the_longest_matching_package_root() {
		let root = PathBuf::from("/repo");
		let packages = vec![
			PackageRecord::new(
				Ecosystem::Npm,
				"workspace",
				root.join("packages/package.json"),
				root.clone(),
				None,
				monochange_core::PublishState::Public,
			),
			PackageRecord::new(
				Ecosystem::Npm,
				"web",
				root.join("packages/web/package.json"),
				root.clone(),
				None,
				monochange_core::PublishState::Public,
			),
		];

		let matched = packages_for_path(&root, &packages, Path::new("packages/web/src/index.ts"));

		assert_eq!(matched.len(), 1);
		assert_eq!(
			matched
				.first()
				.unwrap_or_else(|| panic!("expected one matched package"))
				.name,
			"web"
		);
		assert!(packages_for_path(&root, &packages, Path::new("README.md")).is_empty());
	}

	#[test]
	fn discover_analysis_workspace_collects_multi_ecosystem_packages() {
		let tempdir = monochange_test_helpers::fs::setup_fixture_from(
			env!("CARGO_MANIFEST_DIR"),
			"analysis/multi-ecosystem-diff/before",
		);

		let workspace = discover_analysis_workspace(tempdir.path())
			.unwrap_or_else(|error| panic!("discover analysis workspace: {error}"));

		assert_eq!(workspace.packages.len(), 4);
		assert!(
			workspace
				.packages
				.iter()
				.any(|package| package.name == "core")
		);
		assert!(
			workspace
				.packages
				.iter()
				.any(|package| package.name == "@acme/web")
		);
		assert!(
			workspace
				.packages
				.iter()
				.any(|package| package.name == "runtime")
		);
		assert!(
			workspace
				.packages
				.iter()
				.any(|package| package.name == "mobile_app")
		);
	}

	#[test]
	fn analyze_changes_reports_unmatched_paths_as_warnings() {
		let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
		let readme = tempdir.path().join("README.md");
		fs::write(&readme, "base\n").unwrap_or_else(|error| panic!("write README: {error}"));
		git(tempdir.path(), &["add", "README.md"]);
		git(tempdir.path(), &["commit", "-m", "add readme"]);
		fs::write(&readme, "updated\n").unwrap_or_else(|error| panic!("update README: {error}"));

		let analysis = analyze_changes(
			tempdir.path(),
			&ChangeFrame::WorkingDirectory,
			&AnalysisConfig::default(),
		)
		.unwrap_or_else(|error| panic!("analyze changes: {error}"));

		assert!(
			analysis
				.warnings
				.iter()
				.any(|warning| warning.contains("did not match any configured package"))
		);
	}

	#[test]
	fn snapshot_helpers_cover_error_paths_and_filtered_content() {
		let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
		let root = tempdir.path().to_path_buf();
		let head = git_output_trimmed(&root, &["rev-parse", "HEAD"]);
		let large_file = root.join("crates/core/src/large.rs");
		fs::write(&large_file, "a".repeat(300_000))
			.unwrap_or_else(|error| panic!("write large file: {error}"));
		git(&root, &["add", "."]);
		git(&root, &["commit", "-m", "add large file"]);

		assert!(read_working_tree_text(&large_file).is_none());
		assert!(
			read_text_file_from_git_object(&root, "HEAD:missing.rs")
				.unwrap_or_else(|error| panic!("read missing git object: {error}"))
				.is_none()
		);
		assert!(
			read_text_file_from_git_object(&root, "HEAD:crates/core/src/large.rs")
				.unwrap_or_else(|error| panic!("read large git object: {error}"))
				.is_none()
		);
		assert!(should_skip_directory(Path::new("target")));
		assert!(!should_skip_directory(Path::new("src")));
		assert_eq!(snapshot_label(&SnapshotTarget::WorkingTree), "working_tree");
		assert_eq!(snapshot_label(&SnapshotTarget::GitIndex), "index");
		assert_eq!(
			snapshot_label(&SnapshotTarget::GitRevision(head.clone())),
			head
		);

		let outside_package = PackageRecord::new(
			Ecosystem::Cargo,
			"outside",
			PathBuf::from("/outside/Cargo.toml"),
			PathBuf::from("/outside"),
			None,
			monochange_core::PublishState::Public,
		);
		assert!(snapshot_package(&root, &outside_package, &SnapshotTarget::WorkingTree).is_err());

		let not_a_repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let git_list_error = git_list_files(not_a_repo.path(), &["ls-files"])
			.unwrap_err()
			.render();
		assert!(git_list_error.contains("git"));
		let missing_repo = root.join("missing-repo");
		assert!(read_text_file_from_git_object(&missing_repo, "HEAD:file.rs").is_err());
	}

	#[test]
	fn snapshot_target_helpers_cover_branch_range_pr_and_index_paths() {
		let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
		let root = tempdir.path().to_path_buf();
		git(&root, &["branch", "feature"]);

		let staged_targets = resolve_snapshot_targets(&root, &ChangeFrame::StagedOnly)
			.unwrap_or_else(|error| panic!("resolve staged targets: {error}"));
		assert!(matches!(staged_targets.after, SnapshotTarget::GitIndex));

		let range_targets = resolve_snapshot_targets(
			&root,
			&ChangeFrame::BranchRange {
				base: "main".to_string(),
				head: "feature".to_string(),
			},
		)
		.unwrap_or_else(|error| panic!("resolve branch targets: {error}"));
		assert!(matches!(
			range_targets.before,
			SnapshotTarget::GitRevision(_)
		));
		assert!(matches!(
			range_targets.after,
			SnapshotTarget::GitRevision(_)
		));

		let pr_targets = resolve_snapshot_targets(
			&root,
			&ChangeFrame::PullRequest {
				target: "main".to_string(),
				pr_branch: "feature".to_string(),
			},
		)
		.unwrap_or_else(|error| panic!("resolve pr targets: {error}"));
		assert!(matches!(pr_targets.before, SnapshotTarget::GitRevision(_)));

		let package_root = Path::new("crates/core");
		let working_files = snapshot_files_from_working_tree(&root, package_root)
			.unwrap_or_else(|error| panic!("working tree snapshot: {error}"));
		assert!(!working_files.is_empty());
		assert!(
			snapshot_files_from_working_tree(&root, Path::new("missing"))
				.unwrap()
				.is_empty()
		);

		fs::write(root.join("crates/core/src/lib.rs"), "pub struct Changed;\n")
			.unwrap_or_else(|error| panic!("rewrite lib.rs: {error}"));
		git(&root, &["add", "crates/core/src/lib.rs"]);

		let index_files = snapshot_files_from_index(&root, package_root)
			.unwrap_or_else(|error| panic!("index snapshot: {error}"));
		assert!(
			index_files
				.iter()
				.any(|file| file.path == Path::new("src/lib.rs"))
		);
		assert!(
			read_text_file_from_target(
				&root,
				&SnapshotTarget::GitIndex,
				Path::new("crates/core/src/lib.rs")
			)
			.unwrap_or_else(|error| panic!("read index target: {error}"))
			.is_some()
		);
		assert!(
			read_text_file_from_target(
				&root,
				&SnapshotTarget::GitRevision(git_output_trimmed(&root, &["rev-parse", "HEAD"])),
				Path::new("crates/core/src/lib.rs"),
			)
			.unwrap_or_else(|error| panic!("read revision target: {error}"))
			.is_some()
		);

		let built = build_snapshot_files_from_paths(
			&root,
			package_root,
			&SnapshotTarget::GitIndex,
			&[
				PathBuf::from("outside.txt"),
				PathBuf::from("crates/core/src/lib.rs"),
				PathBuf::from("crates/core/src/missing.rs"),
			],
		)
		.unwrap_or_else(|error| panic!("build snapshot files: {error}"));
		assert_eq!(built.len(), 1);
		assert_eq!(
			built
				.first()
				.unwrap_or_else(|| panic!("expected one built snapshot file"))
				.path,
			PathBuf::from("src/lib.rs")
		);
	}

	#[test]
	fn git_and_snapshot_helpers_cover_remaining_error_paths() {
		let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
		let root = tempdir.path().to_path_buf();
		let package_root = Path::new("crates/core");
		let large_file = root.join("crates/core/src/large.rs");
		fs::write(&large_file, "a".repeat(300_000))
			.unwrap_or_else(|error| panic!("write large file: {error}"));

		let working_files = snapshot_files_from_working_tree(&root, package_root)
			.unwrap_or_else(|error| panic!("working tree snapshot: {error}"));
		assert!(
			!working_files
				.iter()
				.any(|file| file.path == Path::new("src/large.rs"))
		);

		let merge_base_error = git_merge_base(&root, "main", "missing-branch")
			.unwrap_err()
			.to_string();
		assert!(merge_base_error.contains("git merge-base main missing-branch failed"));

		fs::write(root.join("crates/core/src/lib.rs"), "pub struct Indexed;\n")
			.unwrap_or_else(|error| panic!("rewrite lib.rs: {error}"));
		git(&root, &["add", "crates/core/src/lib.rs"]);

		let package = discover_analysis_workspace(&root)
			.unwrap_or_else(|error| panic!("discover analysis workspace: {error}"))
			.packages
			.into_iter()
			.find(|package| package.name == "core")
			.unwrap_or_else(|| panic!("missing core package"));
		let index_snapshot = snapshot_package(&root, &package, &SnapshotTarget::GitIndex)
			.unwrap_or_else(|error| panic!("index package snapshot: {error}"));
		assert!(
			index_snapshot
				.files
				.iter()
				.any(|file| file.path == Path::new("src/lib.rs"))
		);

		let spawn_error = git_list_files(&root.join("missing-repo"), &["ls-files"])
			.unwrap_err()
			.render();
		assert!(spawn_error.contains("failed to run git [\"ls-files\"]"));

		let binary_file = root.join("crates/core/src/invalid.bin");
		fs::write(&binary_file, [0_u8, 159, 146, 150])
			.unwrap_or_else(|error| panic!("write invalid binary file: {error}"));
		git(&root, &["add", "crates/core/src/invalid.bin"]);
		git(&root, &["commit", "-m", "add invalid binary file"]);

		let utf8_error = git_list_files(&root, &["show", "HEAD:crates/core/src/invalid.bin"])
			.unwrap_err()
			.render();
		assert!(utf8_error.contains("invalid utf-8"));
	}
}
