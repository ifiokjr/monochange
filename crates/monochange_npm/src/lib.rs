#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_npm`
//!
//! <!-- {=monochangeNpmCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_npm` discovers npm-family packages and normalizes them for shared planning.
//!
//! Reach for this crate when you want one adapter for npm, pnpm, and Bun workspaces that emits `monochange_core` package and dependency records.
//!
//! ## Why use it?
//!
//! - discover several JavaScript package-manager layouts with one crate
//! - normalize workspace metadata into the same graph used by the rest of `monochange`
//! - capture dependency edges from `package.json` and `pnpm-workspace.yaml`
//!
//! ## Best for
//!
//! - scanning JavaScript or TypeScript monorepos into normalized package records
//! - supporting npm, pnpm, and Bun with one discovery surface
//! - feeding JS workspace topology into shared planning code
//!
//! ## Public entry points
//!
//! - `discover_npm_packages(root)` discovers npm, pnpm, and Bun workspaces plus standalone packages
//! - `NpmAdapter` exposes the shared adapter interface
//!
//! ## Scope
//!
//! - `package.json` workspaces
//! - `pnpm-workspace.yaml`
//! - Bun lockfile detection
//! - normalized dependency extraction
//! <!-- {/monochangeNpmCrateDocs} -->

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use glob::glob;
use monochange_core::normalize_path;
use monochange_core::AdapterDiscovery;
use monochange_core::DependencyKind;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageDependency;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use serde_json::Value;
use serde_yaml_ng::Value as YamlValue;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const PACKAGE_JSON_FILE: &str = "package.json";
pub const PNPM_WORKSPACE_FILE: &str = "pnpm-workspace.yaml";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NpmVersionedFileKind {
	Manifest,
	PackageLock,
	PnpmLock,
	BunLock,
	BunLockBinary,
}

pub fn supported_versioned_file_kind(path: &Path) -> Option<NpmVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();
	match file_name {
		"package-lock.json" => Some(NpmVersionedFileKind::PackageLock),
		"pnpm-lock.yaml" => Some(NpmVersionedFileKind::PnpmLock),
		"bun.lock" => Some(NpmVersionedFileKind::BunLock),
		"bun.lockb" => Some(NpmVersionedFileKind::BunLockBinary),
		_ if path.extension().and_then(|ext| ext.to_str()) == Some("json") => {
			Some(NpmVersionedFileKind::Manifest)
		}
		_ => None,
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
	let candidate_names = [
		"pnpm-lock.yaml",
		"package-lock.json",
		"bun.lock",
		"bun.lockb",
	];
	let mut discovered = candidate_names
		.iter()
		.map(|name| scope.join(name))
		.filter(|path| path.exists())
		.collect::<Vec<_>>();
	if discovered.is_empty() && scope != manifest_dir {
		discovered.extend(
			candidate_names
				.iter()
				.map(|name| manifest_dir.join(name))
				.filter(|path| path.exists()),
		);
	}
	discovered
}

pub fn update_json_dependency_fields(
	value: &mut Value,
	fields: &[&str],
	versioned_deps: &BTreeMap<String, String>,
) {
	for field in fields {
		if let Some(section) = value.get_mut(*field).and_then(Value::as_object_mut) {
			for (dep_name, dep_version) in versioned_deps {
				if section.contains_key(dep_name) {
					section.insert(dep_name.clone(), Value::String(dep_version.clone()));
				}
			}
		}
	}
}

pub fn update_package_lock(
	value: &mut Value,
	package_paths_by_name: &BTreeMap<String, PathBuf>,
	raw_versions: &BTreeMap<String, String>,
) {
	if let Some(root_name) = value.get("name").and_then(Value::as_str) {
		if let Some(version) = raw_versions.get(root_name) {
			if let Some(obj) = value.as_object_mut() {
				obj.insert("version".to_string(), Value::String(version.clone()));
			}
		}
	}
	if let Some(packages) = value.get_mut("packages").and_then(Value::as_object_mut) {
		for (entry_path, entry_value) in packages {
			let Some(entry_object) = entry_value.as_object_mut() else {
				continue;
			};
			if let Some(name) = entry_object.get("name").and_then(Value::as_str) {
				if let Some(version) = raw_versions.get(name) {
					entry_object.insert("version".to_string(), Value::String(version.clone()));
				}
				continue;
			}
			for (name, package_dir) in package_paths_by_name {
				if entry_path == &package_dir.to_string_lossy() {
					if let Some(version) = raw_versions.get(name) {
						entry_object.insert("version".to_string(), Value::String(version.clone()));
					}
				}
			}
		}
	}
	if let Some(dependencies) = value.get_mut("dependencies").and_then(Value::as_object_mut) {
		for (name, version) in raw_versions {
			if let Some(entry) = dependencies.get_mut(name).and_then(Value::as_object_mut) {
				entry.insert("version".to_string(), Value::String(version.clone()));
			}
		}
	}
}

