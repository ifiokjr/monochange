use std::path::PathBuf;

use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::CompatibilityAssessment;
use monochange_core::DependencyEdge;
use monochange_core::DependencyKind;
use monochange_core::DependencySourceKind;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::VersionGroup;
use semver::Version;

use crate::build_release_plan;
use crate::NormalizedGraph;

fn package(id: &str, version: Version) -> PackageRecord {
	let manifest_path = PathBuf::from(id.replace(':', "/")).join("manifest");
	let mut package = PackageRecord::new(
		Ecosystem::Cargo,
		id.to_string(),
		manifest_path,
		PathBuf::from("fixtures/mixed"),
		Some(version),
		PublishState::Public,
	);
	package.id = id.to_string();
	package
}

fn edge(from: &str, to: &str) -> DependencyEdge {
	DependencyEdge {
		from_package_id: from.to_string(),
		to_package_id: to.to_string(),
		dependency_kind: DependencyKind::Runtime,
		source_kind: DependencySourceKind::Manifest,
		version_constraint: None,
		is_optional: false,
		is_direct: true,
	}
}

#[test]
fn transitive_dependents_walks_the_reverse_graph() {
	let packages = [
		package("a", Version::new(1, 0, 0)),
		package("b", Version::new(1, 0, 0)),
		package("c", Version::new(1, 0, 0)),
	];
	let edges = [edge("b", "a"), edge("c", "b")];
	let graph = NormalizedGraph::new(&packages, &edges);

	let dependents = graph.transitive_dependents("a");
	assert!(dependents.contains("b"));
	assert!(dependents.contains("c"));
}

#[test]
fn transitive_dependents_handles_cycles_without_looping_forever() {
	let packages = [
		package("a", Version::new(1, 0, 0)),
		package("b", Version::new(1, 0, 0)),
	];
	let edges = [edge("a", "b"), edge("b", "a")];
	let graph = NormalizedGraph::new(&packages, &edges);

	let dependents = graph.transitive_dependents("a");
	assert!(dependents.contains("b"));
	assert_eq!(dependents.len(), 2);
}

#[test]
fn build_release_plan_patches_direct_parents_when_a_dependency_changes() {
	let packages = vec![
		package("cargo:core", Version::new(1, 0, 0)),
		package("cargo:app", Version::new(1, 0, 0)),
	];
	let plan = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&packages,
		&[edge("cargo:app", "cargo:core")],
		&[],
		&[ChangeSignal {
			package_id: "cargo:core".to_string(),
			requested_bump: Some(BumpSeverity::Minor),
			explicit_version: None,
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("feature".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/feature.md"),
		}],
		&[],
		BumpSeverity::Patch,
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let app = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == "cargo:app")
		.unwrap_or_else(|| panic!("expected app decision"));
	assert_eq!(app.recommended_bump, BumpSeverity::Patch);
	assert_eq!(app.trigger_type, "transitive-dependency");
	assert_eq!(app.planned_version, Some(Version::new(1, 0, 1)));
}

#[test]
fn build_release_plan_propagates_direct_and_transitive_dependency_impact() {
	let packages = vec![
		package("cargo:core", Version::new(1, 0, 0)),
		package("cargo:web", Version::new(1, 0, 0)),
		package("cargo:mobile", Version::new(1, 0, 0)),
	];
	let plan = build_release_plan(
		PathBuf::from("fixtures/mixed").as_path(),
		&packages,
		&[
			edge("cargo:web", "cargo:core"),
			edge("cargo:mobile", "cargo:web"),
		],
		&[],
		&[ChangeSignal {
			package_id: "cargo:core".to_string(),
			requested_bump: Some(BumpSeverity::Minor),
			explicit_version: None,
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("public API addition".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/feature.md"),
		}],
		&[],
		BumpSeverity::Patch,
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let core = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == "cargo:core")
		.unwrap_or_else(|| panic!("expected core decision"));
	let web = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == "cargo:web")
		.unwrap_or_else(|| panic!("expected web decision"));
	let mobile = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == "cargo:mobile")
		.unwrap_or_else(|| panic!("expected mobile decision"));

	assert_eq!(core.recommended_bump, BumpSeverity::Minor);
	assert_eq!(web.recommended_bump, BumpSeverity::Patch);
	assert_eq!(mobile.recommended_bump, BumpSeverity::Patch);
}

