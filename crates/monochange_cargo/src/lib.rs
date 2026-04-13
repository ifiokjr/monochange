#![forbid(clippy::indexing_slicing)]

//! # `monochange_cargo`
//!
//! <!-- {=monochangeCargoCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_cargo` discovers Cargo packages and surfaces Rust-specific release evidence.
//!
//! Reach for this crate when you want to scan Cargo workspaces into normalized `monochange_core` records and optionally feed Rust semver evidence into release planning.
//!
//! ## Why use it?
//!
//! - discover Cargo workspaces and standalone crates with one adapter
//! - normalize crate manifests and dependency edges for the shared planner
//! - attach Rust semver evidence through `RustSemverProvider`
//!
//! ## Best for
//!
//! - building Cargo-aware discovery flows without the full CLI
//! - feeding Rust semver evidence into release planning
//! - converting Cargo workspace structure into shared `monochange_core` records
//!
//! ## Public entry points
//!
//! - `discover_cargo_packages(root)` discovers Cargo workspaces and standalone crates
//! - `CargoAdapter` exposes the shared adapter interface
//! - `RustSemverProvider` parses explicit Rust semver evidence from change input
//!
//! ## Scope
//!
//! - Cargo workspace glob expansion
//! - crate manifest parsing
//! - normalized dependency extraction
//! - Rust semver provider integration for release planning
//! <!-- {/monochangeCargoCrateDocs} -->

use std::collections::BTreeMap;
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
use monochange_semver::CompatibilityProvider;
use semver::Version;
use toml::Value;
use toml_edit::DocumentMut;
use toml_edit::Item;
use toml_edit::TableLike;
use toml_edit::Value as EditValue;
use toml_edit::value;
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CargoVersionedFileKind {
	Manifest,
	Lock,
}

pub fn supported_versioned_file_kind(path: &Path) -> Option<CargoVersionedFileKind> {
	let file_name = path
		.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or_default();
	if file_name == "Cargo.lock" {
		Some(CargoVersionedFileKind::Lock)
	} else if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
		Some(CargoVersionedFileKind::Manifest)
	} else {
		None
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
	let mut discovered = [scope.join("Cargo.lock")]
		.into_iter()
		.filter(|path| path.exists())
		.collect::<Vec<_>>();
	if discovered.is_empty() && scope != manifest_dir {
		discovered.extend(
			[manifest_dir.join("Cargo.lock")]
				.into_iter()
				.filter(|path| path.exists()),
		);
	}
	discovered
}

pub fn default_lockfile_commands(package: &PackageRecord) -> Vec<LockfileCommandExecution> {
	discover_lockfiles(package)
		.into_iter()
		.map(|lockfile| {
			LockfileCommandExecution {
				command: "cargo generate-lockfile".to_string(),
				cwd: lockfile
					.parent()
					.unwrap_or(&package.workspace_root)
					.to_path_buf(),
				shell: ShellConfig::None,
			}
		})
		.collect()
}

pub fn lockfile_requires_command_refresh(lockfile: &Path, packages: &[&PackageRecord]) -> bool {
	let Ok(contents) = fs::read_to_string(lockfile) else {
		return true;
	};
	let Ok(document) = toml::from_str::<Value>(&contents) else {
		return true;
	};
	let locked_package_names = document
		.get("package")
		.and_then(Value::as_array)
		.into_iter()
		.flatten()
		.filter_map(|package| package.get("name").and_then(Value::as_str))
		.collect::<HashSet<_>>();
	if locked_package_names.is_empty() {
		return true;
	}
	let workspace_package_names = packages
		.iter()
		.map(|package| package.name.as_str())
		.collect::<HashSet<_>>();
	packages.iter().any(|package| {
		package
			.declared_dependencies
			.iter()
			.filter(|dependency| !dependency.optional)
			.any(|dependency| {
				!workspace_package_names.contains(dependency.name.as_str())
					&& !locked_package_names.contains(dependency.name.as_str())
			})
	})
}

