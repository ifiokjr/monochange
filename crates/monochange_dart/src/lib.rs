#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

doc_comment::doctest!("../readme.md");

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
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageDependency;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const PUBSPEC_FILE: &str = "pubspec.yaml";

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

	Ok(AdapterDiscovery { packages, warnings })
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
			dependencies.iter().map(|(name, value)| PackageDependency {
				name: name.as_str().unwrap_or_default().to_string(),
				kind: DependencyKind::Runtime,
				version_constraint: match value {
					Value::String(text) => Some(text.clone()),
					Value::Mapping(mapping) => mapping
						.get(Value::String("version".to_string()))
						.and_then(Value::as_str)
						.map(ToString::to_string),
					_ => None,
				},
				optional: false,
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
