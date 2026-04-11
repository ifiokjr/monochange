#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_python`
//!
//! `monochange_python` discovers Python packages from uv workspaces, Poetry
//! projects, and standalone `pyproject.toml` files.
//!
//! ## Why use it?
//!
//! - discover uv workspaces and standalone Python packages with one adapter
//! - normalize Python package manifests and dependency edges for the shared
//!   planner
//! - infer lockfile refresh commands for uv and Poetry
//!
//! ## Public entry points
//!
//! - `discover_python_packages(root)` discovers Python packages
//! - `PythonAdapter` exposes the shared adapter interface
//!
//! ## Scope
//!
//! - uv workspace member expansion
//! - `pyproject.toml` parsing (`[project]` and `[tool.poetry]`)
//! - normalized dependency extraction from PEP 621 metadata
//! - lockfile command inference for uv and Poetry

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use glob::glob;
use monochange_core::normalize_path;
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
use semver::Version;
use toml::Value;
use toml_edit::DocumentMut;
use toml_edit::Item;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const PYPROJECT_FILE: &str = "pyproject.toml";
pub const UV_LOCK_FILE: &str = "uv.lock";
pub const POETRY_LOCK_FILE: &str = "poetry.lock";

pub struct PythonAdapter;

#[must_use]
pub const fn adapter() -> PythonAdapter {
	PythonAdapter
}

impl EcosystemAdapter for PythonAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Python
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_python_packages(root)
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PythonVersionedFileKind {
	Manifest,
	Lock,
}

#[must_use]
pub fn supported_versioned_file_kind(path: &Path) -> Option<PythonVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();
	match file_name {
		PYPROJECT_FILE => Some(PythonVersionedFileKind::Manifest),
		UV_LOCK_FILE | POETRY_LOCK_FILE => Some(PythonVersionedFileKind::Lock),
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

	let mut discovered: Vec<PathBuf> = [scope.join(UV_LOCK_FILE), scope.join(POETRY_LOCK_FILE)]
		.into_iter()
		.filter(|path| path.exists())
		.collect();

	if discovered.is_empty() && scope != manifest_dir {
		discovered.extend(
			[
				manifest_dir.join(UV_LOCK_FILE),
				manifest_dir.join(POETRY_LOCK_FILE),
			]
			.into_iter()
			.filter(|path| path.exists()),
		);
	}

	discovered
}

pub fn default_lockfile_commands(package: &PackageRecord) -> Vec<LockfileCommandExecution> {
	let lockfiles = discover_lockfiles(package);
	lockfiles
		.into_iter()
		.filter_map(|lockfile| {
			let file_name = lockfile.file_name()?.to_str()?;
			let command = match file_name {
				UV_LOCK_FILE => "uv lock",
				POETRY_LOCK_FILE => "poetry lock --no-update",
				_ => return None,
			};
			Some(LockfileCommandExecution {
				command: command.to_string(),
				cwd: lockfile
					.parent()
					.unwrap_or(&package.workspace_root)
					.to_path_buf(),
				shell: ShellConfig::None,
			})
		})
		.collect()
}

pub fn update_versioned_file_text(
	contents: &str,
	kind: PythonVersionedFileKind,
	owner_version: Option<&str>,
	versioned_deps: &BTreeMap<String, String>,
) -> Result<String, toml_edit::TomlError> {
	let mut document = contents.parse::<DocumentMut>()?;
	update_versioned_file(&mut document, kind, owner_version, versioned_deps);
	Ok(document.to_string())
}

pub fn update_versioned_file(
	document: &mut DocumentMut,
	kind: PythonVersionedFileKind,
	owner_version: Option<&str>,
	versioned_deps: &BTreeMap<String, String>,
) {
	match kind {
		PythonVersionedFileKind::Manifest => {
			update_project_version(document, owner_version);
			update_project_dependencies(document, versioned_deps);
		}
		PythonVersionedFileKind::Lock => {
			// Lock files (uv.lock, poetry.lock) are complex and fragile to
			// mutate directly. Prefer running lockfile commands (`uv lock` or
			// `poetry lock --no-update`) which re-resolve the full dependency
			// graph after manifest versions are updated.
		}
	}
}

