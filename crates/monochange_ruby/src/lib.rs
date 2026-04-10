#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_ruby`
//!
//! `monochange_ruby` discovers Ruby gems from `.gemspec` files and resolves
//! versions from `version.rb` constants.
//!
//! ## Why use it?
//!
//! - discover Ruby gems across monorepo directories with one adapter
//! - normalize gem manifests and dependency edges for the shared planner
//! - infer `bundle lock --update` as the default lockfile refresh command
//!
//! ## Public entry points
//!
//! - `discover_ruby_gems(root)` discovers Ruby gems
//! - `RubyAdapter` exposes the shared adapter interface
//!
//! ## Scope
//!
//! - `.gemspec` file scanning for gem discovery
//! - `version.rb` parsing for version constants
//! - gemspec dependency extraction (`add_dependency`, `add_runtime_dependency`,
//!   `add_development_dependency`)
//! - `Gemfile.lock` lockfile discovery
//! - `bundle lock --update` command inference

use std::fs;
use std::path::Path;
use std::path::PathBuf;

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
use regex::Regex;
use semver::Version;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const GEMFILE_LOCK: &str = "Gemfile.lock";

pub struct RubyAdapter;

#[must_use]
pub const fn adapter() -> RubyAdapter {
	RubyAdapter
}

impl EcosystemAdapter for RubyAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Ruby
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_ruby_gems(root)
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RubyVersionedFileKind {
	Gemspec,
	VersionFile,
	Lock,
}

#[must_use]
pub fn supported_versioned_file_kind(path: &Path) -> Option<RubyVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();

	if file_name.ends_with(".gemspec") {
		return Some(RubyVersionedFileKind::Gemspec);
	}
	if file_name == "version.rb" {
		return Some(RubyVersionedFileKind::VersionFile);
	}
	if file_name == GEMFILE_LOCK {
		return Some(RubyVersionedFileKind::Lock);
	}
	None
}

pub fn discover_lockfiles(package: &PackageRecord) -> Vec<PathBuf> {
	let manifest_dir = package
		.manifest_path
		.parent()
		.map_or_else(|| package.workspace_root.clone(), Path::to_path_buf);
	[manifest_dir.join(GEMFILE_LOCK)]
		.into_iter()
		.filter(|path| path.exists())
		.collect()
}

pub fn default_lockfile_commands(package: &PackageRecord) -> Vec<LockfileCommandExecution> {
	let lockfiles = discover_lockfiles(package);
	if lockfiles.is_empty() {
		return Vec::new();
	}
	vec![LockfileCommandExecution {
		command: "bundle lock --update".to_string(),
		cwd: package
			.manifest_path
			.parent()
			.unwrap_or(&package.workspace_root)
			.to_path_buf(),
		shell: ShellConfig::None,
	}]
}

/// Update a Ruby `version.rb` file by replacing the VERSION constant value.
///
/// Matches patterns like:
/// - `VERSION = "1.2.3"`
/// - `VERSION = '1.2.3'`
/// - `  VERSION = "1.2.3"`
pub fn update_version_file_text(contents: &str, new_version: &str) -> String {
	// Try double-quoted first, then single-quoted. Rust regex doesn't support
	// backreferences so we use two separate patterns.
	let double_re = Regex::new(r#"(?m)(VERSION\s*=\s*)"(\d+\.\d+\.\d+[^"]*)""#)
		.unwrap_or_else(|_| unreachable!("double-quote version regex should be valid"));
	if double_re.is_match(contents) {
		return double_re
			.replace(contents, |caps: &regex::Captures<'_>| {
				let prefix = caps.get(1).map_or("VERSION = ", |m| m.as_str());
				format!("{prefix}\"{new_version}\"")
			})
			.to_string();
	}

	let single_re = Regex::new(r"(?m)(VERSION\s*=\s*)'(\d+\.\d+\.\d+[^']*)'")
		.unwrap_or_else(|_| unreachable!("single-quote version regex should be valid"));
	single_re
		.replace(contents, |caps: &regex::Captures<'_>| {
			let prefix = caps.get(1).map_or("VERSION = ", |m| m.as_str());
			format!("{prefix}'{new_version}'")
		})
		.to_string()
}

pub fn discover_ruby_gems(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let mut packages = Vec::new();
	let mut warnings = Vec::new();

	for gemspec_path in find_all_gemspec_files(root) {
		match parse_ruby_gem(&gemspec_path, root) {
			Ok(Some(package)) => packages.push(package),
			Ok(None) => {}
			Err(error) => {
				warnings.push(format!("skipped {}: {error}", gemspec_path.display()));
			}
		}
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	Ok(AdapterDiscovery { packages, warnings })
}

fn parse_ruby_gem(gemspec_path: &Path, root: &Path) -> MonochangeResult<Option<PackageRecord>> {
	let contents = fs::read_to_string(gemspec_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			gemspec_path.display()
		))
	})?;

	let name = parse_gem_name(&contents);
	let Some(name) = name else {
		return Ok(None);
	};

	let gem_dir = gemspec_path.parent().unwrap_or_else(|| Path::new("."));

	// Try to find the version from a version.rb file
	let version = find_version_in_gem_dir(gem_dir, &name);

	let mut record = PackageRecord::new(
		Ecosystem::Ruby,
		&name,
		normalize_path(gemspec_path),
		normalize_path(root),
		version,
		PublishState::Public,
	);

	record.declared_dependencies = parse_gemspec_dependencies(&contents);

	Ok(Some(record))
}

