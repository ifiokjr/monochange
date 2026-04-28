#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_go`
//!
//! `monochange_go` discovers Go modules from `go.mod` files and resolves
//! versions from git tags.
//!
//! ## Why use it?
//!
//! - discover Go modules in single-module and multi-module repositories
//! - normalize Go module dependency edges for the shared planner
//! - infer `go mod tidy` as the default lockfile refresh command
//!
//! ## Best for
//!
//! - building Go-aware discovery flows without the full CLI
//! - converting Go module structure into shared `monochange_core` records
//! - managing multi-module monorepo releases with path-prefixed git tags
//!
//! ## Public entry points
//!
//! - `discover_go_modules(root)` discovers Go modules
//! - `GoAdapter` exposes the shared adapter interface
//!
//! ## Scope
//!
//! - `go.mod` parsing for module path, go version, and require directives
//! - multi-module repository detection
//! - `go mod tidy` lockfile command inference
//! - `go.sum` lockfile discovery

use std::fs;
use std::path::Path;
use std::path::PathBuf;

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
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const GO_MOD_FILE: &str = "go.mod";
pub const GO_SUM_FILE: &str = "go.sum";

pub struct GoAdapter;

#[must_use]
pub const fn adapter() -> GoAdapter {
	GoAdapter
}

impl EcosystemAdapter for GoAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Go
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_go_modules(root)
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GoVersionedFileKind {
	GoMod,
	GoSum,
}

#[must_use]
pub fn supported_versioned_file_kind(path: &Path) -> Option<GoVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();
	match file_name {
		GO_MOD_FILE => Some(GoVersionedFileKind::GoMod),
		GO_SUM_FILE => Some(GoVersionedFileKind::GoSum),
		_ => None,
	}
}

pub fn discover_lockfiles(package: &PackageRecord) -> Vec<PathBuf> {
	let manifest_dir = package
		.manifest_path
		.parent()
		.map_or_else(|| package.workspace_root.clone(), Path::to_path_buf);
	[manifest_dir.join(GO_SUM_FILE)]
		.into_iter()
		.filter(|path| path.exists())
		.collect()
}

pub fn default_lockfile_commands(package: &PackageRecord) -> Vec<LockfileCommandExecution> {
	let manifest_dir = package
		.manifest_path
		.parent()
		.unwrap_or(&package.workspace_root)
		.to_path_buf();
	// Always infer `go mod tidy` for Go modules — it updates both go.mod
	// (removes unused, adds missing) and go.sum (refreshes checksums).
	vec![LockfileCommandExecution {
		command: "go mod tidy".to_string(),
		cwd: manifest_dir,
		shell: ShellConfig::None,
	}]
}

/// Update a `go.mod` file's require directives for cross-module dependencies.
///
/// When module `shared` bumps to `v1.3.0`, this updates:
/// ```text
/// require github.com/org/repo/shared v1.2.0
/// ```
/// to:
/// ```text
/// require github.com/org/repo/shared v1.3.0
/// ```
pub fn update_go_mod_text(
	contents: &str,
	versioned_deps: &std::collections::BTreeMap<String, String>,
) -> String {
	if versioned_deps.is_empty() {
		return contents.to_string();
	}

	let mut result = String::with_capacity(contents.len());
	for line in contents.lines() {
		let updated = update_require_line(line, versioned_deps);
		result.push_str(&updated);
		result.push('\n');
	}
	// Preserve trailing newline status from the original content
	if !contents.ends_with('\n') && result.ends_with('\n') {
		result.pop();
	}
	result
}

