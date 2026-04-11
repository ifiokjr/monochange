#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_jvm`
//!
//! `monochange_jvm` discovers JVM projects from Gradle multi-project builds
//! and Maven multi-module projects.
//!
//! ## Why use it?
//!
//! - discover Gradle and Maven projects in monorepos with one adapter
//! - normalize JVM project manifests and dependency edges for the shared
//!   planner
//! - infer `./gradlew dependencies --write-locks` or `go mod tidy` as the
//!   default lockfile refresh command
//!
//! ## Public entry points
//!
//! - `discover_jvm_projects(root)` discovers JVM projects
//! - `JvmAdapter` exposes the shared adapter interface
//!
//! ## Scope
//!
//! - Gradle `settings.gradle.kts` / `settings.gradle` multi-project parsing
//! - `build.gradle.kts` / `build.gradle` version and dependency extraction
//! - Gradle Version Catalogs (`gradle/libs.versions.toml`) version management
//! - Maven `pom.xml` multi-module and version parsing
//! - lockfile command inference for Gradle projects

use std::collections::BTreeSet;
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
use toml_edit::DocumentMut;
use toml_edit::Item;
use walkdir::DirEntry;
use walkdir::WalkDir;

pub const GRADLE_SETTINGS_KTS: &str = "settings.gradle.kts";
pub const GRADLE_SETTINGS: &str = "settings.gradle";
pub const GRADLE_BUILD_KTS: &str = "build.gradle.kts";
pub const GRADLE_BUILD: &str = "build.gradle";
pub const GRADLE_LOCKFILE: &str = "gradle.lockfile";
pub const VERSION_CATALOG: &str = "libs.versions.toml";
pub const MAVEN_POM: &str = "pom.xml";

pub struct JvmAdapter;

#[must_use]
pub const fn adapter() -> JvmAdapter {
	JvmAdapter
}

