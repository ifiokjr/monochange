#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

doc_comment::doctest!("../readme.md");

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::EcosystemSettings;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::VersionGroup;
use monochange_core::VersionGroupDefinition;
use monochange_core::WorkspaceConfiguration;
use monochange_core::WorkspaceDefaults;
use serde::Deserialize;

const CONFIG_FILE: &str = "monochange.toml";

#[derive(Debug, Deserialize, Default)]
struct RawWorkspaceConfiguration {
	#[serde(default)]
	defaults: RawWorkspaceDefaults,
	#[serde(default)]
	version_groups: Vec<VersionGroupDefinition>,
	#[serde(default)]
	ecosystems: RawEcosystems,
}

#[derive(Debug, Deserialize)]
struct RawWorkspaceDefaults {
	#[serde(default = "default_parent_bump")]
	parent_bump: BumpSeverity,
	#[serde(default)]
	include_private: bool,
	#[serde(default = "default_warn_on_group_mismatch")]
	warn_on_group_mismatch: bool,
}

impl Default for RawWorkspaceDefaults {
	fn default() -> Self {
		Self {
			parent_bump: default_parent_bump(),
			include_private: false,
			warn_on_group_mismatch: default_warn_on_group_mismatch(),
		}
	}
}

#[derive(Debug, Deserialize, Default)]
struct RawEcosystems {
	#[serde(default)]
	cargo: EcosystemSettings,
	#[serde(default)]
	npm: EcosystemSettings,
	#[serde(default)]
	deno: EcosystemSettings,
	#[serde(default)]
	dart: EcosystemSettings,
}

#[derive(Debug, Deserialize, Default)]
struct RawChangeFile {
	#[serde(default)]
	changes: Vec<RawChangeEntry>,
}

#[derive(Debug, Deserialize)]
struct RawChangeEntry {
	package: String,
	#[serde(default)]
	bump: Option<BumpSeverity>,
	#[serde(default)]
	reason: Option<String>,
	#[serde(default = "default_change_origin")]
	origin: String,
	#[serde(default)]
	evidence: Vec<String>,
}

fn default_parent_bump() -> BumpSeverity {
	BumpSeverity::Patch
}

fn default_warn_on_group_mismatch() -> bool {
	true
}

fn default_change_origin() -> String {
	"direct-change".to_string()
}

#[must_use]
pub fn config_path(root: &Path) -> PathBuf {
	root.join(CONFIG_FILE)
}

pub fn load_workspace_configuration(root: &Path) -> MonochangeResult<WorkspaceConfiguration> {
	let path = config_path(root);
	let raw = if path.exists() {
		let contents = fs::read_to_string(&path).map_err(|error| {
			MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
		})?;
		toml::from_str::<RawWorkspaceConfiguration>(&contents).map_err(|error| {
			MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
		})?
	} else {
		RawWorkspaceConfiguration::default()
	};

	validate_version_groups(&raw.version_groups)?;

	Ok(WorkspaceConfiguration {
		root_path: root.to_path_buf(),
		defaults: WorkspaceDefaults {
			parent_bump: raw.defaults.parent_bump,
			include_private: raw.defaults.include_private,
			warn_on_group_mismatch: raw.defaults.warn_on_group_mismatch,
		},
		version_groups: raw.version_groups,
		cargo: raw.ecosystems.cargo,
		npm: raw.ecosystems.npm,
		deno: raw.ecosystems.deno,
		dart: raw.ecosystems.dart,
	})
}

pub fn load_change_signals(
	changes_path: &Path,
	workspace_root: &Path,
	packages: &[PackageRecord],
) -> MonochangeResult<Vec<ChangeSignal>> {
	let contents = fs::read_to_string(changes_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read {}: {error}",
			changes_path.display()
		))
	})?;
	let raw = toml::from_str::<RawChangeFile>(&contents).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse {}: {error}",
			changes_path.display()
		))
	})?;

	let mut seen_package_ids = BTreeSet::new();
	let mut signals = Vec::new();
	for change in raw.changes {
		let package_id = resolve_package_reference(&change.package, workspace_root, packages)?;
		if !seen_package_ids.insert(package_id.clone()) {
			return Err(MonochangeError::Config(format!(
				"duplicate change entry for `{package_id}` in {}",
				changes_path.display()
			)));
		}

		signals.push(ChangeSignal {
			package_id,
			requested_bump: change.bump,
			change_origin: change.origin,
			evidence_refs: change.evidence,
			notes: change.reason,
		});
	}

	Ok(signals)
}

