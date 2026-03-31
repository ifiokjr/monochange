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
	let graph = NormalizedGraph::new(
		&[
			package("a", Version::new(1, 0, 0)),
			package("b", Version::new(1, 0, 0)),
			package("c", Version::new(1, 0, 0)),
		],
		&[edge("b", "a"), edge("c", "b")],
	);

	let dependents = graph.transitive_dependents("a");
	assert!(dependents.contains("b"));
	assert!(dependents.contains("c"));
}

#[test]
fn transitive_dependents_handles_cycles_without_looping_forever() {
	let graph = NormalizedGraph::new(
		&[
			package("a", Version::new(1, 0, 0)),
			package("b", Version::new(1, 0, 0)),
		],
		&[edge("a", "b"), edge("b", "a")],
	);

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
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("feature".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/feature.md"),
		}],
		&[],
		BumpSeverity::Patch,
	);

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
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("public API addition".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/feature.md"),
		}],
		&[],
		BumpSeverity::Patch,
	);

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
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("feature".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/feature.md"),
		}],
		&[],
		BumpSeverity::Patch,
	);

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
			change_origin: "direct-change".to_string(),
			evidence_refs: Vec::new(),
			notes: Some("breaking change".to_string()),
			details: None,
			change_type: None,
			source_path: PathBuf::from(".changeset/feature.md"),
		}],
		&[],
		BumpSeverity::Patch,
	);

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
	);

	let web = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id == "cargo:web")
		.unwrap_or_else(|| panic!("expected web decision"));
	assert_eq!(web.recommended_bump, BumpSeverity::Major);
}
