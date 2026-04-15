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
use monochange_core::ShellConfig;
use monochange_core::normalize_path;
use monochange_core::relative_to_root;
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

/// Classify an npm-family versioned file path.
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

/// Discover lockfiles that should be refreshed for `package`.
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

/// Return the default lockfile refresh commands for `package`.
pub fn default_lockfile_commands(package: &PackageRecord) -> Vec<LockfileCommandExecution> {
	discover_lockfiles(package)
		.into_iter()
		.map(|lockfile| {
			let file_name = lockfile
				.file_name()
				.and_then(|name| name.to_str())
				.unwrap_or_default();
			let command = if file_name == "package-lock.json" {
				"npm install --package-lock-only"
			} else if file_name == "pnpm-lock.yaml" {
				"pnpm install --lockfile-only"
			} else {
				"bun install --lockfile-only"
			};

			LockfileCommandExecution {
				command: command.to_string(),
				cwd: lockfile
					.parent()
					.unwrap_or(&package.workspace_root)
					.to_path_buf(),
				shell: ShellConfig::None,
			}
		})
		.collect()
}

/// Update dependency sections inside a parsed `package.json`-style value.
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

/// Update versions embedded in a parsed `package-lock.json` document.
pub fn update_package_lock(
	value: &mut Value,
	package_paths_by_name: &BTreeMap<String, PathBuf>,
	raw_versions: &BTreeMap<String, String>,
) {
	if let Some(root_name) = value.get("name").and_then(Value::as_str)
		&& let Some(version) = raw_versions.get(root_name)
		&& let Some(obj) = value.as_object_mut()
	{
		obj.insert("version".to_string(), Value::String(version.clone()));
	}
	if let Some(packages) = value.get_mut("packages").and_then(Value::as_object_mut) {
		for (entry_path, entry_value) in packages {
			let Some(entry_object) = entry_value.as_object_mut() else {
				continue;
			};

			if let Some(name) = entry_object.get("name").and_then(Value::as_str) {
				let Some(version) = raw_versions.get(name) else {
					continue;
				};

				entry_object.insert("version".to_string(), Value::String(version.clone()));
				continue;
			}
			for (name, package_dir) in package_paths_by_name {
				if entry_path == &package_dir.to_string_lossy()
					&& let Some(version) = raw_versions.get(name)
				{
					entry_object.insert("version".to_string(), Value::String(version.clone()));
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

fn uses_workspace_reference(text: &str) -> bool {
	text.starts_with("link:") || text.starts_with("workspace:")
}

/// Update versions embedded in a parsed `pnpm-lock.yaml` mapping.
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

					let Some(text) = entry.as_str() else {
						if let Some(entry_mapping) = entry.as_mapping_mut() {
							let version_key = serde_yaml_ng::Value::String("version".to_string());
							if let Some(version_value) = entry_mapping.get_mut(&version_key)
								&& let Some(text) = version_value.as_str()
								&& !uses_workspace_reference(text)
							{
								*version_value = serde_yaml_ng::Value::String(version.clone());
							}
						}

						continue;
					};

					if uses_workspace_reference(text) {
						continue;
					}

					*entry = serde_yaml_ng::Value::String(version.clone());
				}
			}
		}
	}
}

/// Update `pnpm-lock.yaml` text in place using direct YAML-aware replacements.
#[must_use = "the lockfile update result must be checked"]
pub fn update_pnpm_lock_text(
	contents: &str,
	raw_versions: &BTreeMap<String, String>,
) -> MonochangeResult<String> {
	serde_yaml_ng::from_str::<serde_yaml_ng::Value>(contents).map_err(|error| {
		MonochangeError::Config(format!("failed to parse pnpm lock yaml: {error}"))
	})?;
	let line_ranges = yaml_line_ranges(contents);
	let mut replacements = Vec::<((usize, usize), String)>::new();
	for section_name in ["importers", "packages", "snapshots"] {
		let Some(section_index) = find_yaml_key_line(contents, &line_ranges, 0, section_name)
		else {
			continue;
		};
		collect_pnpm_section_replacements(
			contents,
			&line_ranges,
			section_index,
			raw_versions,
			&mut replacements,
		);
	}
	replacements.sort_by_key(|right| std::cmp::Reverse(right.0.0));
	let mut updated = contents.to_string();
	for ((start, end), replacement) in replacements {
		updated.replace_range(start..end, &replacement);
	}
	Ok(updated)
}