pub fn update_pnpm_lock(
	mapping: &mut serde_yaml_ng::Mapping,
	raw_versions: &BTreeMap<String, String>,
) {
	for section_name in ["importers", "packages", "snapshots"] {
		let Some(serde_yaml_ng::Value::Mapping(section)) =
			mapping.get_mut(serde_yaml_ng::Value::String(section_name.to_string()))
		else {
			continue;
		};
		for value in section.values_mut() {
			let Some(entry_mapping) = value.as_mapping_mut() else {
				continue;
			};
			for dependency_field in [
				"dependencies",
				"devDependencies",
				"optionalDependencies",
				"peerDependencies",
			] {
				let Some(serde_yaml_ng::Value::Mapping(dependencies)) = entry_mapping
					.get_mut(serde_yaml_ng::Value::String(dependency_field.to_string()))
				else {
					continue;
				};
				for (name, version) in raw_versions {
					let key = serde_yaml_ng::Value::String(name.clone());
					let Some(entry) = dependencies.get_mut(&key) else {
						continue;
					};
					if let Some(text) = entry.as_str() {
						if !text.starts_with("link:") && !text.starts_with("workspace:") {
							*entry = serde_yaml_ng::Value::String(version.clone());
						}
					} else if let Some(entry_mapping) = entry.as_mapping_mut() {
						let version_key = serde_yaml_ng::Value::String("version".to_string());
						if let Some(version_value) = entry_mapping.get_mut(&version_key) {
							if let Some(text) = version_value.as_str() {
								if !text.starts_with("link:") && !text.starts_with("workspace:") {
									*version_value = serde_yaml_ng::Value::String(version.clone());
								}
							}
						}
					}
				}
			}
		}
	}
}

pub fn update_bun_lock(contents: &str, raw_versions: &BTreeMap<String, String>) -> String {
	let mut updated = contents.to_string();
	for (name, version) in raw_versions {
		let pattern = format!("\"{name}\": \"");
		if let Some(start) = updated.find(&pattern) {
			let value_start = start + pattern.len();
			if let Some(end_offset) = updated[value_start..].find('"') {
				updated.replace_range(value_start..value_start + end_offset, version);
			}
		}
	}
	updated
}

pub fn update_bun_lock_binary(
	contents: &[u8],
	old_versions: &BTreeMap<String, String>,
	raw_versions: &BTreeMap<String, String>,
) -> Vec<u8> {
	let mut updated = contents.to_vec();
	for (name, old_version) in old_versions {
		let Some(new_version) = raw_versions.get(name) else {
			continue;
		};
		let old_bytes = old_version.as_bytes();
		let new_bytes = new_version.as_bytes();
		if old_bytes == new_bytes {
			continue;
		}
		if old_bytes.is_empty() {
			continue;
		}
		let mut cursor = 0usize;
		while let Some(remaining) = updated.get(cursor..) {
			let Some(relative_index) = remaining
				.windows(old_bytes.len())
				.position(|window| window == old_bytes)
			else {
				break;
			};
			let index = cursor + relative_index;
			updated.splice(index..index + old_bytes.len(), new_bytes.iter().copied());
			cursor = index + new_bytes.len();
		}
	}
	updated
}

pub struct NpmAdapter;

#[must_use]
pub const fn adapter() -> NpmAdapter {
	NpmAdapter
}

impl EcosystemAdapter for NpmAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Npm
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_npm_packages(root)
	}
}

