#![forbid(clippy::indexing_slicing)]

//! # `monochange_dart`
//!
//! <!-- {=monochangeDartCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_dart` discovers Dart and Flutter packages for the shared planner.
//!
//! Reach for this crate when you need to scan `pubspec.yaml` files, expand Dart or Flutter workspaces, and normalize package metadata into `monochange_core` records.
//!
//! ## Why use it?
//!
//! - cover both pure Dart and Flutter package layouts with one adapter
//! - normalize pubspec metadata and dependency edges for shared release planning
//! - detect Flutter packages without maintaining a separate discovery path
//!
//! ## Best for
//!
//! - scanning Dart or Flutter monorepos into normalized workspace records
//! - reusing the same planning pipeline for mobile and non-mobile packages
//! - discovering Flutter packages without a dedicated Flutter-only adapter layer
//!
//! ## Public entry points
//!
//! - `discover_dart_packages(root)` discovers Dart and Flutter workspaces plus standalone packages
//! - `DartAdapter` exposes the shared adapter interface
//!
//! ## Scope
//!
//! - `pubspec.yaml` workspace expansion
//! - Dart package parsing
//! - Flutter package detection
//! - normalized dependency extraction
//! <!-- {/monochangeDartCrateDocs} -->

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use glob::glob;
use monochange_core::AdapterDiscovery;
use monochange_core::DependencyKind;
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
use semver::Version;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const PUBSPEC_FILE: &str = "pubspec.yaml";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DartVersionedFileKind {
	Manifest,
	Lock,
}

