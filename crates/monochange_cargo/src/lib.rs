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
use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::CompatibilityAssessment;
use monochange_core::DependencyKind;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageDependency;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_semver::CompatibilityProvider;
use semver::Version;
use toml::Value;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const CARGO_MANIFEST_FILE: &str = "Cargo.toml";
pub const RUST_SEMVER_PROVIDER_ID: &str = "rust-semver";

pub struct CargoAdapter;

#[must_use]
pub const fn adapter() -> CargoAdapter {
	CargoAdapter
}

impl EcosystemAdapter for CargoAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Cargo
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_cargo_packages(root)
	}
}

pub struct RustSemverProvider;

impl CompatibilityProvider for RustSemverProvider {
	fn provider_id(&self) -> &'static str {
		RUST_SEMVER_PROVIDER_ID
	}

	fn assess(
		&self,
		package: &PackageRecord,
		change_signal: &ChangeSignal,
	) -> Option<CompatibilityAssessment> {
		if package.ecosystem != Ecosystem::Cargo {
			return None;
		}

		change_signal
			.evidence_refs
			.iter()
			.find_map(|reference| parse_rust_semver_evidence(reference, &package.id))
	}
}

pub fn discover_cargo_packages(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let workspace_manifests = find_workspace_manifests(root);
	let mut included_manifests = HashSet::new();
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	for workspace_manifest in &workspace_manifests {
		let (workspace_packages, workspace_warnings) =
			discover_workspace_packages(workspace_manifest)?;
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
			parse_package_manifest(&manifest_path, manifest_path.parent().unwrap_or(root), None)?
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
	let contents = fs::read_to_string(workspace_manifest).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			workspace_manifest.display()
		))
	})?;
	let parsed = toml::from_str::<Value>(&contents).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			workspace_manifest.display()
		))
	})?;
	let workspace_root = workspace_manifest
		.parent()
		.unwrap_or_else(|| Path::new("."));
	let workspace_version = workspace_package_version(&parsed);
	let workspace = parsed
		.get("workspace")
		.and_then(Value::as_table)
		.ok_or_else(|| {
			MonochangeError::Discovery(format!(
				"{} is missing [workspace]",
				workspace_manifest.display()
			))
		})?;
	let members = workspace
		.get("members")
		.and_then(Value::as_array)
		.cloned()
		.unwrap_or_default();
	let excludes = workspace
		.get("exclude")
		.and_then(Value::as_array)
		.cloned()
		.unwrap_or_default();

	let member_patterns = members.iter().filter_map(Value::as_str).collect::<Vec<_>>();
	let exclude_patterns = excludes
		.iter()
		.filter_map(Value::as_str)
		.collect::<Vec<_>>();
	let mut warnings = Vec::new();
	let member_manifests = expand_manifest_patterns(
		workspace_root,
		&member_patterns,
		&exclude_patterns,
		&mut warnings,
	);
	let mut packages = Vec::new();

	for manifest_path in member_manifests {
		if let Some(package) =
			parse_package_manifest(&manifest_path, workspace_root, workspace_version.as_ref())?
		{
			packages.push(package);
		}
	}

	Ok((packages, warnings))
}

fn expand_manifest_patterns(
	root: &Path,
	member_patterns: &[&str],
	exclude_patterns: &[&str],
	warnings: &mut Vec<String>,
) -> BTreeSet<PathBuf> {
	let excluded = exclude_patterns
		.iter()
		.flat_map(|pattern| glob_pattern_paths(root, pattern))
		.collect::<HashSet<_>>();
	let mut manifests = BTreeSet::new();

	for pattern in member_patterns {
		let matches = glob_pattern_paths(root, pattern);
		if matches.is_empty() {
			warnings.push(format!(
				"cargo workspace pattern `{pattern}` under {} matched no packages",
				root.display()
			));
		}

		for matched_path in matches {
			let manifest_path = if matched_path.is_dir() {
				matched_path.join(CARGO_MANIFEST_FILE)
			} else {
				matched_path
			};

			if manifest_path.file_name().and_then(|name| name.to_str()) != Some(CARGO_MANIFEST_FILE)
			{
				continue;
			}

			if !excluded.contains(&manifest_path) {
				manifests.insert(manifest_path);
			}
		}
	}

	manifests
}

fn glob_pattern_paths(root: &Path, pattern: &str) -> Vec<PathBuf> {
	let joined_pattern = root.join(pattern).to_string_lossy().to_string();
	glob(&joined_pattern)
		.into_iter()
		.flat_map(|paths| paths.filter_map(Result::ok))
		.collect()
}