pub fn discover_npm_packages(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let mut included_manifests = HashSet::new();
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	for workspace_manifest in find_package_json_workspaces(root) {
		let (workspace_packages, workspace_warnings) =
			discover_package_json_workspace(&workspace_manifest)?;
		warnings.extend(workspace_warnings);
		for package in workspace_packages {
			included_manifests.insert(package.manifest_path.clone());
			packages.push(package);
		}
	}

	for workspace_manifest in find_pnpm_workspaces(root) {
		let (workspace_packages, workspace_warnings) =
			discover_pnpm_workspace(&workspace_manifest)?;
		warnings.extend(workspace_warnings);
		for package in workspace_packages {
			included_manifests.insert(package.manifest_path.clone());
			packages.push(package);
		}
	}

	for manifest_path in find_all_package_json(root) {
		if included_manifests.contains(&manifest_path) {
			continue;
		}

		if let Some(package) = parse_package_json(
			&manifest_path,
			manifest_path.parent().unwrap_or(root),
			detect_npm_manager(manifest_path.parent().unwrap_or(root)),
		)? {
			packages.push(package);
		}
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	Ok(AdapterDiscovery { packages, warnings })
}

fn discover_package_json_workspace(
	workspace_manifest: &Path,
) -> MonochangeResult<(Vec<PackageRecord>, Vec<String>)> {
	let contents = fs::read_to_string(workspace_manifest).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			workspace_manifest.display()
		))
	})?;
	let parsed = serde_json::from_str::<Value>(&contents).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			workspace_manifest.display()
		))
	})?;
	let workspace_root = workspace_manifest
		.parent()
		.unwrap_or_else(|| Path::new("."));
	let patterns = workspace_patterns_from_package_json(&parsed);
	let mut warnings = Vec::new();
	let manifests = expand_member_patterns(workspace_root, &patterns, &mut warnings);
	let mut packages = Vec::new();

	for manifest in manifests {
		if let Some(package) = parse_package_json(
			&manifest,
			workspace_root,
			detect_npm_manager(workspace_root),
		)? {
			packages.push(package);
		}
	}

	Ok((packages, warnings))
}

fn discover_pnpm_workspace(
	workspace_manifest: &Path,
) -> MonochangeResult<(Vec<PackageRecord>, Vec<String>)> {
	let contents = fs::read_to_string(workspace_manifest).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			workspace_manifest.display()
		))
	})?;
	let parsed = serde_yaml_ng::from_str::<YamlValue>(&contents).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			workspace_manifest.display()
		))
	})?;
	let workspace_root = workspace_manifest
		.parent()
		.unwrap_or_else(|| Path::new("."));
	let patterns = parsed
		.get("packages")
		.and_then(YamlValue::as_sequence)
		.map(|items| {
			items
				.iter()
				.filter_map(YamlValue::as_str)
				.map(ToString::to_string)
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();
	let mut warnings = Vec::new();
	let manifests = expand_member_patterns(workspace_root, &patterns, &mut warnings);
	let mut packages = Vec::new();

	for manifest in manifests {
		if let Some(package) = parse_package_json(&manifest, workspace_root, "pnpm")? {
			packages.push(package);
		}
	}

	Ok((packages, warnings))
}

fn workspace_patterns_from_package_json(parsed: &Value) -> Vec<String> {
	if let Some(array) = parsed.get("workspaces").and_then(Value::as_array) {
		return array
			.iter()
			.filter_map(Value::as_str)
			.map(ToString::to_string)
			.collect();
	}

	parsed
		.get("workspaces")
		.and_then(Value::as_object)
		.and_then(|object| object.get("packages"))
		.and_then(Value::as_array)
		.map(|items| {
			items
				.iter()
				.filter_map(Value::as_str)
				.map(ToString::to_string)
				.collect::<Vec<_>>()
		})
		.unwrap_or_default()
}

fn parse_package_json(
	manifest_path: &Path,
	workspace_root: &Path,
	manager: &str,
) -> MonochangeResult<Option<PackageRecord>> {
	let contents = fs::read_to_string(manifest_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			manifest_path.display()
		))
	})?;
	let parsed = serde_json::from_str::<Value>(&contents).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			manifest_path.display()
		))
	})?;
	let Some(name) = parsed.get("name").and_then(Value::as_str) else {
		return Ok(None);
	};
	let version = parsed
		.get("version")
		.and_then(Value::as_str)
		.and_then(|value| Version::parse(value).ok());
	let publish_state = if parsed.get("private").and_then(Value::as_bool) == Some(true) {
		PublishState::Private
	} else {
		PublishState::Public
	};

	let mut package = PackageRecord::new(
		Ecosystem::Npm,
		name,
		manifest_path.to_path_buf(),
		workspace_root.to_path_buf(),
		version,
		publish_state,
	);
	package
		.metadata
		.insert("manager".to_string(), manager.to_string());
	package.declared_dependencies = parse_dependencies(&parsed);
	Ok(Some(package))
}