fn update_project_version(document: &mut DocumentMut, owner_version: Option<&str>) {
	let Some(version) = owner_version else {
		return;
	};
	let Some(project) = document
		.get_mut("project")
		.and_then(Item::as_table_like_mut)
	else {
		return;
	};
	if let Some(existing) = project.get_mut("version") {
		if let Some(existing_value) = existing.as_value() {
			let mut new_value = toml_edit::Value::from(version);
			*new_value.decor_mut() = existing_value.decor().clone();
			*existing = Item::Value(new_value);
		}
	}
}

fn update_project_dependencies(
	document: &mut DocumentMut,
	versioned_deps: &BTreeMap<String, String>,
) {
	if versioned_deps.is_empty() {
		return;
	}
	let Some(project) = document
		.get_mut("project")
		.and_then(Item::as_table_like_mut)
	else {
		return;
	};
	let Some(deps) = project.get_mut("dependencies").and_then(Item::as_array_mut) else {
		return;
	};
	for item in deps.iter_mut() {
		let Some(spec) = item.as_str() else {
			continue;
		};
		if let Some(updated) = update_dependency_specifier(spec, versioned_deps) {
			let mut new_value = toml_edit::Value::from(updated);
			*new_value.decor_mut() = item.decor().clone();
			*item = new_value;
		}
	}
}

/// Update a PEP 508 dependency specifier if the package name matches a
/// versioned dependency.
///
/// Input:  `"my-core>=1.0.0"`, deps = `{"my-core": ">=2.0.0"}`
/// Output: `Some("my-core>=2.0.0")`
fn update_dependency_specifier(
	spec: &str,
	versioned_deps: &BTreeMap<String, String>,
) -> Option<String> {
	let name = parse_dependency_name(spec)?;
	let normalized = normalize_python_package_name(&name);
	let version = versioned_deps.get(&normalized)?;
	// Replace everything after the package name with the new version constraint.
	// Preserve extras (e.g., `httpx[cli]>=1.0` → `httpx[cli]>=2.0`).
	let after_name = &spec[name.len()..];
	let extras_end = after_name
		.find(|ch: char| ch != '[' && ch != ']' && !ch.is_alphanumeric() && ch != ',')
		.unwrap_or(0);
	let extras = &after_name[..extras_end];
	Some(format!("{name}{extras}{version}"))
}

/// Parse the package name from a PEP 508 dependency specifier.
///
/// `"httpx>=0.20.0"` → `"httpx"`
/// `"httpx[cli]>=0.20.0"` → `"httpx"`
/// `"Django>2.1; os_name != 'nt'"` → `"Django"`
fn parse_dependency_name(spec: &str) -> Option<String> {
	let name: String = spec
		.chars()
		.take_while(|ch| ch.is_alphanumeric() || *ch == '-' || *ch == '_' || *ch == '.')
		.collect();
	if name.is_empty() {
		None
	} else {
		Some(name)
	}
}

/// Normalize a Python package name per PEP 503: lowercase, replace [-_.]+
/// with a single hyphen.
fn normalize_python_package_name(name: &str) -> String {
	let mut result = String::with_capacity(name.len());
	let mut prev_was_separator = false;
	for ch in name.chars() {
		if ch == '-' || ch == '_' || ch == '.' {
			if !prev_was_separator && !result.is_empty() {
				result.push('-');
			}
			prev_was_separator = true;
		} else {
			result.push(ch.to_ascii_lowercase());
			prev_was_separator = false;
		}
	}
	result
}

