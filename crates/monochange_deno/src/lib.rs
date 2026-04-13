#![forbid(clippy::indexing_slicing)]

//! # `monochange_deno`
//!
//! <!-- {=monochangeDenoCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_deno` discovers Deno packages and workspace members for the shared planner.
//!
//! Reach for this crate when you need to scan `deno.json` or `deno.jsonc` files, expand Deno workspaces, and normalize Deno dependencies into `monochange_core` records.
//!
//! ## Why use it?
//!
//! - discover Deno workspaces and standalone packages with one adapter
//! - normalize manifest and dependency data for cross-ecosystem release planning
//! - include Deno-specific import and dependency extraction in the shared graph
//!
//! ## Best for
//!
//! - scanning Deno repos without adopting the full workspace CLI
//! - turning `deno.json` metadata into shared package and dependency records
//! - mixing Deno packages into a broader cross-ecosystem monorepo plan
//!
//! ## Public entry points
//!
//! - `discover_deno_packages(root)` discovers Deno workspaces and standalone packages
//! - `DenoAdapter` exposes the shared adapter interface
//!
//! ## Scope
//!
//! - `deno.json` and `deno.jsonc`
//! - workspace glob expansion
//! - normalized dependency and import extraction
//! <!-- {/monochangeDenoCrateDocs} -->

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use glob::glob;
use monochange_core::AdapterDiscovery;
use monochange_core::DependencyKind;
use monochange_core::DiscoveryPathFilter;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::LockfileCommandExecution;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageDependency;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::normalize_path;
use semver::Version;
use serde_json::Value;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const DENO_MANIFEST_FILES: [&str; 2] = ["deno.json", "deno.jsonc"];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DenoVersionedFileKind {
	Manifest,
	Lock,
}

pub fn supported_versioned_file_kind(path: &Path) -> Option<DenoVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();
	match file_name {
		"deno.lock" => Some(DenoVersionedFileKind::Lock),
		_ if path.extension().and_then(|ext| ext.to_str()) == Some("json")
			|| path.extension().and_then(|ext| ext.to_str()) == Some("jsonc") =>
		{
			Some(DenoVersionedFileKind::Manifest)
		}
		_ => None,
	}
}

fn rewrite_dependency_reference(text: &str, package_name: &str, version: &str) -> String {
	let mut updated = text.to_string();
	for prefix in [format!("npm:{package_name}@"), format!("{package_name}@")] {
		let mut cursor = 0usize;
		while let Some(found) = updated[cursor..].find(&prefix) {
			let start = cursor + found + prefix.len();
			let end = updated[start..]
				.char_indices()
				.find_map(|(index, ch)| {
					(!ch.is_ascii_alphanumeric() && ch != '.' && ch != '-' && ch != '+')
						.then_some(start + index)
				})
				.unwrap_or(updated.len());
			updated.replace_range(start..end, version);
			cursor = start + version.len();
		}
	}
	updated
}

pub fn update_lockfile(value: &mut Value, raw_versions: &BTreeMap<String, String>) {
	let Ok(mut rendered) = serde_json::to_string(value) else {
		return;
	};
	for (package_name, version) in raw_versions {
		rendered = rewrite_dependency_reference(&rendered, package_name, version);
	}
	if let Ok(updated) = serde_json::from_str::<Value>(&rendered) {
		*value = updated;
	}
}

pub fn discover_lockfiles(package: &PackageRecord) -> Vec<PathBuf> {
	let manifest_dir = package
		.manifest_path
		.parent()
		.map_or_else(|| package.workspace_root.clone(), Path::to_path_buf);
	let scope = if manifest_dir == package.workspace_root {
		manifest_dir.clone()
	} else {
		package.workspace_root.clone()
	};
	let mut discovered = [scope.join("deno.lock")]
		.into_iter()
		.filter(|path| path.exists())
		.collect::<Vec<_>>();
	if discovered.is_empty() && scope != manifest_dir {
		discovered.extend(
			[manifest_dir.join("deno.lock")]
				.into_iter()
				.filter(|path| path.exists()),
		);
	}
	discovered
}

pub fn default_lockfile_commands(_package: &PackageRecord) -> Vec<LockfileCommandExecution> {
	Vec::new()
}

pub struct DenoAdapter;

#[must_use]
pub const fn adapter() -> DenoAdapter {
	DenoAdapter
}

impl EcosystemAdapter for DenoAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Deno
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_deno_packages(root)
	}
}

#[tracing::instrument(skip_all)]
#[must_use = "the discovery result must be checked"]
pub fn discover_deno_packages(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let workspace_manifests = find_workspace_manifests(root);
	let mut included_manifests = HashSet::new();
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	for workspace_manifest in workspace_manifests {
		let (workspace_packages, workspace_warnings) =
			discover_workspace_packages(&workspace_manifest)?;
		warnings.extend(workspace_warnings);
		for package in workspace_packages {
			included_manifests.insert(package.manifest_path.clone());
			packages.push(package);
		}
	}

	for manifest_path in find_all_manifests(root) {
		if included_manifests.contains(&manifest_path) {
			continue;
		}

		if let Some(package) =
			parse_manifest(&manifest_path, manifest_path.parent().unwrap_or(root))?
		{
			packages.push(package);
		}
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);
	tracing::debug!(packages = packages.len(), "discovered deno packages");

	Ok(AdapterDiscovery { packages, warnings })
}