#[test]
fn build_release_plan_synchronizes_version_groups() {
	let mut core = package("cargo:core", Version::new(1, 0, 0));
	core.version_group_id = Some("sdk".to_string());
	let mut web = package("npm:web", Version::new(1, 0, 0));
	web.version_group_id = Some("sdk".to_string());
	let mobile = package("dart:mobile", Version::new(1, 0, 0));
	let version_group = VersionGroup {
		group_id: "sdk".to_string(),
		display_name: "sdk".to_string(),
		members: vec![core.id.clone(), web.id.clone()],
		mismatch_detected: false,
	};

	let plan = build_release_plan(
		PathBuf::from("fixtures/mixed").as_path(),
		&[core.clone(), web.clone(), mobile],
		&[edge("dart:mobile", "npm:web")],
		&[version_group],
		&[ChangeSignal {
			package_id: core.id.clone(),
			requested_bump: Some(BumpSeverity::Minor),
			explicit_version: None,
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("feature".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/feature.md"),
		}],
		&[],
		BumpSeverity::Patch,
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let synced_member = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == web.id)
		.unwrap_or_else(|| panic!("expected version-group member"));
	let mobile_decision = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == "dart:mobile")
		.unwrap_or_else(|| panic!("expected mobile decision"));
	let group = plan
		.groups
		.first()
		.unwrap_or_else(|| panic!("expected planned version group"));

	assert_eq!(synced_member.recommended_bump, BumpSeverity::Minor);
	assert_eq!(synced_member.trigger_type, "version-group-synchronization");
	assert_eq!(mobile_decision.recommended_bump, BumpSeverity::Patch);
	assert_eq!(group.planned_version, Some(Version::new(1, 1, 0)));
}

#[test]
fn build_release_plan_shifts_major_to_minor_for_pre_stable_versions() {
	let packages = vec![
		package("cargo:core", Version::new(0, 1, 0)),
		package("cargo:app", Version::new(0, 1, 0)),
	];
	let plan = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&packages,
		&[edge("cargo:app", "cargo:core")],
		&[],
		&[ChangeSignal {
			package_id: "cargo:core".to_string(),
			requested_bump: Some(BumpSeverity::Major),
			explicit_version: None,
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("breaking change".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/feature.md"),
		}],
		&[],
		BumpSeverity::Patch,
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let core = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == "cargo:core")
		.unwrap_or_else(|| panic!("expected core decision"));
	let app = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == "cargo:app")
		.unwrap_or_else(|| panic!("expected app decision"));

	// major requested on 0.1.0 → planned version should be 0.2.0
	assert_eq!(core.recommended_bump, BumpSeverity::Major);
	assert_eq!(core.planned_version, Some(Version::new(0, 2, 0)));

	// transitive dependent gets patch on 0.1.0 → 0.1.1
	assert_eq!(app.recommended_bump, BumpSeverity::Patch);
	assert_eq!(app.planned_version, Some(Version::new(0, 1, 1)));
}

#[test]
fn build_release_plan_uses_compatibility_assessments_to_escalate_parents() {
	let packages = vec![
		package("cargo:core", Version::new(1, 0, 0)),
		package("cargo:web", Version::new(1, 0, 0)),
	];
	let plan = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&packages,
		&[edge("cargo:web", "cargo:core")],
		&[],
		&[ChangeSignal {
			package_id: "cargo:core".to_string(),
			requested_bump: None,
			explicit_version: None,
			change_origin: "direct-change".to_string(),
			evidence_refs: vec!["rust-semver:major:public API break detected".to_string()],
			notes: Some("breaking change".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/feature.md"),
		}],
		&[CompatibilityAssessment {
			package_id: "cargo:core".to_string(),
			provider_id: "rust-semver".to_string(),
			severity: BumpSeverity::Major,
			confidence: "high".to_string(),
			summary: "public API break detected".to_string(),
			evidence_location: None,
		}],
		BumpSeverity::Patch,
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let web = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == "cargo:web")
		.unwrap_or_else(|| panic!("expected web decision"));
	assert_eq!(web.recommended_bump, BumpSeverity::Major);
}