#[must_use = "the validation result must be checked"]
pub fn validate_workspace_version_groups(packages: &[PackageRecord]) -> MonochangeResult<()> {
	let mut workspace_versioned = BTreeMap::<PathBuf, Vec<&PackageRecord>>::new();
	for package in packages {
		if package.ecosystem == Ecosystem::Cargo
			&& package.metadata.contains_key("config_id")
			&& package
				.metadata
				.get("uses_workspace_version")
				.map(String::as_str)
				== Some("true")
		{
			workspace_versioned
				.entry(package.workspace_root.clone())
				.or_default()
				.push(package);
		}
	}

	for packages in workspace_versioned.values() {
		if packages.len() < 2 {
			continue;
		}
		let group_ids = packages
			.iter()
			.map(|package| package.version_group_id.as_deref())
			.collect::<BTreeSet<_>>();
		if group_ids.len() > 1 || group_ids.contains(&None) {
			let details = packages
				.iter()
				.map(|package| {
					match &package.version_group_id {
						Some(group_id) => format!("`{}` in group `{}`", package.name, group_id),
						None => format!("`{}` not in any group", package.name),
					}
				})
				.collect::<Vec<_>>();
			return Err(MonochangeError::Config(format!(
				"cargo packages using `version.workspace = true` must belong to the same version group, but found mismatched assignments: {}",
				details.join(", ")
			)));
		}
	}

	Ok(())
}

pub fn update_versioned_file(
	document: &mut DocumentMut,
	kind: CargoVersionedFileKind,
	fields: &[&str],
	owner_version: Option<&str>,
	shared_release_version: Option<&str>,
	versioned_deps: &BTreeMap<String, String>,
	raw_versions: &BTreeMap<String, String>,
) {
	match kind {
		CargoVersionedFileKind::Lock => {
			if let Some(packages) = document
				.get_mut("package")
				.and_then(Item::as_array_of_tables_mut)
			{
				for package in packages.iter_mut() {
					let Some(package_name) = package.get("name").and_then(Item::as_str) else {
						continue;
					};
					if let Some(version) = raw_versions.get(package_name) {
						set_table_value(package, "version", version);
					}
				}
			}
		}
		CargoVersionedFileKind::Manifest => {
			update_manifest_owner_version(document, owner_version);
			for field in fields {
				update_manifest_field(
					document,
					field,
					owner_version,
					shared_release_version,
					versioned_deps,
				);
			}
			update_manifest_workspace_version(document, shared_release_version);
			if !fields_target_workspace_dependencies(fields) {
				update_workspace_dependencies(document, versioned_deps);
			}
		}
	}
}

pub fn update_versioned_file_text(
	contents: &str,
	kind: CargoVersionedFileKind,
	fields: &[&str],
	owner_version: Option<&str>,
	shared_release_version: Option<&str>,
	versioned_deps: &BTreeMap<String, String>,
	raw_versions: &BTreeMap<String, String>,
) -> Result<String, toml_edit::TomlError> {
	if kind == CargoVersionedFileKind::Lock {
		return Ok(update_lockfile_text(contents, raw_versions));
	}
	let mut document = contents.parse::<DocumentMut>()?;
	update_versioned_file(
		&mut document,
		kind,
		fields,
		owner_version,
		shared_release_version,
		versioned_deps,
		raw_versions,
	);
	Ok(document.to_string())
}

fn update_lockfile_text(contents: &str, raw_versions: &BTreeMap<String, String>) -> String {
	let mut lines = contents
		.split_inclusive('\n')
		.map(ToString::to_string)
		.collect::<Vec<_>>();
	let mut line_index = 0;
	while let Some(current_line) = lines.get(line_index) {
		if current_line.trim() != "[[package]]" {
			line_index += 1;
			continue;
		}
		let section_start = line_index + 1;
		let section_end = lines
			.iter()
			.skip(section_start)
			.position(|line| line.trim() == "[[package]]")
			.map_or(lines.len(), |offset| section_start + offset);
		let package_name = lines
			.iter()
			.skip(section_start)
			.take(section_end.saturating_sub(section_start))
			.find_map(|line| {
				parse_toml_basic_string_assignment(line, "name").map(|(_, _, value)| value)
			});
		let Some(package_name) = package_name else {
			line_index = section_end;
			continue;
		};
		let Some(version) = raw_versions.get(&package_name) else {
			line_index = section_end;
			continue;
		};
		for section_line in lines
			.iter_mut()
			.skip(section_start)
			.take(section_end.saturating_sub(section_start))
		{
			let Some((value_start, value_end, _)) =
				parse_toml_basic_string_assignment(section_line, "version")
			else {
				continue;
			};
			section_line.replace_range(value_start..value_end, version);
			break;
		}
		line_index = section_end;
	}
	lines.concat()
}

