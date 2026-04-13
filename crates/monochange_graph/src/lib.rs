#![forbid(clippy::indexing_slicing)]

//! # `monochange_graph`
//!
//! <!-- {=monochangeGraphCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_graph` turns normalized workspace data into release decisions.
//!
//! Reach for this crate when you already have discovered packages, dependency edges, configuration, and change signals and need to calculate propagated bumps, synchronized version groups, and final release-plan output.
//!
//! ## Why use it?
//!
//! - calculate release impact across direct and transitive dependents
//! - keep version groups synchronized during planning
//! - produce one deterministic release plan from normalized input data
//!
//! ## Best for
//!
//! - embedding release-planning logic in custom automation or other tools
//! - computing the exact set of packages that need to move after a change
//! - separating planning logic from ecosystem-specific discovery code
//!
//! ## Public entry points
//!
//! - `NormalizedGraph` builds adjacency and reverse-dependency views over package data
//! - `build_release_plan(workspace_root, packages, dependency_edges, defaults, version_groups, change_signals, providers)` computes the release plan
//!
//! ## Responsibilities
//!
//! - build reverse dependency views
//! - propagate release impact across direct and transitive dependents
//! - synchronize version groups
//! - calculate planned group versions
//! <!-- {/monochangeGraphCrateDocs} -->

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::CompatibilityAssessment;
use monochange_core::DependencyEdge;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::PlannedVersionGroup;
use monochange_core::ReleaseDecision;
use monochange_core::ReleasePlan;
use monochange_core::VersionGroup;
use monochange_semver::direct_release_severity;
use monochange_semver::propagated_release_severity;
use monochange_semver::strongest_assessment_for_package;
use semver::Version;

/// Reverse-dependency graph over discovered packages.
///
/// `NormalizedGraph` borrows string slices from the input `PackageRecord` and
/// `DependencyEdge` slices. It must be consumed (queried and dropped) before
/// the input data goes out of scope. If you need to store the graph across
/// async boundaries or function returns, clone the relevant data first.
#[derive(Debug, Clone)]
pub struct NormalizedGraph<'a> {
	package_ids: BTreeSet<&'a str>,
	reverse_edges: BTreeMap<&'a str, BTreeSet<&'a str>>,
}

#[derive(Debug, Clone)]
struct DecisionState {
	severity: BumpSeverity,
	trigger_type: String,
	reasons: BTreeSet<String>,
	upstream_sources: BTreeSet<String>,
	warnings: Vec<String>,
}

impl Default for DecisionState {
	fn default() -> Self {
		Self {
			severity: BumpSeverity::None,
			trigger_type: "none".to_string(),
			reasons: BTreeSet::new(),
			upstream_sources: BTreeSet::new(),
			warnings: Vec::new(),
		}
	}
}

impl<'a> NormalizedGraph<'a> {
	#[must_use]
	pub fn new(packages: &'a [PackageRecord], dependency_edges: &'a [DependencyEdge]) -> Self {
		let mut reverse_edges = BTreeMap::<&'a str, BTreeSet<&'a str>>::new();
		let package_ids = packages.iter().map(|package| package.id.as_str()).collect();

		for edge in dependency_edges {
			reverse_edges
				.entry(&edge.to_package_id)
				.or_default()
				.insert(&edge.from_package_id);
		}

		Self {
			package_ids,
			reverse_edges,
		}
	}