pub fn supported_versioned_file_kind(path: &Path) -> Option<DartVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();
	match file_name {
		"pubspec.lock" => Some(DartVersionedFileKind::Lock),
		_ if path.extension().and_then(|ext| ext.to_str()) == Some("yaml")
			|| path.extension().and_then(|ext| ext.to_str()) == Some("yml") =>
		{
			Some(DartVersionedFileKind::Manifest)
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
	let mut discovered = [scope.join("pubspec.lock")]
		.into_iter()
		.filter(|path| path.exists())
		.collect::<Vec<_>>();
	if discovered.is_empty() && scope != manifest_dir {
		discovered.extend(
			[manifest_dir.join("pubspec.lock")]
				.into_iter()
				.filter(|path| path.exists()),
		);
	}
	discovered
}

pub fn default_lockfile_commands(package: &PackageRecord) -> Vec<LockfileCommandExecution> {
	let command = match package.ecosystem {
		Ecosystem::Flutter => "flutter pub get",
		Ecosystem::Dart => "dart pub get",
		_ => return Vec::new(),
	};
	discover_lockfiles(package)
		.into_iter()
		.map(|lockfile| {
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

pub fn update_dependency_fields(
	mapping: &mut Mapping,
	fields: &[&str],
	versioned_deps: &std::collections::BTreeMap<String, String>,
) {
	for field in fields {
		if let Some(Value::Mapping(section)) = mapping.get_mut(Value::String(field.to_string())) {
			for (dep_name, dep_version) in versioned_deps {
				let key = Value::String(dep_name.clone());
				if section.contains_key(&key) {
					section.insert(key, Value::String(dep_version.clone()));
				}
			}
		}
	}
}

pub fn update_manifest_text(
	contents: &str,
	owner_version: Option<&str>,
	fields: &[&str],
	versioned_deps: &std::collections::BTreeMap<String, String>,
) -> MonochangeResult<String> {
	serde_yaml_ng::from_str::<Mapping>(contents).map_err(|error| {
		MonochangeError::Config(format!("failed to parse pubspec yaml: {error}"))
	})?;
	let line_ranges = yaml_line_ranges(contents);
	let mut replacements = Vec::<((usize, usize), String)>::new();
	if let Some(owner_version) = owner_version
		&& let Some(span) = find_yaml_scalar_for_key(contents, &line_ranges, 0, "version")
	{
		replacements.push((
			span,
			render_yaml_scalar(&contents[span.0..span.1], owner_version),
		));
	}
	for field in fields {
		let Some(section_index) = find_yaml_key_line(contents, &line_ranges, 0, field) else {
			continue;
		};
		for (dep_name, dep_version) in versioned_deps {
			if let Some(span) =
				find_yaml_dependency_scalar(contents, &line_ranges, section_index, dep_name)
			{
				replacements.push((
					span,
					render_yaml_scalar(&contents[span.0..span.1], dep_version),
				));
			}
		}
	}
	replacements.sort_by(|left, right| right.0.0.cmp(&left.0.0));
	let mut updated = contents.to_string();
	for ((start, end), replacement) in replacements {
		updated.replace_range(start..end, &replacement);
	}
	Ok(updated)
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

fn find_yaml_scalar_for_key(
	contents: &str,
	line_ranges: &[(usize, usize)],
	indent: usize,
	key: &str,
) -> Option<(usize, usize)> {
	let line_index = find_yaml_key_line(contents, line_ranges, indent, key)?;
	let range = *line_ranges.get(line_index)?;
	parse_yaml_line(contents, range).and_then(|line| line.value_span)
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

fn find_yaml_dependency_scalar(
	contents: &str,
	line_ranges: &[(usize, usize)],
	section_index: usize,
	dep_name: &str,
) -> Option<(usize, usize)> {
	let section = parse_yaml_line(contents, *line_ranges.get(section_index)?)?;
	let section_indent = section.indent;
	let mut index = section_index + 1;
	while let Some(range) = line_ranges.get(index) {
		let Some(line) = parse_yaml_line(contents, *range) else {
			index += 1;
			continue;
		};
		if line.indent <= section_indent {
			break;
		}
		if line.key == dep_name {
			if let Some(value_span) = line.value_span {
				return Some(value_span);
			}
			let dep_indent = line.indent;
			let mut nested_index = index + 1;
			while let Some(nested_range) = line_ranges.get(nested_index) {
				let Some(nested_line) = parse_yaml_line(contents, *nested_range) else {
					nested_index += 1;
					continue;
				};
				if nested_line.indent <= dep_indent {
					break;
				}
				if nested_line.key == "version" {
					return nested_line.value_span;
				}
				nested_index += 1;
			}
			return None;
		}
		index += 1;
	}
	None
}

struct ParsedYamlLine<'a> {
	indent: usize,
	key: &'a str,
	value_span: Option<(usize, usize)>,
}

fn parse_yaml_line(contents: &str, range: (usize, usize)) -> Option<ParsedYamlLine<'_>> {
	let line = &contents[range.0..range.1];
	let trimmed = line.trim_start_matches([' ', '\t']);
	if trimmed.is_empty() || trimmed.starts_with('#') {
		return None;
	}
	let indent = line.len() - trimmed.len();
	let colon = trimmed.find(':')?;
	let key = trimmed[..colon].trim();
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
	let value = &suffix[value_offset..];
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
		let trimmed_end = value[..comment_index].trim_end_matches([' ', '\t']).len();
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

pub fn update_pubspec_lock(
	mapping: &mut Mapping,
	raw_versions: &std::collections::BTreeMap<String, String>,
) {
	let Some(Value::Mapping(packages)) = mapping.get_mut(Value::String("packages".to_string()))
	else {
		return;
	};
	for (name, version) in raw_versions {
		let key = Value::String(name.clone());
		let Some(Value::Mapping(entry)) = packages.get_mut(&key) else {
			continue;
		};
		entry.insert(
			Value::String("version".to_string()),
			Value::String(version.clone()),
		);
	}
}

pub struct DartAdapter;

#[must_use]
pub const fn adapter() -> DartAdapter {
	DartAdapter
}

impl EcosystemAdapter for DartAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Dart
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_dart_packages(root)
	}
}

#[tracing::instrument(skip_all)]
pub fn discover_dart_packages(root: &Path) -> MonochangeResult<AdapterDiscovery> {
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
	tracing::debug!(packages = packages.len(), "discovered dart packages");

	Ok(AdapterDiscovery { packages, warnings })
}

/// Load one explicitly configured Dart/Flutter package without walking the repo.
pub fn load_configured_dart_package(
	root: &Path,
	package_path: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let manifest_path =
		if package_path.file_name().and_then(|name| name.to_str()) == Some(PUBSPEC_FILE) {
			package_path.to_path_buf()
		} else {
			package_path.join(PUBSPEC_FILE)
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
	let parsed = parse_yaml_manifest(workspace_manifest)?;
	let workspace_root = workspace_manifest
		.parent()
		.unwrap_or_else(|| Path::new("."));
	let patterns = yaml_array_strings(&parsed, "workspace");
	let mut warnings = Vec::new();
	let manifests = expand_workspace_patterns(workspace_root, &patterns, &mut warnings);
	let mut packages = Vec::new();

	for manifest_path in manifests {
		if let Some(package) = parse_manifest(&manifest_path, workspace_root)? {
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
				"dart workspace pattern `{pattern}` under {} matched no packages",
				root.display()
			));
		}

		for matched_path in matches {
			let manifest_path = if matched_path.is_dir() {
				matched_path.join(PUBSPEC_FILE)
			} else {
				matched_path
			};
			if manifest_path.file_name().and_then(|name| name.to_str()) == Some(PUBSPEC_FILE)
				&& manifest_path.exists()
			{
				manifests.insert(manifest_path);
			}
		}
	}
	manifests
}

fn parse_manifest(
	manifest_path: &Path,
	workspace_root: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let parsed = parse_yaml_manifest(manifest_path)?;
	let Some(name) = yaml_string(&parsed, "name") else {
		return Ok(None);
	};
	let ecosystem = if parsed.get(Value::String("flutter".to_string())).is_some() {
		Ecosystem::Flutter
	} else {
		Ecosystem::Dart
	};
	let version = yaml_string(&parsed, "version").and_then(|value| Version::parse(&value).ok());
	let publish_state = if yaml_bool(&parsed, "publish_to").is_some() {
		PublishState::Private
	} else {
		PublishState::Public
	};

	let mut package = PackageRecord::new(
		ecosystem,
		name,
		manifest_path.to_path_buf(),
		workspace_root.to_path_buf(),
		version,
		publish_state,
	);
	package.declared_dependencies = parse_dependencies(&parsed);
	Ok(Some(package))
}

fn parse_dependencies(parsed: &Mapping) -> Vec<PackageDependency> {
	["dependencies", "dev_dependencies"]
		.into_iter()
		.filter_map(|section| yaml_mapping(parsed, section))
		.flat_map(|dependencies| {
			dependencies.iter().map(|(name, value)| {
				PackageDependency {
					name: name.as_str().unwrap_or_default().to_string(),
					kind: DependencyKind::Runtime,
					version_constraint: match value {
						Value::String(text) => Some(text.clone()),
						Value::Mapping(mapping) => {
							mapping
								.get(Value::String("version".to_string()))
								.and_then(Value::as_str)
								.map(ToString::to_string)
						}
						_ => None,
					},
					optional: false,
				}
			})
		})
		.filter(|dependency| !dependency.name.is_empty())
		.collect()
}

fn has_workspace_section(manifest_path: &Path) -> MonochangeResult<bool> {
	let parsed = parse_yaml_manifest(manifest_path)?;
	Ok(parsed
		.get(Value::String("workspace".to_string()))
		.and_then(Value::as_sequence)
		.is_some_and(|items| !items.is_empty()))
}

fn parse_yaml_manifest(manifest_path: &Path) -> MonochangeResult<Mapping> {
	let contents = fs::read_to_string(manifest_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			manifest_path.display()
		))
	})?;
	serde_yaml_ng::from_str::<Mapping>(&contents).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			manifest_path.display()
		))
	})
}

fn yaml_string(mapping: &Mapping, key: &str) -> Option<String> {
	mapping
		.get(Value::String(key.to_string()))
		.and_then(Value::as_str)
		.map(ToString::to_string)
}

fn yaml_bool(mapping: &Mapping, key: &str) -> Option<bool> {
	mapping
		.get(Value::String(key.to_string()))
		.and_then(Value::as_bool)
}

fn yaml_mapping<'map>(mapping: &'map Mapping, key: &str) -> Option<&'map Mapping> {
	mapping
		.get(Value::String(key.to_string()))
		.and_then(Value::as_mapping)
}

fn yaml_array_strings(mapping: &Mapping, key: &str) -> Vec<String> {
	mapping
		.get(Value::String(key.to_string()))
		.and_then(Value::as_sequence)
		.map(|items| {
			items
				.iter()
				.filter_map(Value::as_str)
				.map(ToString::to_string)
				.collect::<Vec<_>>()
		})
		.unwrap_or_default()
}

fn find_all_manifests(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == PUBSPEC_FILE)
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