fn parse_toml_basic_string_assignment(line: &str, key: &str) -> Option<(usize, usize, String)> {
	let trimmed_line = line.trim_end_matches(['\n', '\r']);
	let content_start = trimmed_line
		.char_indices()
		.find_map(|(index, ch)| (!ch.is_whitespace()).then_some(index))
		.unwrap_or(trimmed_line.len());
	let key_start = content_start;
	let key_end = key_start + key.len();
	let remainder = trimmed_line.get(key_start..)?;
	let remainder = remainder.strip_prefix(key)?;
	let equals_start = remainder
		.char_indices()
		.find_map(|(index, ch)| (!ch.is_whitespace()).then_some(index))?;
	if remainder.get(equals_start..=equals_start)? != "=" {
		return None;
	}
	let value_start_in_remainder = remainder[equals_start + 1..]
		.char_indices()
		.find_map(|(index, ch)| (!ch.is_whitespace()).then_some(equals_start + 1 + index))?;
	if remainder.get(value_start_in_remainder..=value_start_in_remainder)? != "\"" {
		return None;
	}
	let value_start = key_end + value_start_in_remainder + 1;
	let value_end = trimmed_line
		.get(value_start..)?
		.find('"')
		.map(|offset| value_start + offset)?;
	Some((
		value_start,
		value_end,
		trimmed_line[value_start..value_end].to_string(),
	))
}

fn update_manifest_owner_version(document: &mut DocumentMut, owner_version: Option<&str>) {
	let Some(owner_version) = owner_version else {
		return;
	};
	let Some(package_table) = document
		.get_mut("package")
		.and_then(Item::as_table_like_mut)
	else {
		return;
	};
	let uses_workspace_version = package_table
		.get("version")
		.is_some_and(uses_workspace_marker);
	if !uses_workspace_version {
		set_table_value(package_table, "version", owner_version);
	}
}

fn update_manifest_workspace_version(
	document: &mut DocumentMut,
	shared_release_version: Option<&str>,
) {
	let Some(shared_release_version) = shared_release_version else {
		return;
	};
	let Some(workspace_package) = document
		.get_mut("workspace")
		.and_then(Item::as_table_like_mut)
		.and_then(|workspace| workspace.get_mut("package"))
		.and_then(Item::as_table_like_mut)
	else {
		return;
	};
	set_table_value(workspace_package, "version", shared_release_version);
}

fn update_workspace_dependencies(
	document: &mut DocumentMut,
	versioned_deps: &BTreeMap<String, String>,
) {
	let Some(workspace_deps) = document
		.get_mut("workspace")
		.and_then(Item::as_table_like_mut)
		.and_then(|workspace| workspace.get_mut("dependencies"))
		.and_then(Item::as_table_like_mut)
	else {
		return;
	};
	for (dep_name, dep_version) in versioned_deps {
		update_dependency_by_name(workspace_deps, dep_name, dep_version);
	}
}

fn update_manifest_field(
	document: &mut DocumentMut,
	field: &str,
	owner_version: Option<&str>,
	shared_release_version: Option<&str>,
	versioned_deps: &BTreeMap<String, String>,
) {
	let segments = normalized_manifest_field_segments(field);
	match segments.as_slice() {
		["package", "version"] => update_manifest_owner_version(document, owner_version),
		["workspace", "package", "version"] => {
			update_manifest_workspace_version(document, shared_release_version.or(owner_version));
		}
		[table] if is_dependency_table(table) => {
			let Some(table) = document.get_mut(table).and_then(Item::as_table_like_mut) else {
				return;
			};
			for (dep_name, dep_version) in versioned_deps {
				update_dependency_by_name(table, dep_name, dep_version);
			}
		}
		["workspace", "dependencies"] => update_workspace_dependencies(document, versioned_deps),
		[table, dep_name] if is_dependency_table(table) => {
			let Some(dep_version) = versioned_deps.get(*dep_name) else {
				return;
			};
			let Some(table) = document.get_mut(table).and_then(Item::as_table_like_mut) else {
				return;
			};
			update_dependency_by_name(table, dep_name, dep_version);
		}
		[table, dep_name, "version"] if is_dependency_table(table) => {
			let Some(dep_version) = versioned_deps.get(*dep_name) else {
				return;
			};
			let Some(table) = document.get_mut(table).and_then(Item::as_table_like_mut) else {
				return;
			};
			update_dependency_version_by_name(table, dep_name, dep_version);
		}
		["workspace", "dependencies", dep_name] => {
			let Some(dep_version) = versioned_deps.get(*dep_name) else {
				return;
			};
			let Some(workspace_deps) = document
				.get_mut("workspace")
				.and_then(Item::as_table_like_mut)
				.and_then(|workspace| workspace.get_mut("dependencies"))
				.and_then(Item::as_table_like_mut)
			else {
				return;
			};
			update_dependency_by_name(workspace_deps, dep_name, dep_version);
		}
		["workspace", "dependencies", dep_name, "version"] => {
			let Some(dep_version) = versioned_deps.get(*dep_name) else {
				return;
			};
			let Some(workspace_deps) = document
				.get_mut("workspace")
				.and_then(Item::as_table_like_mut)
				.and_then(|workspace| workspace.get_mut("dependencies"))
				.and_then(Item::as_table_like_mut)
			else {
				return;
			};
			update_dependency_version_by_name(workspace_deps, dep_name, dep_version);
		}
		_ => {
			if let Some(version) = shared_release_version.or(owner_version) {
				set_table_value_by_path(document.as_table_mut(), &segments, version);
			}
		}
	}
}