pub fn resolve_package_reference(
	reference: &str,
	workspace_root: &Path,
	packages: &[PackageRecord],
) -> MonochangeResult<String> {
	let matching_package_ids = find_matching_package_ids(reference, workspace_root, packages);
	match matching_package_ids.as_slice() {
		[] => Err(MonochangeError::Config(format!(
			"change package reference `{reference}` did not match any discovered package"
		))),
		[package_id] => Ok(package_id.clone()),
		_ => Err(MonochangeError::Config(format!(
			"change package reference `{reference}` matched multiple packages: {}",
			matching_package_ids.join(", ")
		))),
	}
}

fn validate_version_groups(version_groups: &[VersionGroupDefinition]) -> MonochangeResult<()> {
	let mut seen_names = BTreeSet::new();

	for version_group in version_groups {
		if !seen_names.insert(version_group.name.clone()) {
			return Err(MonochangeError::Config(format!(
				"duplicate version group `{}`",
				version_group.name
			)));
		}
	}

	Ok(())
}

pub fn apply_version_groups(
	packages: &mut [PackageRecord],
	configuration: &WorkspaceConfiguration,
) -> MonochangeResult<(Vec<VersionGroup>, Vec<String>)> {
	let mut warnings = Vec::new();
	let mut assigned = BTreeMap::<String, String>::new();
	let mut groups = Vec::new();

	for version_group in &configuration.version_groups {
		let mut members = Vec::new();
		let mut versions = BTreeSet::new();

		for member in &version_group.members {
			let matching_indices =
				find_matching_package_indices(packages, &configuration.root_path, member);

			if matching_indices.is_empty() {
				warnings.push(format!(
					"version group `{}` member `{member}` did not match any discovered package",
					version_group.name
				));
				continue;
			}

			for package_index in matching_indices {
				let package = packages.get_mut(package_index).ok_or_else(|| {
					MonochangeError::Config(format!(
						"matched package index `{package_index}` for version group `{}` is invalid",
						version_group.name
					))
				})?;

				if let Some(existing_group) = assigned.get(&package.id) {
					return Err(MonochangeError::Config(format!(
						"package `{}` belongs to conflicting version groups `{existing_group}` \
						 and `{}`",
						package.id, version_group.name
					)));
				}

				assigned.insert(package.id.clone(), version_group.name.clone());
				package.version_group_id = Some(version_group.name.clone());
				members.push(package.id.clone());

				if let Some(version) = &package.current_version {
					versions.insert(version.to_string());
				}
			}
		}

		let mismatch_detected = versions.len() > 1;
		if mismatch_detected && configuration.defaults.warn_on_group_mismatch {
			warnings.push(format!(
				"version group `{}` contains packages with mismatched versions",
				version_group.name
			));
		}

		groups.push(VersionGroup {
			group_id: version_group.name.clone(),
			display_name: version_group.name.clone(),
			members,
			mismatch_detected,
		});
	}

	Ok((groups, warnings))
}

fn find_matching_package_indices(
	packages: &[PackageRecord],
	root: &Path,
	member: &str,
) -> Vec<usize> {
	packages
		.iter()
		.enumerate()
		.filter_map(|(index, package)| {
			if package_matches_reference(package, root, member) {
				Some(index)
			} else {
				None
			}
		})
		.collect()
}

fn find_matching_package_ids(
	reference: &str,
	root: &Path,
	packages: &[PackageRecord],
) -> Vec<String> {
	packages
		.iter()
		.filter(|package| package_matches_reference(package, root, reference))
		.map(|package| package.id.clone())
		.collect()
}

fn package_matches_reference(package: &PackageRecord, root: &Path, reference: &str) -> bool {
	let manifest_match = package
		.manifest_path
		.strip_prefix(root)
		.ok()
		.and_then(|path| path.to_str())
		.is_some_and(|path| path == reference);
	let directory_match = package
		.manifest_path
		.parent()
		.and_then(|path| path.strip_prefix(root).ok())
		.and_then(|path| path.to_str())
		.is_some_and(|path| path == reference);
	let name_match = package.name == reference;
	let id_match = package.id == reference;

	manifest_match || directory_match || name_match || id_match
}

#[cfg(test)]
mod __tests;