/// Update a single `require` line if it matches a versioned dependency.
///
/// Handles both standalone require lines and lines inside a require block:
/// - `require github.com/org/shared v1.0.0`
/// - `  github.com/org/shared v1.0.0`
/// - `  github.com/org/shared v1.0.0 // indirect`
fn update_require_line(
	line: &str,
	versioned_deps: &std::collections::BTreeMap<String, String>,
) -> String {
	let trimmed = line.trim();

	// Skip empty lines, comments, and non-require directives
	if trimmed.is_empty()
		|| trimmed.starts_with("//")
		|| trimmed.starts_with("module ")
		|| trimmed.starts_with("go ")
		|| trimmed.starts_with("replace ")
		|| trimmed.starts_with("exclude ")
		|| trimmed.starts_with("retract ")
		|| trimmed == "require ("
		|| trimmed == ")"
		|| trimmed == "require"
	{
		return line.to_string();
	}

	// Handle `require module/path v1.2.3` (single-line require)
	let parts: Vec<&str> = if let Some(rest) = trimmed.strip_prefix("require ") {
		rest.split_whitespace().collect()
	} else {
		// Inside a require block: `	module/path v1.2.3`
		trimmed.split_whitespace().collect()
	};

	if parts.len() < 2 {
		return line.to_string();
	}

	let module_path = parts.first().copied().unwrap_or_default();

	// Extract the module name (last path segment) for matching
	let module_name = module_path.rsplit('/').next().unwrap_or(module_path);

	// Strip version suffix from module name (e.g., `shared/v2` → `shared`)
	let clean_name = module_name
		.strip_prefix('v')
		.and_then(|rest| {
			rest.chars()
				.all(|ch| ch.is_ascii_digit())
				.then_some(module_name)
		})
		.map_or(module_name, |_| {
			module_path.rsplit('/').nth(1).unwrap_or(module_path)
		});

	if let Some(new_version) = versioned_deps.get(clean_name) {
		// Ensure the version has a `v` prefix for Go
		let go_version = if new_version.starts_with('v') {
			new_version.clone()
		} else {
			format!("v{new_version}")
		};

		// Preserve the original line structure (indentation, comments)
		let prefix = &line[..line.len() - line.trim_start().len()];
		let comment = if let Some(comment_start) = trimmed.find("//") {
			let after_version = &trimmed[comment_start..];
			format!(" {after_version}")
		} else {
			String::new()
		};

		if trimmed.starts_with("require ") {
			format!("{prefix}require {module_path} {go_version}{comment}")
		} else {
			format!("{prefix}{module_path} {go_version}{comment}")
		}
	} else {
		line.to_string()
	}
}

#[tracing::instrument(skip_all)]
pub fn discover_go_modules(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	for go_mod_path in find_all_go_mod_files(root) {
		match parse_go_module(&go_mod_path, root) {
			Ok(Some(package)) => packages.push(package),
			Ok(None) => {}
			Err(error) => {
				warnings.push(format!("skipped {}: {error}", go_mod_path.display()));
			}
		}
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	tracing::debug!(packages = packages.len(), "discovered go modules");

	Ok(AdapterDiscovery { packages, warnings })
}

fn parse_go_module(go_mod_path: &Path, root: &Path) -> MonochangeResult<Option<PackageRecord>> {
	let contents = fs::read_to_string(go_mod_path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", go_mod_path.display()))
	})?;

	let module_path = parse_module_path(&contents);
	let Some(module_path) = module_path else {
		return Ok(None);
	};

	// Derive the package name from the module path (last non-version segment)
	let name = derive_module_name(&module_path);

	let manifest_dir = go_mod_path.parent().unwrap_or_else(|| Path::new("."));

	let mut record = PackageRecord::new(
		Ecosystem::Go,
		&name,
		normalize_path(go_mod_path),
		normalize_path(root),
		None, // Go versions come from git tags, not manifest files
		PublishState::Public,
	);

	// Store the full module path as metadata for tag resolution
	record
		.metadata
		.insert("module_path".to_string(), module_path.clone());

	// Store the relative path for tag prefix computation
	let normalized_dir = normalize_path(manifest_dir);
	let normalized_root = normalize_path(root);
	let relative_path = normalized_dir
		.strip_prefix(&normalized_root)
		.unwrap_or(Path::new(""))
		.to_string_lossy()
		.to_string();
	if !relative_path.is_empty() && relative_path != "." {
		record
			.metadata
			.insert("relative_path".to_string(), relative_path);
	}

	record.declared_dependencies = parse_require_directives(&contents);

	Ok(Some(record))
}