/// Parse the gem name from a gemspec file.
///
/// Looks for patterns like:
/// - `spec.name = "my_gem"`
/// - `spec.name    = 'my-gem'`
/// - `s.name = "gem_name"`
fn parse_gem_name(contents: &str) -> Option<String> {
	let re = Regex::new(r#"(?m)\.\s*name\s*=\s*["']([^"']+)["']"#).ok()?;
	re.captures(contents)
		.and_then(|caps| caps.get(1))
		.map(|m| m.as_str().to_string())
}

/// Find the version constant in the gem's `lib/` directory.
///
/// Searches for `VERSION = "x.y.z"` in:
/// 1. `lib/<gem_name>/version.rb`
/// 2. `lib/version.rb`
/// 3. Any `version.rb` under `lib/`
fn find_version_in_gem_dir(gem_dir: &Path, gem_name: &str) -> Option<Version> {
	// Normalize gem name for directory lookup (replace - with _)
	let dir_name = gem_name.replace('-', "_");

	let candidates = [
		gem_dir.join(format!("lib/{dir_name}/version.rb")),
		gem_dir.join("lib/version.rb"),
	];

	for candidate in &candidates {
		if let Some(version) = parse_version_from_file(candidate) {
			return Some(version);
		}
	}

	// Fallback: search for any version.rb under lib/
	let lib_dir = gem_dir.join("lib");
	if lib_dir.is_dir() {
		for entry in WalkDir::new(&lib_dir)
			.max_depth(3)
			.into_iter()
			.filter_map(Result::ok)
		{
			if entry.file_name() == "version.rb" {
				if let Some(version) = parse_version_from_file(entry.path()) {
					return Some(version);
				}
			}
		}
	}

	None
}

/// Parse a VERSION constant from a Ruby file.
fn parse_version_from_file(path: &Path) -> Option<Version> {
	let contents = fs::read_to_string(path).ok()?;
	parse_version_constant(&contents)
}

/// Extract a semver version from a Ruby VERSION constant.
///
/// Matches `VERSION = "1.2.3"` or `VERSION = '1.2.3'`.
pub fn parse_version_constant(contents: &str) -> Option<Version> {
	let re = Regex::new(r#"(?m)VERSION\s*=\s*["'](\d+\.\d+\.\d+)["']"#).ok()?;
	re.captures(contents)
		.and_then(|caps| caps.get(1))
		.and_then(|m| Version::parse(m.as_str()).ok())
}

/// Parse dependencies from a gemspec file.
///
/// Matches patterns like:
/// - `spec.add_dependency "rails", "~> 7.0"`
/// - `spec.add_runtime_dependency 'redis', ">= 4.0"`
/// - `s.add_development_dependency "rspec", "~> 3.0"`
fn parse_gemspec_dependencies(contents: &str) -> Vec<PackageDependency> {
	let dep_re = Regex::new(
		r#"(?m)\.\s*add_(runtime_)?dependency\s+["']([^"']+)["'](?:\s*,\s*["']([^"']+)["'])*"#,
	);
	let dev_dep_re = Regex::new(
		r#"(?m)\.\s*add_development_dependency\s+["']([^"']+)["'](?:\s*,\s*["']([^"']+)["'])*"#,
	);

	let mut deps = Vec::new();

	if let Ok(re) = &dep_re {
		for caps in re.captures_iter(contents) {
			let name = caps.get(2).map(|m| m.as_str().to_string());
			let constraint = caps.get(3).map(|m| m.as_str().to_string());
			if let Some(name) = name {
				deps.push(PackageDependency {
					name,
					kind: DependencyKind::Runtime,
					version_constraint: constraint,
					optional: false,
				});
			}
		}
	}

	if let Ok(re) = &dev_dep_re {
		for caps in re.captures_iter(contents) {
			let name = caps.get(1).map(|m| m.as_str().to_string());
			let constraint = caps.get(2).map(|m| m.as_str().to_string());
			if let Some(name) = name {
				deps.push(PackageDependency {
					name,
					kind: DependencyKind::Development,
					version_constraint: constraint,
					optional: false,
				});
			}
		}
	}

	deps
}

fn find_all_gemspec_files(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| {
			entry
				.file_name()
				.to_str()
				.is_some_and(|name| name.ends_with(".gemspec"))
		})
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

fn should_descend(entry: &DirEntry) -> bool {
	let file_name = entry.file_name().to_string_lossy();
	!matches!(
		file_name.as_ref(),
		".git"
			| "vendor"
			| "node_modules"
			| "target"
			| ".devenv"
			| "book" | ".bundle"
			| "tmp" | "pkg"
	)
}

#[cfg(test)]
mod __tests;