	#[must_use]
	pub fn direct_dependents(&self, package_id: &str) -> Vec<&'a str> {
		self.reverse_edges
			.get(package_id)
			.map(|set| set.iter().copied().collect())
			.unwrap_or_default()
	}

	#[must_use]
	pub fn transitive_dependents(&self, package_id: &str) -> BTreeSet<&'a str> {
		let mut discovered = BTreeSet::new();
		let mut queue: VecDeque<&str> = VecDeque::from([package_id]);

		while let Some(current) = queue.pop_front() {
			for dependent in self.direct_dependents(current) {
				if discovered.insert(dependent) {
					queue.push_back(dependent);
				}
			}
		}

		discovered
	}

	#[must_use]
	pub fn contains(&self, package_id: &str) -> bool {
		self.package_ids.contains(package_id)
	}
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
#[must_use = "the release plan result must be checked"]
pub fn build_release_plan(
	workspace_root: &Path,
	packages: &[PackageRecord],
	dependency_edges: &[DependencyEdge],
	version_groups: &[VersionGroup],
	change_signals: &[ChangeSignal],
	compatibility_evidence: &[CompatibilityAssessment],
	default_parent_bump: BumpSeverity,
	strict_version_conflicts: bool,
) -> MonochangeResult<ReleasePlan> {
	let graph = NormalizedGraph::new(packages, dependency_edges);
	let package_by_id = packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<BTreeMap<_, _>>();
	let group_by_id = version_groups
		.iter()
		.map(|group| (group.group_id.as_str(), group))
		.collect::<BTreeMap<_, _>>();

	let (explicit_package_versions, explicit_group_versions, warnings) = resolve_explicit_versions(
		&package_by_id,
		&group_by_id,
		change_signals,
		strict_version_conflicts,
	)?;

	let mut states = packages
		.iter()
		.map(|package| {
			(
				package.id.as_str(),
				DecisionState {
					trigger_type: "none".to_string(),
					..DecisionState::default()
				},
			)
		})
		.collect::<BTreeMap<&str, _>>();
	let mut queue: VecDeque<&str> = VecDeque::new();

	// Process all change signals to establish initial decisions.
	for change_signal in change_signals {
		let assessment =
			strongest_assessment_for_package(compatibility_evidence, &change_signal.package_id);
		let direct_severity =
			direct_release_severity(change_signal.requested_bump, assessment.as_ref());
		let reason = change_signal
			.notes
			.clone()
			.unwrap_or_else(|| "explicit change input".to_string());
		let upstream_sources = BTreeSet::from([change_signal.package_id.clone()]);

		apply_decision(
			&mut states,
			&mut queue,
			&change_signal.package_id,
			direct_severity,
			"direct-change",
			&reason,
			&upstream_sources,
		);
	}

	// Propagate decisions through the dependency graph.
	while let Some(source_package_id) = queue.pop_front() {
		let source_state = if let Some(state) = states.get(source_package_id) {
			state.clone()
		} else {
			continue;
		};

		if !source_state.severity.is_release() {
			continue;
		}

		let source_assessment =
			strongest_assessment_for_package(compatibility_evidence, source_package_id);
		let propagated_severity =
			propagated_release_severity(default_parent_bump, source_assessment.as_ref());

		if propagated_severity.is_release() {
			for dependent_id in graph.direct_dependents(source_package_id) {
				let reason = format!("depends on `{source_package_id}`");
				apply_decision(
					&mut states,
					&mut queue,
					dependent_id,
					propagated_severity,
					"transitive-dependency",
					&reason,
					&source_state.upstream_sources,
				);
			}
		}

		let group_id = package_by_id
			.get(source_package_id)
			.and_then(|package| package.version_group_id.as_deref());
		let Some(group_id) = group_id else {
			continue;
		};
		let Some(group) = group_by_id.get(group_id) else {
			continue;
		};

		let group_max = group
			.members
			.iter()
			.map(|member| {
				states.get(member.as_str()).map_or_else(
					|| {
						eprintln!(
							"warning: version group `{group_id}` member `{member}` was not found in discovered packages"
						);
						BumpSeverity::None
					},
					|state| state.severity,
				)
			})
			.max()
			.unwrap_or(BumpSeverity::None);

		if group_max.is_release() {
			let reason = format!("shares version group `{group_id}`");
			for member_id in &group.members {
				apply_decision(
					&mut states,
					&mut queue,
					member_id,
					group_max,
					"version-group-synchronization",
					&reason,
					&source_state.upstream_sources,
				);
			}
		}
	}

	let planned_groups = version_groups
		.iter()
		.filter_map(|group| planned_group(group, &package_by_id, &states, &explicit_group_versions))
		.collect::<Vec<_>>();
	let planned_group_by_id: BTreeMap<&str, &PlannedVersionGroup> = planned_groups
		.iter()
		.map(|group| (group.group_id.as_str(), group))
		.collect();

	let decisions = packages
		.iter()
		.map(|package| {
			let state = states.get(package.id.as_str()).cloned().unwrap_or_default();
			let planned_version = package.version_group_id.as_deref().and_then(|group_id| {
				planned_group_by_id
					.get(group_id)
					.and_then(|group| group.planned_version.clone())
			});
			let standalone_planned_version =
				if planned_version.is_none() && state.severity.is_release() {
					explicit_package_versions
						.get(&package.id)
						.cloned()
						.or_else(|| {
							package
								.current_version
								.as_ref()
								.map(|version| state.severity.apply_to_version(version))
						})
				} else {
					None
				};

			ReleaseDecision {
				package_id: package.id.clone(),
				trigger_type: state.trigger_type,
				recommended_bump: state.severity,
				planned_version: planned_version.or(standalone_planned_version),
				group_id: package.version_group_id.clone(),
				reasons: state.reasons.into_iter().collect(),
				upstream_sources: state.upstream_sources.into_iter().collect(),
				warnings: state.warnings,
			}
		})
		.collect();

	Ok(ReleasePlan {
		workspace_root: workspace_root.to_path_buf(),
		decisions,
		groups: planned_groups,
		warnings,
		unresolved_items: Vec::new(),
		compatibility_evidence: compatibility_evidence.to_vec(),
	})
}