/// Parse the `module` directive from a go.mod file.
///
/// ```text
/// module github.com/org/repo/api
/// ```
fn parse_module_path(contents: &str) -> Option<String> {
	for line in contents.lines() {
		let trimmed = line.trim();
		if let Some(path) = trimmed.strip_prefix("module ") {
			return Some(path.trim().to_string());
		}
	}
	None
}

/// Derive a human-friendly module name from a Go module path.
///
/// `github.com/org/repo` → `repo`
/// `github.com/org/repo/api` → `api`
/// `github.com/org/repo/api/v2` → `api`
fn derive_module_name(module_path: &str) -> String {
	let segments: Vec<&str> = module_path.split('/').collect();

	// Walk backwards to find the first non-version segment
	for segment in segments.iter().rev() {
		if !is_major_version_suffix(segment) {
			return (*segment).to_string();
		}
	}

	// Fallback to the full path if everything is a version
	module_path.to_string()
}

/// Check if a path segment is a major version suffix like `v2`, `v3`, etc.
/// In Go, only v2+ appear as import path suffixes. `v0` and `v1` are not
/// path suffixes.
fn is_major_version_suffix(segment: &str) -> bool {
	segment.strip_prefix('v').is_some_and(|rest| {
		!rest.is_empty()
			&& rest.chars().all(|ch| ch.is_ascii_digit())
			&& rest.parse::<u64>().is_ok_and(|n| n >= 2)
	})
}

/// Parse `require` directives from go.mod content.
fn parse_require_directives(contents: &str) -> Vec<PackageDependency> {
	let mut deps = Vec::new();
	let mut in_require_block = false;

	for line in contents.lines() {
		let trimmed = line.trim();

		if trimmed == "require (" {
			in_require_block = true;
			continue;
		}
		if trimmed == ")" {
			in_require_block = false;
			continue;
		}

		// Handle single-line require: `require module/path v1.2.3`
		if let Some(rest) = trimmed.strip_prefix("require ") {
			if !rest.starts_with('(')
				&& let Some(dep) = parse_require_entry(rest)
			{
				deps.push(dep);
			}
			continue;
		}

		// Handle entries inside require block
		if in_require_block && let Some(dep) = parse_require_entry(trimmed) {
			deps.push(dep);
		}
	}

	deps
}

/// Parse a single require entry like `github.com/org/shared v1.2.3 // indirect`.
fn parse_require_entry(entry: &str) -> Option<PackageDependency> {
	let parts: Vec<&str> = entry.split_whitespace().collect();
	if parts.len() < 2 {
		return None;
	}

	let module_path = *parts.first()?;
	let version_str = *parts.get(1)?;
	let is_indirect = parts.contains(&"indirect");

	// Extract the module name (last path segment, excluding version suffixes)
	let name = derive_module_name(module_path);

	// Parse version string, stripping the `v` prefix
	let constraint = version_str
		.strip_prefix('v')
		.map(ToString::to_string)
		.or_else(|| Some(version_str.to_string()));

	let kind = if is_indirect {
		DependencyKind::Development
	} else {
		DependencyKind::Runtime
	};

	Some(PackageDependency {
		name,
		kind,
		version_constraint: constraint,
		optional: false,
	})
}

/// Parse a Go semver version string like `v1.2.3` into a `semver::Version`.
pub fn parse_go_version(version_str: &str) -> Option<Version> {
	let stripped = version_str.strip_prefix('v').unwrap_or(version_str);
	Version::parse(stripped).ok()
}

fn find_all_go_mod_files(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == GO_MOD_FILE)
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

fn should_descend(entry: &DirEntry) -> bool {
	let file_name = entry.file_name().to_string_lossy();
	!matches!(
		file_name.as_ref(),
		".git" | "vendor" | "node_modules" | "target" | ".devenv" | "book" | "testdata"
	)
}

#[cfg(test)]
mod __tests;