fn collect_pnpm_section_replacements(
	contents: &str,
	line_ranges: &[(usize, usize)],
	section_index: usize,
	raw_versions: &BTreeMap<String, String>,
	replacements: &mut Vec<((usize, usize), String)>,
) {
	let Some(section) = line_ranges
		.get(section_index)
		.and_then(|range| parse_yaml_line(contents, *range))
	else {
		return;
	};
	let mut index = section_index + 1;
	while let Some(range) = line_ranges.get(index) {
		let Some(line) = parse_yaml_line(contents, *range) else {
			index += 1;
			continue;
		};
		if line.indent <= section.indent {
			break;
		}
		let entry_indent = line.indent;
		index += 1;
		while let Some(nested_range) = line_ranges.get(index) {
			let Some(nested_line) = parse_yaml_line(contents, *nested_range) else {
				index += 1;
				continue;
			};
			if nested_line.indent <= entry_indent {
				break;
			}
			if is_pnpm_dependency_field(nested_line.key) {
				collect_pnpm_dependency_replacements(
					contents,
					line_ranges,
					index,
					raw_versions,
					replacements,
				);
			}
			index += 1;
		}
	}
}

fn collect_pnpm_dependency_replacements(
	contents: &str,
	line_ranges: &[(usize, usize)],
	section_index: usize,
	raw_versions: &BTreeMap<String, String>,
	replacements: &mut Vec<((usize, usize), String)>,
) {
	let Some(section) = line_ranges
		.get(section_index)
		.and_then(|range| parse_yaml_line(contents, *range))
	else {
		return;
	};
	let mut index = section_index + 1;
	while let Some(range) = line_ranges.get(index) {
		let Some(line) = parse_yaml_line(contents, *range) else {
			index += 1;
			continue;
		};
		if line.indent <= section.indent {
			break;
		}
		let Some(version) = raw_versions.get(line.key) else {
			index += 1;
			continue;
		};
		if let Some(value_span) = line.value_span {
			push_pnpm_scalar_replacement(contents, value_span, version, replacements);
			index += 1;
			continue;
		}
		let dependency_indent = line.indent;
		index += 1;
		while let Some(nested_range) = line_ranges.get(index) {
			let Some(nested_line) = parse_yaml_line(contents, *nested_range) else {
				index += 1;
				continue;
			};
			if nested_line.indent <= dependency_indent {
				break;
			}
			if nested_line.key == "version" {
				if let Some(value_span) = nested_line.value_span {
					push_pnpm_scalar_replacement(contents, value_span, version, replacements);
				}
				break;
			}
			index += 1;
		}
	}
}

fn push_pnpm_scalar_replacement(
	contents: &str,
	span: (usize, usize),
	version: &str,
	replacements: &mut Vec<((usize, usize), String)>,
) {
	let Some(existing) = contents.get(span.0..span.1) else {
		return;
	};
	if !yaml_scalar_is_updatable(existing) {
		return;
	}
	let replacement = render_yaml_scalar(existing, version);
	if replacement != existing {
		replacements.push((span, replacement));
	}
}

fn yaml_scalar_is_updatable(existing: &str) -> bool {
	serde_yaml_ng::from_str::<serde_yaml_ng::Value>(existing)
		.ok()
		.and_then(|value| value.as_str().map(str::to_string))
		.is_some_and(|text| !text.starts_with("link:") && !text.starts_with("workspace:"))
}

fn is_pnpm_dependency_field(key: &str) -> bool {
	matches!(
		key,
		"dependencies" | "devDependencies" | "optionalDependencies" | "peerDependencies"
	)
}

fn yaml_line_ranges(contents: &str) -> Vec<(usize, usize)> {
	let mut ranges = Vec::new();
	let mut start = 0usize;
	for (index, ch) in contents.char_indices() {
		if ch == '\n' {
			ranges.push((start, index));
			start = index + 1;
		}
	}
	if start <= contents.len() {
		ranges.push((start, contents.len()));
	}
	ranges
}

fn find_yaml_key_line(
	contents: &str,
	line_ranges: &[(usize, usize)],
	indent: usize,
	key: &str,
) -> Option<usize> {
	line_ranges.iter().position(|range| {
		parse_yaml_line(contents, *range)
			.is_some_and(|line| line.indent == indent && line.key == key)
	})
}

struct ParsedYamlLine<'a> {
	indent: usize,
	key: &'a str,
	value_span: Option<(usize, usize)>,
}

fn parse_yaml_line(contents: &str, range: (usize, usize)) -> Option<ParsedYamlLine<'_>> {
	let line = contents.get(range.0..range.1)?;
	let trimmed = line.trim_start_matches([' ', '\t']);
	if trimmed.is_empty() || trimmed.starts_with('#') {
		return None;
	}
	let indent = line.len() - trimmed.len();
	let colon = trimmed.find(':')?;
	let key = trimmed.get(..colon)?.trim();
	if key.is_empty() {
		return None;
	}
	let value_span = yaml_value_span(line, range.0, indent + colon + 1);
	Some(ParsedYamlLine {
		indent,
		key,
		value_span,
	})
}