type ExplicitVersionResolution = (
	BTreeMap<String, Version>,
	BTreeMap<String, Version>,
	Vec<String>,
);

fn resolve_explicit_versions(
	package_by_id: &BTreeMap<&str, &PackageRecord>,
	group_by_id: &BTreeMap<&str, &VersionGroup>,
	change_signals: &[ChangeSignal],
	strict_version_conflicts: bool,
) -> MonochangeResult<ExplicitVersionResolution> {
	let mut package_inputs = BTreeMap::<String, Vec<ExplicitVersionInput>>::new();
	let mut group_inputs = BTreeMap::<String, Vec<ExplicitVersionInput>>::new();
	let mut warnings = Vec::new();

	for signal in change_signals {
		let Some(version) = signal.explicit_version.clone() else {
			continue;
		};
		let input = ExplicitVersionInput {
			package_id: signal.package_id.clone(),
			source_path: signal.source_path.clone(),
			version,
		};
		if let Some(group_id) = package_by_id
			.get(signal.package_id.as_str())
			.and_then(|package| package.version_group_id.as_ref())
		{
			group_inputs
				.entry(group_id.clone())
				.or_default()
				.push(input);
		} else {
			package_inputs
				.entry(signal.package_id.clone())
				.or_default()
				.push(input);
		}
	}

	let package_versions = package_inputs
		.into_iter()
		.map(|(package_id, inputs)| {
			let package = package_by_id.get(package_id.as_str()).ok_or_else(|| {
				MonochangeError::Config(format!(
					"changeset references package `{package_id}` which was not found in the workspace"
				))
			})?;
			let owner = format!("package `{package_id}`");
			resolve_explicit_version_choice(
				&owner,
				&inputs,
				package.current_version.as_ref(),
				strict_version_conflicts,
				&mut warnings,
			)
			.map(|version| (package_id, version))
		})
		.collect::<MonochangeResult<BTreeMap<_, _>>>()?;

	let group_versions = group_inputs
		.into_iter()
		.map(|(group_id, inputs)| {
			let group = group_by_id.get(group_id.as_str()).ok_or_else(|| {
				MonochangeError::Config(format!(
					"changeset references group `{group_id}` which was not found in the workspace configuration"
				))
			})?;
			let current_version = group
				.members
				.iter()
				.filter_map(|member| package_by_id.get(member.as_str()))
				.filter_map(|package| package.current_version.as_ref())
				.max();
			let owner = format!(
				"group `{group_id}` (packages: {})",
				group.members.join(", ")
			);
			resolve_explicit_version_choice(
				&owner,
				&inputs,
				current_version,
				strict_version_conflicts,
				&mut warnings,
			)
			.map(|version| (group_id, version))
		})
		.collect::<MonochangeResult<BTreeMap<_, _>>>()?;

	Ok((package_versions, group_versions, warnings))
}