#[test]
fn build_release_plan_uses_explicit_package_versions() {
	let packages = vec![package("cargo:core", Version::new(1, 0, 0))];
	let plan = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&packages,
		&[],
		&[],
		&[ChangeSignal {
			package_id: "cargo:core".to_string(),
			requested_bump: Some(BumpSeverity::Patch),
			explicit_version: Some(Version::new(1, 2, 0)),
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("pin release".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/pin.md"),
		}],
		&[],
		BumpSeverity::Patch,
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let decision = plan
		.decisions
		.first()
		.unwrap_or_else(|| panic!("expected one decision"));
	assert_eq!(decision.planned_version, Some(Version::new(1, 2, 0)));
}

#[test]
fn build_release_plan_propagates_explicit_member_versions_to_group_version() {
	let mut core = package("cargo:core", Version::new(1, 0, 0));
	core.version_group_id = Some("sdk".to_string());
	let mut web = package("npm:web", Version::new(1, 0, 0));
	web.version_group_id = Some("sdk".to_string());
	let version_group = VersionGroup {
		group_id: "sdk".to_string(),
		display_name: "sdk".to_string(),
		members: vec![core.id.clone(), web.id.clone()],
		mismatch_detected: false,
	};
	let plan = build_release_plan(
		PathBuf::from("fixtures/mixed").as_path(),
		&[core.clone(), web.clone()],
		&[],
		&[version_group],
		&[ChangeSignal {
			package_id: core.id.clone(),
			requested_bump: Some(BumpSeverity::Major),
			explicit_version: Some(Version::new(2, 0, 0)),
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("promote sdk".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/group-pin.md"),
		}],
		&[],
		BumpSeverity::Patch,
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let group = plan
		.groups
		.first()
		.unwrap_or_else(|| panic!("expected one group"));
	assert_eq!(group.planned_version, Some(Version::new(2, 0, 0)));
	assert!(plan
		.decisions
		.iter()
		.all(|decision| decision.planned_version == Some(Version::new(2, 0, 0))));
}

#[test]
fn build_release_plan_uses_highest_conflicting_explicit_version_with_warning() {
	let packages = vec![package("cargo:core", Version::new(1, 0, 0))];
	let plan = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&packages,
		&[],
		&[],
		&[
			ChangeSignal {
				package_id: "cargo:core".to_string(),
				requested_bump: Some(BumpSeverity::Minor),
				explicit_version: Some(Version::new(1, 1, 0)),
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: Some("first pin".to_string()),
				details: None,
				change_type: None,
				source_path: PathBuf::from(".changeset/001-first.md"),
			},
			ChangeSignal {
				package_id: "cargo:core".to_string(),
				requested_bump: Some(BumpSeverity::Major),
				explicit_version: Some(Version::new(2, 0, 0)),
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: Some("second pin".to_string()),
				details: None,
				change_type: None,
				source_path: PathBuf::from(".changeset/002-second.md"),
			},
		],
		&[],
		BumpSeverity::Patch,
		false,
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));

	let decision = plan
		.decisions
		.first()
		.unwrap_or_else(|| panic!("expected one decision"));
	let warning = plan
		.warnings
		.first()
		.unwrap_or_else(|| panic!("expected one warning"));
	assert_eq!(decision.planned_version, Some(Version::new(2, 0, 0)));
	assert_eq!(plan.warnings.len(), 1);
	assert!(warning.contains("conflicting explicit versions"));
	assert!(warning.contains("001-first.md"));
	assert!(warning.contains("002-second.md"));
}

#[test]
fn build_release_plan_rejects_conflicting_explicit_versions_in_strict_mode() {
	let packages = vec![package("cargo:core", Version::new(1, 0, 0))];
	let error = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&packages,
		&[],
		&[],
		&[
			ChangeSignal {
				package_id: "cargo:core".to_string(),
				requested_bump: Some(BumpSeverity::Minor),
				explicit_version: Some(Version::new(1, 1, 0)),
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: Some("first pin".to_string()),
				details: None,
				change_type: None,
				source_path: PathBuf::from(".changeset/001-first.md"),
			},
			ChangeSignal {
				package_id: "cargo:core".to_string(),
				requested_bump: Some(BumpSeverity::Major),
				explicit_version: Some(Version::new(2, 0, 0)),
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: Some("second pin".to_string()),
				details: None,
				change_type: None,
				source_path: PathBuf::from(".changeset/002-second.md"),
			},
		],
		&[],
		BumpSeverity::Patch,
		true,
	)
	.err()
	.unwrap_or_else(|| panic!("expected strict conflict error"));

	assert!(error.to_string().contains("conflicting explicit versions"));
}