impl EcosystemAdapter for JvmAdapter {
	fn ecosystem(&self) -> Ecosystem {
		Ecosystem::Jvm
	}

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery> {
		discover_jvm_projects(root)
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum JvmVersionedFileKind {
	GradleBuild,
	VersionCatalog,
	GradleLock,
	MavenPom,
}

#[must_use]
pub fn supported_versioned_file_kind(path: &Path) -> Option<JvmVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();
	match file_name {
		GRADLE_BUILD_KTS | GRADLE_BUILD => Some(JvmVersionedFileKind::GradleBuild),
		VERSION_CATALOG => Some(JvmVersionedFileKind::VersionCatalog),
		GRADLE_LOCKFILE => Some(JvmVersionedFileKind::GradleLock),
		MAVEN_POM => Some(JvmVersionedFileKind::MavenPom),
		_ => None,
	}
}

pub fn discover_lockfiles(package: &PackageRecord) -> Vec<PathBuf> {
	let manifest_dir = package
		.manifest_path
		.parent()
		.map_or_else(|| package.workspace_root.clone(), Path::to_path_buf);
	let mut lockfiles: Vec<PathBuf> = [manifest_dir.join(GRADLE_LOCKFILE)]
		.into_iter()
		.filter(|path| path.exists())
		.collect();
	// Also check workspace root for a shared lockfile
	if manifest_dir != package.workspace_root {
		lockfiles.extend(
			[package.workspace_root.join(GRADLE_LOCKFILE)]
				.into_iter()
				.filter(|path| path.exists()),
		);
	}
	lockfiles.dedup();
	lockfiles
}

pub fn default_lockfile_commands(package: &PackageRecord) -> Vec<LockfileCommandExecution> {
	let build_tool = detect_build_tool(package);
	match build_tool {
		BuildTool::Gradle => {
			let cwd = package
				.manifest_path
				.parent()
				.unwrap_or(&package.workspace_root)
				.to_path_buf();
			// Prefer gradlew wrapper if it exists
			let command = if cwd.join("gradlew").exists() {
				"./gradlew dependencies --write-locks"
			} else {
				"gradle dependencies --write-locks"
			};
			vec![LockfileCommandExecution {
				command: command.to_string(),
				cwd,
				shell: ShellConfig::None,
			}]
		}
		BuildTool::Maven => Vec::new(), // Maven has no native lockfile
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildTool {
	Gradle,
	Maven,
}

fn detect_build_tool(package: &PackageRecord) -> BuildTool {
	let file_name = package
		.manifest_path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();
	if file_name == MAVEN_POM {
		BuildTool::Maven
	} else {
		BuildTool::Gradle
	}
}

/// Update a Gradle `build.gradle.kts` file's version declaration.
///
/// Replaces `version = "1.2.3"` with the new version.
pub fn update_gradle_build_version(contents: &str, new_version: &str) -> String {
	let re = Regex::new(r#"(?m)(version\s*=\s*)"([^"]+)""#)
		.unwrap_or_else(|_| unreachable!("gradle version regex should be valid"));
	re.replace(contents, |caps: &regex::Captures<'_>| {
		let prefix = caps.get(1).map_or("version = ", |m| m.as_str());
		format!("{prefix}\"{new_version}\"")
	})
	.to_string()
}

/// Update version entries in a Gradle Version Catalog (`libs.versions.toml`).
pub fn update_version_catalog_text(
	contents: &str,
	versioned_deps: &std::collections::BTreeMap<String, String>,
) -> Result<String, toml_edit::TomlError> {
	if versioned_deps.is_empty() {
		return Ok(contents.to_string());
	}
	let mut document = contents.parse::<DocumentMut>()?;
	if let Some(versions) = document
		.get_mut("versions")
		.and_then(Item::as_table_like_mut)
	{
		for (name, version) in versioned_deps {
			if let Some(existing) = versions.get_mut(name) {
				if let Some(existing_value) = existing.as_value() {
					let mut new_value = toml_edit::Value::from(version.as_str());
					*new_value.decor_mut() = existing_value.decor().clone();
					*existing = Item::Value(new_value);
				}
			}
		}
	}
	Ok(document.to_string())
}

/// Update the `<version>` element in a Maven `pom.xml` file.
///
/// Only updates the top-level project version, not dependency versions.
pub fn update_pom_version(contents: &str, new_version: &str) -> String {
	let re = Regex::new(r"(?s)(<project[^>]*>.*?<version>)([^<]+)(</version>)")
		.unwrap_or_else(|_| unreachable!("pom version regex should be valid"));
	re.replace(contents, |caps: &regex::Captures<'_>| {
		let before = caps.get(1).map_or("", |m| m.as_str());
		let after = caps.get(3).map_or("", |m| m.as_str());
		format!("{before}{new_version}{after}")
	})
	.to_string()
}

pub fn discover_jvm_projects(root: &Path) -> MonochangeResult<AdapterDiscovery> {
	let mut packages = Vec::new();
	let mut warnings = Vec::new();
	let mut included_dirs = BTreeSet::new();

	// Phase 1: Gradle multi-project discovery via settings files
	for settings_path in find_settings_files(root) {
		let settings_dir = settings_path.parent().unwrap_or_else(|| Path::new("."));
		let subproject_names = parse_gradle_subprojects(&settings_path);
		included_dirs.insert(normalize_path(settings_dir));

		for subproject_name in &subproject_names {
			let subproject_dir = settings_dir.join(subproject_name);
			if !subproject_dir.is_dir() {
				warnings.push(format!(
					"Gradle subproject `{subproject_name}` not found at {}",
					subproject_dir.display()
				));
				continue;
			}
			let build_file = find_gradle_build_file(&subproject_dir);
			let Some(build_file) = build_file else {
				continue;
			};
			match parse_gradle_project(&build_file, root, subproject_name) {
				Ok(Some(package)) => {
					included_dirs.insert(normalize_path(&subproject_dir));
					packages.push(package);
				}
				Ok(None) => {}
				Err(error) => {
					warnings.push(format!("skipped {}: {error}", build_file.display()));
				}
			}
		}
	}

	// Phase 2: Maven multi-module discovery via pom.xml
	for pom_path in find_all_pom_files(root) {
		let pom_dir = pom_path.parent().unwrap_or_else(|| Path::new("."));
		let normalized = normalize_path(pom_dir);
		if included_dirs.contains(&normalized) {
			continue;
		}
		match parse_maven_project(&pom_path, root) {
			Ok(Some(package)) => {
				included_dirs.insert(normalized);
				packages.push(package);
			}
			Ok(None) => {}
			Err(error) => {
				warnings.push(format!("skipped {}: {error}", pom_path.display()));
			}
		}
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	Ok(AdapterDiscovery { packages, warnings })
}

fn find_settings_files(root: &Path) -> Vec<PathBuf> {
	let mut files = Vec::new();
	for name in [GRADLE_SETTINGS_KTS, GRADLE_SETTINGS] {
		let path = root.join(name);
		if path.exists() {
			files.push(path);
			break; // Prefer .kts over .gradle
		}
	}
	files
}

fn find_gradle_build_file(dir: &Path) -> Option<PathBuf> {
	let kts = dir.join(GRADLE_BUILD_KTS);
	if kts.exists() {
		return Some(kts);
	}
	let groovy = dir.join(GRADLE_BUILD);
	if groovy.exists() {
		return Some(groovy);
	}
	None
}

/// Parse `include(...)` directives from a Gradle settings file.
fn parse_gradle_subprojects(settings_path: &Path) -> Vec<String> {
	let contents = fs::read_to_string(settings_path).unwrap_or_default();
	let re = Regex::new(r#"include\s*\(\s*"([^"]+)"\s*(?:,\s*"([^"]+)"\s*)*\)"#)
		.unwrap_or_else(|_| unreachable!("include regex should be valid"));

	let mut subprojects = Vec::new();
	for caps in re.captures_iter(&contents) {
		for i in 1..caps.len() {
			if let Some(m) = caps.get(i) {
				let name = m.as_str().trim_start_matches(':');
				if !name.is_empty() {
					subprojects.push(name.to_string());
				}
			}
		}
	}

	// Also handle comma-separated include without parens: include "a", "b"
	let simple_re = Regex::new(r#"(?m)^include\s+"([^"]+)""#)
		.unwrap_or_else(|_| unreachable!("simple include regex should be valid"));
	for caps in simple_re.captures_iter(&contents) {
		if let Some(m) = caps.get(1) {
			let name = m.as_str().trim_start_matches(':');
			if !name.is_empty() && !subprojects.contains(&name.to_string()) {
				subprojects.push(name.to_string());
			}
		}
	}

	subprojects
}

fn parse_gradle_project(
	build_file: &Path,
	root: &Path,
	project_name: &str,
) -> MonochangeResult<Option<PackageRecord>> {
	let contents = fs::read_to_string(build_file).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", build_file.display()))
	})?;

	let version = parse_gradle_version(&contents);

	let mut record = PackageRecord::new(
		Ecosystem::Jvm,
		project_name,
		normalize_path(build_file),
		normalize_path(root),
		version,
		PublishState::Public,
	);

	record.declared_dependencies = parse_gradle_dependencies(&contents);

	// Store build tool metadata
	record
		.metadata
		.insert("build_tool".to_string(), "gradle".to_string());

	Ok(Some(record))
}

/// Parse the version from a Gradle build file.
///
/// Looks for `version = "1.2.3"` or `version "1.2.3"`.
fn parse_gradle_version(contents: &str) -> Option<Version> {
	let re = Regex::new(r#"(?m)^\s*version\s*=?\s*"([^"]+)""#).ok()?;
	re.captures(contents)
		.and_then(|caps| caps.get(1))
		.and_then(|m| Version::parse(m.as_str()).ok())
}

/// Parse dependencies from a Gradle build file.
///
/// Matches patterns like:
/// - `implementation("group:artifact:version")`
/// - `implementation "group:artifact:version"`
/// - `testImplementation("group:artifact:version")`
/// - `api("group:artifact:version")`
fn parse_gradle_dependencies(contents: &str) -> Vec<PackageDependency> {
	let re = Regex::new(
		r#"(?m)(implementation|api|compileOnly|runtimeOnly|testImplementation|testRuntimeOnly|testCompileOnly)\s*[\("]"?([^":\)]+):([^":\)]+)(?::([^":\)]+))?"?\s*["\)]"#,
	);

	let Ok(re) = re else {
		return Vec::new();
	};

	let mut deps = Vec::new();
	for caps in re.captures_iter(contents) {
		let config = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
		let artifact = caps.get(3).map(|m| m.as_str().to_string());
		let version = caps.get(4).map(|m| m.as_str().to_string());

		let kind = if config.starts_with("test") {
			DependencyKind::Development
		} else {
			DependencyKind::Runtime
		};

		if let Some(name) = artifact {
			deps.push(PackageDependency {
				name,
				kind,
				version_constraint: version,
				optional: config == "compileOnly",
			});
		}
	}

	deps
}

fn parse_maven_project(pom_path: &Path, root: &Path) -> MonochangeResult<Option<PackageRecord>> {
	let contents = fs::read_to_string(pom_path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", pom_path.display()))
	})?;

	let name = parse_maven_artifact_id(&contents);
	let Some(name) = name else {
		return Ok(None);
	};

	let version = parse_maven_version(&contents);

	let mut record = PackageRecord::new(
		Ecosystem::Jvm,
		&name,
		normalize_path(pom_path),
		normalize_path(root),
		version,
		PublishState::Public,
	);

	record.declared_dependencies = parse_maven_dependencies(&contents);

	record
		.metadata
		.insert("build_tool".to_string(), "maven".to_string());

	Ok(Some(record))
}

/// Parse the `<artifactId>` from a Maven pom.xml.
fn parse_maven_artifact_id(contents: &str) -> Option<String> {
	// Match the first <artifactId> that is a direct child of <project>
	// (not inside <dependency>, <parent>, etc.)
	let re = Regex::new(r"(?s)<project[^>]*>.*?<artifactId>([^<]+)</artifactId>").ok()?;
	re.captures(contents)
		.and_then(|caps| caps.get(1))
		.map(|m| m.as_str().trim().to_string())
}

/// Parse the `<version>` from a Maven pom.xml.
fn parse_maven_version(contents: &str) -> Option<Version> {
	let re = Regex::new(r"(?s)<project[^>]*>.*?<version>([^<]+)</version>").ok()?;
	re.captures(contents)
		.and_then(|caps| caps.get(1))
		.and_then(|m| {
			let version = m.as_str().trim();
			// Skip Maven property references like ${revision}
			if version.starts_with("${") {
				return None;
			}
			Version::parse(version).ok()
		})
}

/// Parse dependencies from a Maven pom.xml.
fn parse_maven_dependencies(contents: &str) -> Vec<PackageDependency> {
	let dep_re = Regex::new(
		r"(?s)<dependency>\s*<groupId>[^<]+</groupId>\s*<artifactId>([^<]+)</artifactId>(?:\s*<version>([^<]+)</version>)?(?:\s*<scope>([^<]+)</scope>)?",
	);

	let Ok(re) = dep_re else {
		return Vec::new();
	};

	let mut deps = Vec::new();
	for caps in re.captures_iter(contents) {
		let name = caps.get(1).map(|m| m.as_str().trim().to_string());
		let version = caps
			.get(2)
			.map(|m| m.as_str().trim().to_string())
			.filter(|v| !v.starts_with("${"));
		let scope = caps.get(3).map(|m| m.as_str().trim().to_string());

		let kind = match scope.as_deref() {
			Some("test") => DependencyKind::Development,
			Some("provided" | "system") => DependencyKind::Build,
			_ => DependencyKind::Runtime,
		};

		if let Some(name) = name {
			deps.push(PackageDependency {
				name,
				kind,
				version_constraint: version,
				optional: false,
			});
		}
	}

	deps
}

fn find_all_pom_files(root: &Path) -> Vec<PathBuf> {
	WalkDir::new(root)
		.into_iter()
		.filter_entry(should_descend)
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == MAVEN_POM)
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
		.collect()
}

fn should_descend(entry: &DirEntry) -> bool {
	let file_name = entry.file_name().to_string_lossy();
	!matches!(
		file_name.as_ref(),
		".git" | "node_modules" | "target" | ".devenv" | "book" | ".gradle" | "build" | ".mvn"
	)
}

#[cfg(test)]
mod __tests;