fn fields_target_workspace_dependencies(fields: &[&str]) -> bool {
	fields.iter().any(|field| {
		matches!(
			normalized_manifest_field_segments(field).as_slice(),
			["workspace", "dependencies"] | ["workspace", "dependencies", ..]
		)
	})
}

fn normalized_manifest_field_segments(field: &str) -> Vec<&str> {
	let mut segments = field
		.split('.')
		.filter(|segment| !segment.is_empty())
		.map(normalize_manifest_field_segment)
		.collect::<Vec<_>>();
	if matches!(segments.as_slice(), ["workspace", "version"]) {
		segments = vec!["workspace", "package", "version"];
	}
	segments
}

fn normalize_manifest_field_segment(segment: &str) -> &str {
	match segment {
		"dev_dependencies" => "dev-dependencies",
		"build_dependencies" => "build-dependencies",
		_ => segment,
	}
}

fn is_dependency_table(segment: &str) -> bool {
	matches!(
		segment,
		"dependencies" | "dev-dependencies" | "build-dependencies"
	)
}

fn update_dependency_by_name(table: &mut dyn TableLike, dep_name: &str, version: &str) {
	let Some(entry) = table.get_mut(dep_name) else {
		return;
	};
	update_dependency_entry(entry, version);
}

fn update_dependency_version_by_name(table: &mut dyn TableLike, dep_name: &str, version: &str) {
	let Some(entry) = table.get_mut(dep_name) else {
		return;
	};
	update_dependency_version_field(entry, version);
}

fn update_dependency_entry(entry: &mut Item, version: &str) {
	if entry.is_str() {
		set_item_string(entry, version);
		return;
	}
	let Some(entry_table) = entry.as_table_like_mut() else {
		return;
	};
	let uses_workspace = entry_table
		.get("workspace")
		.is_some_and(uses_workspace_marker);
	if !uses_workspace {
		set_table_value(entry_table, "version", version);
	}
}

fn update_dependency_version_field(entry: &mut Item, version: &str) {
	let Some(entry_table) = entry.as_table_like_mut() else {
		return;
	};
	let uses_workspace = entry_table
		.get("workspace")
		.is_some_and(uses_workspace_marker);
	if !uses_workspace {
		set_table_value(entry_table, "version", version);
	}
}

fn uses_workspace_marker(item: &Item) -> bool {
	item.as_bool() == Some(true)
		|| item
			.as_inline_table()
			.and_then(|table| table.get("workspace"))
			.and_then(EditValue::as_bool)
			== Some(true)
}

fn set_table_value(table: &mut dyn TableLike, key: &str, version: &str) {
	if let Some(item) = table.get_mut(key) {
		set_item_string(item, version);
	} else {
		table.insert(key, value(version));
	}
}

fn set_table_value_by_path(table: &mut dyn TableLike, path: &[&str], version: &str) {
	let Some((head, tail)) = path.split_first() else {
		return;
	};
	if tail.is_empty() {
		set_table_value(table, head, version);
		return;
	}
	let Some(item) = table.get_mut(head) else {
		return;
	};
	let Some(next_table) = item.as_table_like_mut() else {
		return;
	};
	set_table_value_by_path(next_table, tail, version);
}