#[test]
fn build_release_plan_rejects_explicit_versions_not_greater_than_current() {
	let packages = vec![package("cargo:core", Version::new(1, 0, 0))];
	let error = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&packages,
		&[],
		&[],
		&[ChangeSignal {
			package_id: "cargo:core".to_string(),
			requested_bump: Some(BumpSeverity::Patch),
			explicit_version: Some(Version::new(1, 0, 0)),
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("same version".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/same.md"),
		}],
		&[],
		BumpSeverity::Patch,
		false,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid explicit version error"));

	assert!(error
		.to_string()
		.contains("must be greater than current version"));
}

#[test]
fn build_release_plan_returns_error_for_unknown_package_in_changeset() {
	let packages = vec![package("cargo:core", Version::new(1, 0, 0))];
	let error = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&packages,
		&[],
		&[],
		&[ChangeSignal {
			package_id: "cargo:nonexistent".to_string(),
			requested_bump: Some(BumpSeverity::Patch),
			explicit_version: Some(Version::new(2, 0, 0)),
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: None,
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/ghost.md"),
		}],
		&[],
		BumpSeverity::Patch,
		false,
	)
	.err()
	.unwrap_or_else(|| panic!("expected error for unknown package"));
	assert!(error.to_string().contains("cargo:nonexistent"));
	assert!(error.to_string().contains("not found"));
}

#[test]
fn build_release_plan_returns_error_for_unknown_group_in_changeset() {
	let mut core = package("cargo:core", Version::new(1, 0, 0));
	core.version_group_id = Some("sdk".to_string());
	let version_group = VersionGroup {
		group_id: "sdk".to_string(),
		display_name: "sdk".to_string(),
		members: vec![core.id.clone()],
		mismatch_detected: false,
	};
	// Create a changeset that targets the group member, which maps to a group,
	// but use a version_group_id that doesn't match any defined group.
	let mut misrouted = package("cargo:orphan", Version::new(1, 0, 0));
	misrouted.version_group_id = Some("nonexistent-group".to_string());
	let error = build_release_plan(
		PathBuf::from("fixtures/cargo").as_path(),
		&[core, misrouted.clone()],
		&[],
		&[version_group],
		&[ChangeSignal {
			package_id: misrouted.id.clone(),
			requested_bump: Some(BumpSeverity::Patch),
			explicit_version: Some(Version::new(2, 0, 0)),
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: None,
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/orphan.md"),
		}],
		&[],
		BumpSeverity::Patch,
		false,
	)
	.err()
	.unwrap_or_else(|| panic!("expected error for unknown group"));
	assert!(error.to_string().contains("nonexistent-group"));
	assert!(error.to_string().contains("not found"));
}