fn yaml_value_span(
	line: &str,
	line_start: usize,
	value_start_in_line: usize,
) -> Option<(usize, usize)> {
	let suffix = line.get(value_start_in_line..)?;
	let value_offset = suffix.find(|ch: char| !matches!(ch, ' ' | '\t'))?;
	let value = suffix.get(value_offset..)?;
	if value.starts_with('#') {
		return None;
	}
	let span_start = line_start + value_start_in_line + value_offset;
	let span_end = if let Some(quote) = value
		.chars()
		.next()
		.filter(|quote| *quote == '"' || *quote == '\'')
	{
		let quote_end = find_yaml_quote_end(value, quote)?;
		span_start + quote_end + 1
	} else {
		let comment_index = value.find('#').unwrap_or(value.len());
		let trimmed_end = value
			.get(..comment_index)?
			.trim_end_matches([' ', '\t'])
			.len();
		span_start + trimmed_end
	};
	(span_end > span_start).then_some((span_start, span_end))
}

fn find_yaml_quote_end(value: &str, quote: char) -> Option<usize> {
	let mut chars = value.char_indices();
	chars.next()?;
	for (index, ch) in chars {
		if ch == quote {
			return Some(index);
		}
	}
	None
}

fn render_yaml_scalar(existing: &str, value: &str) -> String {
	if existing.starts_with('"') && existing.ends_with('"') {
		return format!("\"{value}\"");
	}
	if existing.starts_with('\'') && existing.ends_with('\'') {
		return format!("'{value}'");
	}
	value.to_string()
}

/// Update text-based Bun lockfiles by replacing package version literals.
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

/// Update a binary `bun.lockb` file in place.
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

/// Shared npm-family ecosystem adapter.
pub struct NpmAdapter;

/// Return the shared npm-family ecosystem adapter.
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

#[tracing::instrument(skip_all)]
#[must_use = "the discovery result must be checked"]
/// Discover npm, pnpm, and Bun packages rooted at `root`.
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

	normalize_package_ids(root, &mut packages);
	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);
	tracing::debug!(packages = packages.len(), "discovered npm packages");

	Ok(AdapterDiscovery { packages, warnings })
}

/// Load one explicitly configured npm package without recursively scanning the repo.
///
/// Performance note:
/// release planning only needs the configured package manifests. Walking the
/// entire repository is wasted work in workspaces that contain fixture package
/// trees, so higher-level code uses this helper to parse only the known package.
#[must_use = "the package result must be checked"]
pub fn load_configured_npm_package(
	root: &Path,
	package_path: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let manifest_path =
		if package_path.file_name().and_then(|name| name.to_str()) == Some(PACKAGE_JSON_FILE) {
			package_path.to_path_buf()
		} else {
			package_path.join(PACKAGE_JSON_FILE)
		};
	let workspace_root = manifest_path.parent().unwrap_or(root);
	let mut package = parse_package_json(
		&manifest_path,
		workspace_root,
		detect_npm_manager(workspace_root),
	)?;
	if let Some(package) = package.as_mut() {
		normalize_package_id(root, package);
	}
	Ok(package)
}

fn normalize_package_ids(root: &Path, packages: &mut [PackageRecord]) {
	for package in packages {
		normalize_package_id(root, package);
	}
}

fn normalize_package_id(root: &Path, package: &mut PackageRecord) {
	let Some(relative_manifest) = relative_to_root(root, &package.manifest_path) else {
		return;
	};
	package.id = format!(
		"{}:{}",
		package.ecosystem.as_str(),
		relative_manifest.display()
	);
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
					version.as_str().map(|constraint| {
						PackageDependency {
							name: name.clone(),
							kind,
							version_constraint: Some(constraint.to_string()),
							optional: false,
						}
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
			if manifest_path.file_name().and_then(|name| name.to_str()) == Some(PACKAGE_JSON_FILE)
				&& filter.allows(&manifest_path)
			{
				manifests.insert(manifest_path);
			}
		}
	}
	manifests
}

fn find_pnpm_workspaces(root: &Path) -> Vec<PathBuf> {
	let filter = DiscoveryPathFilter::new(root);
	WalkDir::new(root)
		.into_iter()
		.filter_entry(|entry| filter.should_descend(entry.path()))
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == PNPM_WORKSPACE_FILE)
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

fn find_all_package_json(root: &Path) -> Vec<PathBuf> {
	let filter = DiscoveryPathFilter::new(root);
	WalkDir::new(root)
		.into_iter()
		.filter_entry(|entry| filter.should_descend(entry.path()))
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == PACKAGE_JSON_FILE)
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

#[cfg(test)]
mod __tests;