#[tracing::instrument(skip_all)]
pub fn discover_python_packages(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let mut packages = Vec::new();
	let mut warnings = Vec::new();
	let mut included_manifests = BTreeSet::new();

	// Phase 1: uv workspace discovery
	let root_manifest = root.join(PYPROJECT_FILE);
	if root_manifest.exists() {
		let workspace_members = match parse_uv_workspace_members(&root_manifest) {
			Ok(members) => members,
			Err(error) => {
				warnings.push(format!("skipped {}: {error}", root_manifest.display()));
				None
			}
		};
		if let Some(workspace_members) = workspace_members {
			// Exclude the workspace root manifest from standalone discovery
			included_manifests.insert(normalize_path(&root_manifest));
			let member_manifests =
				expand_workspace_members(root, &workspace_members, &mut warnings);
			for manifest_path in member_manifests {
				if let Some(package) = parse_python_package(&manifest_path, root)? {
					included_manifests.insert(normalize_path(&manifest_path));
					packages.push(package);
				}
			}
		}
	}

	// Phase 2: scan for standalone pyproject.toml files not already discovered.
	// Parse errors are treated as warnings since the walker picks up all
	// pyproject.toml files including test fixtures and generated files.
	for manifest_path in find_all_pyproject_files(root) {
		let normalized = normalize_path(&manifest_path);
		if included_manifests.contains(&normalized) {
			continue;
		}
		let manifest_dir = manifest_path
			.parent()
			.unwrap_or_else(|| Path::new("."))
			.to_path_buf();
		match parse_python_package(&manifest_path, &manifest_dir) {
			Ok(Some(package)) => packages.push(package),
			Ok(None) => {}
			Err(error) => {
				warnings.push(format!("skipped {}: {error}", manifest_path.display()));
			}
		}
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	tracing::debug!(packages = packages.len(), "discovered python packages");

	Ok(AdapterDiscovery { packages, warnings })
}

fn parse_uv_workspace_members(manifest_path: &Path) -> MonochangeResult<Option<Vec<String>>> {
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

	let members = parsed
		.get("tool")
		.and_then(|tool| tool.get("uv"))
		.and_then(|uv| uv.get("workspace"))
		.and_then(|workspace| workspace.get("members"))
		.and_then(Value::as_array)
		.map(|members| {
			members
				.iter()
				.filter_map(Value::as_str)
				.map(ToString::to_string)
				.collect()
		});

	Ok(members)
}

fn expand_workspace_members(
	root: &Path,
	patterns: &[String],
	warnings: &mut Vec<String>,
) -> BTreeSet<PathBuf> {
	let mut manifests = BTreeSet::new();

	for pattern in patterns {
		let joined = root.join(pattern).to_string_lossy().to_string();
		let matches: Vec<PathBuf> = glob(&joined)
			.into_iter()
			.flat_map(|paths| paths.filter_map(Result::ok))
			.map(|path| normalize_path(&path))
			.collect();

		if matches.is_empty() {
			warnings.push(format!(
				"uv workspace pattern `{pattern}` under {} matched no packages",
				root.display()
			));
		}

		for matched_path in matches {
			let manifest_path = if matched_path.is_dir() {
				matched_path.join(PYPROJECT_FILE)
			} else if matched_path.file_name().and_then(|name| name.to_str())
				== Some(PYPROJECT_FILE)
			{
				matched_path
			} else {
				continue;
			};

			if manifest_path.exists() {
				manifests.insert(manifest_path);
			}
		}
	}

	manifests
}

fn parse_python_package(
	manifest_path: &Path,
	workspace_root: &Path,
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

	// Prefer [project] (PEP 621) over [tool.poetry]
	let (name, version, dependencies) = if let Some(project) = parsed.get("project") {
		let name = project.get("name").and_then(Value::as_str);
		let version = project
			.get("version")
			.and_then(Value::as_str)
			.and_then(parse_pep440_as_semver);
		let dynamic = project
			.get("dynamic")
			.and_then(Value::as_array)
			.is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("version")));
		let version = if dynamic { None } else { version };
		let deps = parse_pep621_dependencies(project);
		(name, version, deps)
	} else if let Some(poetry) = parsed.get("tool").and_then(|tool| tool.get("poetry")) {
		let name = poetry.get("name").and_then(Value::as_str);
		let version = poetry
			.get("version")
			.and_then(Value::as_str)
			.and_then(parse_pep440_as_semver);
		let deps = parse_poetry_dependencies(poetry);
		(name, version, deps)
	} else {
		return Ok(None);
	};

	let Some(name) = name else {
		return Ok(None);
	};

	let mut record = PackageRecord::new(
		Ecosystem::Python,
		name,
		normalize_path(manifest_path),
		normalize_path(workspace_root),
		version,
		PublishState::Public,
	);
	record.declared_dependencies = dependencies;
	Ok(Some(record))
}

