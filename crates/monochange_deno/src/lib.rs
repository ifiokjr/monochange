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
use serde_json::Value;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const DENO_MANIFEST_FILES: [&str; 2] = ["deno.json", "deno.jsonc"];

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
	let mut manifests = BTreeSet::new();
	for pattern in patterns {
		let joined_pattern = root.join(pattern).to_string_lossy().to_string();
		let matches = glob(&joined_pattern)
			.into_iter()
			.flat_map(|paths| paths.filter_map(Result::ok))
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
					value.as_str().map(|constraint| PackageDependency {
						name: name.clone(),
						kind: DependencyKind::Runtime,
						version_constraint: Some(constraint.to_string()),
						optional: false,
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
	serde_json::from_str::<Value>(&contents).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			manifest_path.display()
		))
	})
}

fn find_all_manifests(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| DENO_MANIFEST_FILES.contains(&entry.file_name().to_string_lossy().as_ref()))
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