fn parse_dependencies(parsed: &Value) -> Vec<PackageDependency> {
	[
		("dependencies", DependencyKind::Runtime),
		("devDependencies", DependencyKind::Development),
		("peerDependencies", DependencyKind::Peer),
	]
	.into_iter()
	.flat_map(|(section, kind)| parse_dependency_map(parsed, section, kind))
	.collect()
}

fn parse_dependency_map(
	parsed: &Value,
	section: &str,
	kind: DependencyKind,
) -> Vec<PackageDependency> {
	parsed
		.get(section)
		.and_then(Value::as_object)
		.map(|dependencies| {
			dependencies
				.iter()
				.filter_map(|(name, version)| {
					version.as_str().map(|constraint| PackageDependency {
						name: name.clone(),
						kind,
						version_constraint: Some(constraint.to_string()),
						optional: false,
					})
				})
				.collect::<Vec<_>>()
		})
		.unwrap_or_default()
}

fn detect_npm_manager(workspace_root: &Path) -> &'static str {
	if workspace_root.join("bun.lockb").exists() {
		"bun"
	} else if workspace_root.join(PNPM_WORKSPACE_FILE).exists() {
		"pnpm"
	} else {
		"npm"
	}
}

fn find_package_json_workspaces(root: &Path) -> Vec<PathBuf> {
	let mut manifests = find_all_package_json(root)
		.into_iter()
		.filter(|manifest_path| package_json_declares_workspaces(manifest_path).unwrap_or(false))
		.collect::<Vec<_>>();
	manifests.sort();
	manifests
}

fn package_json_declares_workspaces(manifest_path: &Path) -> MonochangeResult<bool> {
	let contents = fs::read_to_string(manifest_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			manifest_path.display()
		))
	})?;
	let parsed = serde_json::from_str::<Value>(&contents).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			manifest_path.display()
		))
	})?;
	Ok(!workspace_patterns_from_package_json(&parsed).is_empty())
}

fn expand_member_patterns(
	root: &Path,
	patterns: &[String],
	warnings: &mut Vec<String>,
) -> BTreeSet<PathBuf> {
	let mut manifests = BTreeSet::new();
	for pattern in patterns {
		let joined_pattern = root.join(pattern).to_string_lossy().to_string();
		let matches = glob(&joined_pattern)
			.into_iter()
			.flat_map(|paths| paths.filter_map(Result::ok))
			.map(|path| normalize_path(&path))
			.collect::<Vec<_>>();
		if matches.is_empty() {
			warnings.push(format!(
				"npm workspace pattern `{pattern}` under {} matched no packages",
				root.display()
			));
		}

		for matched_path in matches {
			let manifest_path = if matched_path.is_dir() {
				matched_path.join(PACKAGE_JSON_FILE)
			} else {
				matched_path
			};
			if manifest_path.file_name().and_then(|name| name.to_str()) == Some(PACKAGE_JSON_FILE) {
				manifests.insert(manifest_path);
			}
		}
	}
	manifests
}

fn find_pnpm_workspaces(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == PNPM_WORKSPACE_FILE)
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

fn find_all_package_json(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == PACKAGE_JSON_FILE)
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

fn should_descend(entry: &DirEntry) -> bool {
	let file_name = entry.file_name().to_string_lossy();
	!matches!(
		file_name.as_ref(),
		".git" | "target" | "node_modules" | ".devenv" | "book"
	)
}

#[cfg(test)]
mod __tests;
