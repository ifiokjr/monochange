#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

doc_comment::doctest!("../readme.md");

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::path::Path;

use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::CompatibilityAssessment;
use monochange_core::DependencyEdge;
use monochange_core::PackageRecord;
use monochange_core::PlannedVersionGroup;
use monochange_core::ReleaseDecision;
use monochange_core::ReleasePlan;
use monochange_core::VersionGroup;
use monochange_semver::direct_release_severity;
use monochange_semver::propagated_release_severity;
use monochange_semver::strongest_assessment_for_package;

#[derive(Debug, Clone)]
pub struct NormalizedGraph {
	package_ids: BTreeSet<String>,
	reverse_edges: BTreeMap<String, BTreeSet<String>>,
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

impl NormalizedGraph {
	#[must_use]
	pub fn new(packages: &[PackageRecord], dependency_edges: &[DependencyEdge]) -> Self {
		let mut reverse_edges = BTreeMap::<String, BTreeSet<String>>::new();
		let package_ids = packages.iter().map(|package| package.id.clone()).collect();

		for edge in dependency_edges {
			reverse_edges
				.entry(edge.to_package_id.clone())
				.or_default()
				.insert(edge.from_package_id.clone());
		}

		Self {
			package_ids,
			reverse_edges,
		}
	}

	#[must_use]
	pub fn direct_dependents(&self, package_id: &str) -> BTreeSet<String> {
		self.reverse_edges
			.get(package_id)
			.cloned()
			.unwrap_or_default()
	}

	#[must_use]
	pub fn transitive_dependents(&self, package_id: &str) -> BTreeSet<String> {
		let mut discovered = BTreeSet::new();
		let mut queue = VecDeque::from([package_id.to_string()]);

		while let Some(current) = queue.pop_front() {
			for dependent in self.direct_dependents(&current) {
				if discovered.insert(dependent.clone()) {
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

#[must_use]
pub fn build_release_plan(
	workspace_root: &Path,
	packages: &[PackageRecord],
	dependency_edges: &[DependencyEdge],
	version_groups: &[VersionGroup],
	change_signals: &[ChangeSignal],
	compatibility_evidence: &[CompatibilityAssessment],
	default_parent_bump: BumpSeverity,
) -> ReleasePlan {
	let graph = NormalizedGraph::new(packages, dependency_edges);
	let package_by_id = packages
		.iter()
		.map(|package| (package.id.clone(), package))
		.collect::<BTreeMap<_, _>>();
	let group_by_id = version_groups
		.iter()
		.map(|group| (group.group_id.clone(), group))
		.collect::<BTreeMap<_, _>>();
	let mut states = packages
		.iter()
		.map(|package| {
			(
				package.id.clone(),
				DecisionState {
					trigger_type: "none".to_string(),
					..DecisionState::default()
				},
			)
		})
		.collect::<BTreeMap<_, _>>();
	let mut queue = VecDeque::new();

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

	while let Some(source_package_id) = queue.pop_front() {
		let source_state = if let Some(state) = states.get(&source_package_id) {
			state.clone()
		} else {
			continue;
		};
		if !source_state.severity.is_release() {
			continue;
		}

		let source_assessment =
			strongest_assessment_for_package(compatibility_evidence, &source_package_id);
		let propagated_severity =
			propagated_release_severity(default_parent_bump, source_assessment.as_ref());

		if propagated_severity.is_release() {
			for dependent_id in graph.direct_dependents(&source_package_id) {
				let reason = format!("depends on `{source_package_id}`");
				apply_decision(
					&mut states,
					&mut queue,
					&dependent_id,
					propagated_severity,
					"transitive-dependency",
					&reason,
					&source_state.upstream_sources,
				);
			}
		}

		if let Some(group_id) = package_by_id
			.get(&source_package_id)
			.and_then(|package| package.version_group_id.as_deref())
		{
			let Some(group) = group_by_id.get(group_id) else {
				continue;
			};
			let group_max = group
				.members
				.iter()
				.filter_map(|member| states.get(member))
				.map(|state| state.severity)
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
	}

	let planned_groups = version_groups
		.iter()
		.filter_map(|group| planned_group(group, &package_by_id, &states))
		.collect::<Vec<_>>();

	let decisions = packages
		.iter()
		.map(|package| {
			let state = states.get(&package.id).cloned().unwrap_or_default();
			let planned_version = package.version_group_id.as_deref().and_then(|group_id| {
				planned_groups
					.iter()
					.find(|group| group.group_id == group_id)
					.and_then(|group| group.planned_version.clone())
			});
			let standalone_planned_version =
				if planned_version.is_none() && state.severity.is_release() {
					package
						.current_version
						.as_ref()
						.map(|version| state.severity.apply_to_version(version))
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

	ReleasePlan {
		workspace_root: workspace_root.to_path_buf(),
		decisions,
		groups: planned_groups,
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: compatibility_evidence.to_vec(),
	}
}

fn apply_decision(
	states: &mut BTreeMap<String, DecisionState>,
	queue: &mut VecDeque<String>,
	package_id: &str,
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
		queue.push_back(package_id.to_string());
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
	package_by_id: &BTreeMap<String, &PackageRecord>,
	states: &BTreeMap<String, DecisionState>,
) -> Option<PlannedVersionGroup> {
	let recommended_bump = group
		.members
		.iter()
		.filter_map(|member| states.get(member))
		.map(|state| state.severity)
		.max()
		.unwrap_or(BumpSeverity::None);
	if !recommended_bump.is_release() {
		return None;
	}

	let base_version = group
		.members
		.iter()
		.filter_map(|member| package_by_id.get(member))
		.filter_map(|package| package.current_version.clone())
		.max();
	let planned_version = base_version
		.as_ref()
		.map(|version| recommended_bump.apply_to_version(version));

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