fn resolve_explicit_version_choice(
	owner: &str,
	inputs: &[ExplicitVersionInput],
	current_version: Option<&Version>,
	strict_version_conflicts: bool,
	warnings: &mut Vec<String>,
) -> MonochangeResult<Version> {
	let chosen_version = inputs
		.iter()
		.map(|input| input.version.clone())
		.max()
		.ok_or_else(|| {
			MonochangeError::Config(format!("no explicit version inputs found for {owner}"))
		})?;
	let distinct_versions = inputs
		.iter()
		.map(|input| input.version.clone())
		.collect::<BTreeSet<_>>();
	if distinct_versions.len() > 1 {
		let details = inputs
			.iter()
			.map(|input| {
				format!(
					"{} @ {} [{}]",
					input.version,
					input.source_path.display(),
					input.package_id
				)
			})
			.collect::<Vec<_>>()
			.join(", ");
		let message = format!(
			"conflicting explicit versions for {owner}; using highest version `{chosen_version}` from: {details}"
		);
		if strict_version_conflicts {
			return Err(MonochangeError::Config(message));
		}
		warnings.push(message);
	}
	if let Some(current_version) = current_version
		&& chosen_version <= *current_version
	{
		return Err(MonochangeError::Config(format!(
			"explicit version `{chosen_version}` for {owner} must be greater than current version `{current_version}`"
		)));
	}
	Ok(chosen_version)
}

#[derive(Debug, Clone)]
struct ExplicitVersionInput {
	package_id: String,
	source_path: PathBuf,
	version: Version,
}

fn apply_decision<'a>(
	states: &mut BTreeMap<&'a str, DecisionState>,
	queue: &mut VecDeque<&'a str>,
	package_id: &'a str,
	new_severity: BumpSeverity,
	trigger_type: &str,
	reason: &str,
	upstream_sources: &BTreeSet<String>,
) {
	let Some(state) = states.get_mut(package_id) else {
		return;
	};

	if new_severity > state.severity {
		state.severity = new_severity;
		queue.push_back(package_id);
	}
	if trigger_priority(trigger_type) > trigger_priority(&state.trigger_type) {
		state.trigger_type = trigger_type.to_string();
	}
	state.reasons.insert(reason.to_string());
	state
		.upstream_sources
		.extend(upstream_sources.iter().cloned());
}

fn trigger_priority(trigger_type: &str) -> u8 {
	match trigger_type {
		"direct-change" => 3,
		"version-group-synchronization" => 2,
		"transitive-dependency" => 1,
		_ => 0,
	}
}

fn planned_group(
	group: &VersionGroup,
	package_by_id: &BTreeMap<&str, &PackageRecord>,
	states: &BTreeMap<&str, DecisionState>,
	explicit_group_versions: &BTreeMap<String, Version>,
) -> Option<PlannedVersionGroup> {
	let recommended_bump = group
		.members
		.iter()
		.filter_map(|member| states.get(member.as_str()))
		.map(|state| state.severity)
		.max()
		.unwrap_or(BumpSeverity::None);
	if !recommended_bump.is_release() {
		return None;
	}

	let base_version = group
		.members
		.iter()
		.filter_map(|member| package_by_id.get(member.as_str()))
		.filter_map(|package| package.current_version.clone())
		.max();
	let planned_version = explicit_group_versions
		.get(&group.group_id)
		.cloned()
		.or_else(|| {
			base_version
				.as_ref()
				.map(|version| recommended_bump.apply_to_version(version))
		});

	Some(PlannedVersionGroup {
		group_id: group.group_id.clone(),
		display_name: group.display_name.clone(),
		members: group.members.clone(),
		mismatch_detected: group.mismatch_detected,
		planned_version,
		recommended_bump,
	})
}

#[cfg(test)]
mod __tests;