fn parse_pep621_dependencies(project: &Value) -> Vec<PackageDependency> {
	let mut deps = Vec::new();

	if let Some(dep_array) = project.get("dependencies").and_then(Value::as_array) {
		for dep in dep_array {
			if let Some(spec) = dep.as_str() {
				if let Some(name) = parse_dependency_name(spec) {
					deps.push(PackageDependency {
						name: normalize_python_package_name(&name),
						kind: DependencyKind::Runtime,
						version_constraint: extract_version_constraint(spec, &name),
						optional: false,
					});
				}
			}
		}
	}

	if let Some(optional_deps) = project
		.get("optional-dependencies")
		.and_then(Value::as_table)
	{
		for (_group, group_deps) in optional_deps {
			if let Some(dep_array) = group_deps.as_array() {
				for dep in dep_array {
					if let Some(spec) = dep.as_str() {
						if let Some(name) = parse_dependency_name(spec) {
							deps.push(PackageDependency {
								name: normalize_python_package_name(&name),
								kind: DependencyKind::Development,
								version_constraint: extract_version_constraint(spec, &name),
								optional: true,
							});
						}
					}
				}
			}
		}
	}

	deps
}

fn parse_poetry_dependencies(poetry: &Value) -> Vec<PackageDependency> {
	let mut deps = Vec::new();

	if let Some(dep_table) = poetry.get("dependencies").and_then(Value::as_table) {
		for (name, value) in dep_table {
			if name == "python" {
				continue;
			}
			let constraint = match value {
				Value::String(version) => Some(version.clone()),
				Value::Table(table) => table
					.get("version")
					.and_then(Value::as_str)
					.map(ToString::to_string),
				_ => None,
			};
			deps.push(PackageDependency {
				name: normalize_python_package_name(name),
				kind: DependencyKind::Runtime,
				version_constraint: constraint,
				optional: false,
			});
		}
	}

	// Parse grouped dev dependencies
	if let Some(groups) = poetry.get("group").and_then(Value::as_table) {
		for (_group_name, group) in groups {
			if let Some(group_deps) = group
				.as_table()
				.and_then(|table| table.get("dependencies"))
				.and_then(Value::as_table)
			{
				for (name, value) in group_deps {
					let constraint = match value {
						Value::String(version) => Some(version.clone()),
						Value::Table(table) => table
							.get("version")
							.and_then(Value::as_str)
							.map(ToString::to_string),
						_ => None,
					};
					deps.push(PackageDependency {
						name: normalize_python_package_name(name),
						kind: DependencyKind::Development,
						version_constraint: constraint,
						optional: false,
					});
				}
			}
		}
	}

	deps
}

fn extract_version_constraint(spec: &str, name: &str) -> Option<String> {
	let rest = spec.get(name.len()..)?;
	// Skip extras like [cli]
	let after_extras = if rest.starts_with('[') {
		rest.find(']').map_or(rest, |end| &rest[end + 1..])
	} else {
		rest
	};
	let constraint = after_extras.split(';').next().unwrap_or("").trim();
	if constraint.is_empty() {
		None
	} else {
		Some(constraint.to_string())
	}
}

/// Parse a PEP 440 version string as semver where possible.
///
/// Handles common cases like `1.2.3`, `1.2`, `0.1.0`. Ignores PEP 440
/// pre-release suffixes (`a1`, `b2`, `rc1`, `.post1`, `.dev0`) that don't
/// map cleanly to semver.
fn parse_pep440_as_semver(version: &str) -> Option<Version> {
	// Try direct semver parse first
	if let Ok(version) = Version::parse(version) {
		return Some(version);
	}
	// Try adding .0 for two-part versions like "1.2"
	let parts: Vec<&str> = version.split('.').collect();
	match parts.len() {
		2 => {
			let extended = format!("{version}.0");
			Version::parse(&extended).ok()
		}
		_ => None,
	}
}

fn find_all_pyproject_files(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == PYPROJECT_FILE)
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

fn should_descend(entry: &DirEntry) -> bool {
	let file_name = entry.file_name().to_string_lossy();
	!matches!(
		file_name.as_ref(),
		".git"
			| ".venv" | "venv"
			| "__pycache__"
			| ".mypy_cache"
			| ".ruff_cache"
			| ".pytest_cache"
			| "node_modules"
			| "target"
			| ".devenv"
			| "book" | ".tox"
			| "dist" | "build"
			| ".eggs" | "*.egg-info"
	)
}

#[cfg(test)]
mod __tests;