/// Load one explicitly configured Deno package without scanning unrelated manifests.
#[must_use = "the package result must be checked"]
pub fn load_configured_deno_package(
	root: &Path,
	package_path: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let manifest_path = if package_path.is_file() {
		package_path.to_path_buf()
	} else {
		DENO_MANIFEST_FILES
			.into_iter()
			.map(|name| package_path.join(name))
			.find(|candidate| candidate.exists())
			.unwrap_or_else(|| package_path.join(DENO_MANIFEST_FILES[0]))
	};
	parse_manifest(&manifest_path, manifest_path.parent().unwrap_or(root))
}

fn find_workspace_manifests(root: &Path) -> Vec<PathBuf> {
	let mut manifests = find_all_manifests(root)
		.into_iter()
		.filter(|manifest_path| has_workspace_section(manifest_path).unwrap_or(false))
		.collect::<Vec<_>>();
	manifests.sort();
	manifests
}

fn discover_workspace_packages(
	workspace_manifest: &Path,
) -> MonochangeResult<(Vec<PackageRecord>, Vec<String>)> {
	let parsed = parse_json_manifest(workspace_manifest)?;
	let workspace_root = workspace_manifest
		.parent()
		.unwrap_or_else(|| Path::new("."));
	let patterns = parsed
		.get("workspace")
		.and_then(Value::as_array)
		.map(|items| {
			items
				.iter()
				.filter_map(Value::as_str)
				.map(ToString::to_string)
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();
	let mut warnings = Vec::new();
	let member_manifests = expand_workspace_patterns(workspace_root, &patterns, &mut warnings);
	let mut packages = Vec::new();

	for member_manifest in member_manifests {
		if let Some(package) = parse_manifest(&member_manifest, workspace_root)? {
			packages.push(package);
		}
	}

	Ok((packages, warnings))
}

fn expand_workspace_patterns(
	root: &Path,
	patterns: &[String],
	warnings: &mut Vec<String>,
) -> BTreeSet<PathBuf> {
	let filter = DiscoveryPathFilter::new(root);
	let mut manifests = BTreeSet::new();
	for pattern in patterns {
		let joined_pattern = root.join(pattern).to_string_lossy().to_string();
		let matches = glob(&joined_pattern)
			.into_iter()
			.flat_map(|paths| paths.filter_map(Result::ok))
			.map(|path| normalize_path(&path))
			.filter(|path| filter.allows(path))
			.collect::<Vec<_>>();
		if matches.is_empty() {
			warnings.push(format!(
				"deno workspace pattern `{pattern}` under {} matched no packages",
				root.display()
			));
		}

		for matched_path in matches {
			for manifest_name in DENO_MANIFEST_FILES {
				let manifest_path = if matched_path.is_dir() {
					matched_path.join(manifest_name)
				} else {
					matched_path.clone()
				};
				if manifest_path.file_name().and_then(|name| name.to_str()) == Some(manifest_name)
					&& manifest_path.exists()
					&& filter.allows(&manifest_path)
				{
					manifests.insert(manifest_path);
				}
			}
		}
	}
	manifests
}

fn parse_manifest(
	manifest_path: &Path,
	workspace_root: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let parsed = parse_json_manifest(manifest_path)?;
	let Some(name) = parsed.get("name").and_then(Value::as_str) else {
		return Ok(None);
	};
	let version = parsed
		.get("version")
		.and_then(Value::as_str)
		.and_then(|value| Version::parse(value).ok());

	let mut package = PackageRecord::new(
		Ecosystem::Deno,
		name,
		manifest_path.to_path_buf(),
		workspace_root.to_path_buf(),
		version,
		PublishState::Public,
	);
	package.declared_dependencies = ["dependencies", "imports"]
		.into_iter()
		.flat_map(|section| parse_dependency_map(&parsed, section))
		.collect();
	Ok(Some(package))
}

fn parse_dependency_map(parsed: &Value, section: &str) -> Vec<PackageDependency> {
	parsed
		.get(section)
		.and_then(Value::as_object)
		.map(|dependencies| {
			dependencies
				.iter()
				.filter_map(|(name, value)| {
					value.as_str().map(|constraint| {
						PackageDependency {
							name: name.clone(),
							kind: DependencyKind::Runtime,
							version_constraint: Some(constraint.to_string()),
							optional: false,
						}
					})
				})
				.collect::<Vec<_>>()
		})
		.unwrap_or_default()
}

fn has_workspace_section(manifest_path: &Path) -> MonochangeResult<bool> {
	let parsed = parse_json_manifest(manifest_path)?;
	Ok(parsed
		.get("workspace")
		.and_then(Value::as_array)
		.is_some_and(|items| !items.is_empty()))
}

fn parse_json_manifest(manifest_path: &Path) -> MonochangeResult<Value> {
	let contents = fs::read_to_string(manifest_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			manifest_path.display()
		))
	})?;
	let normalized = monochange_core::strip_json_comments(&contents);
	serde_json::from_str::<Value>(&normalized).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			manifest_path.display()
		))
	})
}

fn find_all_manifests(root: &Path) -> Vec<PathBuf> {
	let filter = DiscoveryPathFilter::new(root);
	WalkDir::new(root)
		.into_iter()
		.filter_entry(|entry| filter.should_descend(entry.path()))
		.filter_map(Result::ok)
		.filter(|entry| DENO_MANIFEST_FILES.contains(&entry.file_name().to_string_lossy().as_ref()))
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

#[cfg(test)]
mod __tests;