fn set_item_string(item: &mut Item, version: &str) {
	if let Some(existing_value) = item.as_value() {
		let mut new_value = EditValue::from(version);
		*new_value.decor_mut() = existing_value.decor().clone();
		*item = Item::Value(new_value);
	} else {
		*item = value(version);
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

#[tracing::instrument(skip_all)]
#[must_use = "the discovery result must be checked"]
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
	tracing::debug!(packages = packages.len(), "discovered cargo packages");

	Ok(AdapterDiscovery { packages, warnings })
}

/// Load one explicitly configured Cargo package without walking the whole repo.
///
/// Performance note:
/// full-repository `WalkDir` scans are fine for `mc discover`, but they become a
/// large fixed cost for release planning in repositories that vendor many test
/// fixtures. Release planning already knows the configured package paths, so this
/// helper lets higher-level code parse just the manifests it needs instead of
/// rediscovering every Cargo fixture on disk.
#[must_use = "the package result must be checked"]
pub fn load_configured_cargo_package(
	root: &Path,
	package_path: &Path,
) -> MonochangeResult<Option<PackageRecord>> {
	let manifest_path =
		if package_path.file_name().and_then(|name| name.to_str()) == Some(CARGO_MANIFEST_FILE) {
			package_path.to_path_buf()
		} else {
			package_path.join(CARGO_MANIFEST_FILE)
		};
	let workspace_manifest = find_nearest_workspace_manifest(root, &manifest_path);
	let workspace_root = workspace_manifest
		.as_ref()
		.and_then(|path| path.parent())
		.unwrap_or_else(|| manifest_path.parent().unwrap_or(root));
	let workspace_version = match workspace_manifest.as_ref() {
		Some(path) => workspace_package_version_from_manifest(path)?,
		None => None,
	};
	parse_package_manifest(&manifest_path, workspace_root, workspace_version.as_ref())
}

fn find_nearest_workspace_manifest(root: &Path, manifest_path: &Path) -> Option<PathBuf> {
	let mut current = manifest_path.parent();
	while let Some(directory) = current {
		let candidate = directory.join(CARGO_MANIFEST_FILE);
		if candidate.exists() && has_workspace_section(&candidate).unwrap_or(false) {
			return Some(candidate);
		}
		if directory == root {
			break;
		}
		current = directory.parent();
	}
	None
}

fn workspace_package_version_from_manifest(
	workspace_manifest: &Path,
) -> MonochangeResult<Option<Version>> {
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
	Ok(workspace_package_version(&parsed))
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
	let filter = DiscoveryPathFilter::new(root);
	let excluded = exclude_patterns
		.iter()
		.flat_map(|pattern| glob_pattern_paths(root, pattern, &filter))
		.collect::<HashSet<_>>();
	let mut manifests = BTreeSet::new();

	for pattern in member_patterns {
		let matches = glob_pattern_paths(root, pattern, &filter);
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
				|| !filter.allows(&manifest_path)
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

fn glob_pattern_paths(root: &Path, pattern: &str, filter: &DiscoveryPathFilter) -> Vec<PathBuf> {
	let joined_pattern = root.join(pattern).to_string_lossy().to_string();
	glob(&joined_pattern)
		.into_iter()
		.flat_map(|paths| paths.filter_map(Result::ok))
		.map(|path| normalize_path(&path))
		.filter(|path| filter.allows(path))
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
	let uses_workspace_version = package
		.get("version")
		.and_then(Value::as_table)
		.and_then(|table| table.get("workspace"))
		.and_then(Value::as_bool)
		.unwrap_or(false);
	if uses_workspace_version {
		package_record
			.metadata
			.insert("uses_workspace_version".to_string(), "true".to_string());
	}
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
				.map(|(name, value)| {
					PackageDependency {
						name: name.clone(),
						kind,
						version_constraint: dependency_constraint(value),
						optional: value
							.as_table()
							.and_then(|table| table.get("optional"))
							.and_then(Value::as_bool)
							.unwrap_or(false),
					}
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
	let filter = DiscoveryPathFilter::new(root);
	WalkDir::new(root)
		.into_iter()
		.filter_entry(|entry| filter.should_descend(entry.path()))
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name() == CARGO_MANIFEST_FILE)
		.map(DirEntry::into_path)
		.map(|path| normalize_path(&path))
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

#[cfg(test)]
mod __tests;