fn parse_package_manifest(
	manifest_path: &Path,
	workspace_root: &Path,
	workspace_version: Option<&Version>,
) -> MonochangeResult<Option<PackageRecord>> {
	let contents = fs::read_to_string(manifest_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			manifest_path.display()
		))
	})?;
	let parsed = toml::from_str::<Value>(&contents).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			manifest_path.display()
		))
	})?;
	let Some(package) = parsed.get("package").and_then(Value::as_table) else {
		return Ok(None);
	};

	let name = package.get("name").and_then(Value::as_str).ok_or_else(|| {
		MonochangeError::Discovery(format!(
			"{} is missing package.name",
			manifest_path.display()
		))
	})?;
	let version = package
		.get("version")
		.and_then(|value| parse_package_version(value, workspace_version));
	let publish_state = if package.get("publish").and_then(Value::as_bool) == Some(false) {
		PublishState::Private
	} else {
		PublishState::Public
	};

	let mut package_record = PackageRecord::new(
		Ecosystem::Cargo,
		name,
		manifest_path.to_path_buf(),
		workspace_root.to_path_buf(),
		version,
		publish_state,
	);
	package_record.declared_dependencies = parse_dependencies(&parsed);
	Ok(Some(package_record))
}

fn workspace_package_version(parsed: &Value) -> Option<Version> {
	parsed
		.get("workspace")
		.and_then(Value::as_table)
		.and_then(|workspace| workspace.get("package"))
		.and_then(Value::as_table)
		.and_then(|package| package.get("version"))
		.and_then(Value::as_str)
		.and_then(|value| Version::parse(value).ok())
}

fn parse_package_version(value: &Value, workspace_version: Option<&Version>) -> Option<Version> {
	value
		.as_str()
		.and_then(|version| Version::parse(version).ok())
		.or_else(|| {
			value
				.as_table()
				.and_then(|table| table.get("workspace"))
				.and_then(Value::as_bool)
				.filter(|is_workspace| *is_workspace)
				.and(workspace_version.cloned())
		})
}

fn parse_dependencies(parsed: &Value) -> Vec<PackageDependency> {
	[
		("dependencies", DependencyKind::Runtime),
		("dev-dependencies", DependencyKind::Development),
		("build-dependencies", DependencyKind::Build),
	]
	.into_iter()
	.flat_map(|(section, kind)| parse_dependency_table(parsed, section, kind))
	.collect()
}

fn parse_dependency_table(
	parsed: &Value,
	section: &str,
	kind: DependencyKind,
) -> Vec<PackageDependency> {
	parsed
		.get(section)
		.and_then(Value::as_table)
		.map(|table| {
			table
				.iter()
				.map(|(name, value)| PackageDependency {
					name: name.clone(),
					kind,
					version_constraint: dependency_constraint(value),
					optional: value
						.as_table()
						.and_then(|table| table.get("optional"))
						.and_then(Value::as_bool)
						.unwrap_or(false),
				})
				.collect::<Vec<_>>()
		})
		.unwrap_or_default()
}

fn dependency_constraint(value: &Value) -> Option<String> {
	if let Some(version) = value.as_str() {
		return Some(version.to_string());
	}

	value
		.as_table()
		.and_then(|table| table.get("version"))
		.and_then(Value::as_str)
		.map(ToString::to_string)
}

fn has_workspace_section(manifest_path: &Path) -> MonochangeResult<bool> {
	let contents = fs::read_to_string(manifest_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			manifest_path.display()
		))
	})?;
	let parsed = toml::from_str::<Value>(&contents).map_err(|error| {
		MonochangeError::Discovery(format!(
			"failed to parse {}: {error}",
			manifest_path.display()
		))
	})?;
	Ok(parsed.get("workspace").is_some())
}

fn find_all_manifests(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == CARGO_MANIFEST_FILE)
		.map(DirEntry::into_path)
		.collect()
}

fn parse_rust_semver_evidence(
	reference: &str,
	package_id: &str,
) -> Option<CompatibilityAssessment> {
	let normalized = reference
		.strip_prefix("rust-semver:")
		.or_else(|| reference.strip_prefix("cargo-semver:"))?;
	let mut parts = normalized.splitn(2, ':');
	let severity = parse_severity(parts.next()?);
	let summary = parts
		.next()
		.map_or_else(|| "Rust semver assessment".to_string(), ToString::to_string);

	Some(CompatibilityAssessment {
		package_id: package_id.to_string(),
		provider_id: RUST_SEMVER_PROVIDER_ID.to_string(),
		severity,
		confidence: "high".to_string(),
		summary,
		evidence_location: None,
	})
}

fn parse_severity(value: &str) -> BumpSeverity {
	match value {
		"major" => BumpSeverity::Major,
		"minor" => BumpSeverity::Minor,
		"patch" => BumpSeverity::Patch,
		_ => BumpSeverity::None,
	}
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
