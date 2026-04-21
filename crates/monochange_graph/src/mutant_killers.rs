#[cfg(test)]
mod mutant_killers {
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

	use crate::NormalizedGraph;
	use crate::build_release_plan;

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

	// -- Kill mutant: contains() always returns true/false --

	#[test]
	fn normalized_graph_contains_distinguishes_present_and_absent() {
		let packages = [
			package("a", Version::new(1, 0, 0)),
			package("b", Version::new(1, 0, 0)),
		];
		let edges = [edge("b", "a")];
		let graph = NormalizedGraph::new(&packages, &edges);

		assert!(graph.contains("a"), "package 'a' should be in graph");
		assert!(graph.contains("b"), "package 'b' should be in graph");
		assert!(!graph.contains("c"), "package 'c' should not be in graph");
	}

	// -- Kill mutant: > replaced with >= in distinct_versions.len() check --

	#[test]
	fn build_release_plan_has_no_warning_for_single_explicit_version() {
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
				notes: None,
				details: None,
				change_type: None,
				caused_by: Vec::new(),
				source_path: PathBuf::from(".changeset/pin.md"),
			}],
			&[],
			BumpSeverity::Patch,
			false,
		)
		.unwrap_or_else(|error| panic!("release plan: {error}"));

		assert!(
			plan.warnings.is_empty(),
			"single explicit version should not produce a warning, got: {:?}",
			plan.warnings
		);
	}

	// -- Kill mutant: && replaced with || in planned_version.is_none() check --

	#[test]
	fn build_release_plan_leaves_unreleased_package_without_planned_version() {
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
				requested_bump: Some(BumpSeverity::Patch),
				explicit_version: None,
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: None,
				details: None,
				change_type: None,
				caused_by: Vec::new(),
				source_path: PathBuf::from(".changeset/core.md"),
			}],
			&[],
			BumpSeverity::None,
			false,
		)
		.unwrap_or_else(|error| panic!("release plan: {error}"));

		// With default_parent_bump = None, app gets None severity.
		let app = plan
			.decisions
			.iter()
			.find(|d| d.package_id == "cargo:app")
			.unwrap_or_else(|| panic!("expected app decision"));
		assert_eq!(app.recommended_bump, BumpSeverity::None);
		assert!(
			app.planned_version.is_none(),
			"unreleased dependent should have no planned_version, got: {:?}",
			app.planned_version
		);
	}

	#[test]
	fn build_release_plan_uses_group_version_not_standalone_for_group_members() {
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
				requested_bump: Some(BumpSeverity::Minor),
				explicit_version: None,
				change_origin: "direct-change".to_string(),
				evidence_refs: Vec::new(),
				notes: Some("feature".to_string()),
				details: None,
				change_type: None,
				caused_by: Vec::new(),
				source_path: PathBuf::from(".changeset/feature.md"),
			}],
			&[],
			BumpSeverity::Patch,
			false,
		)
		.unwrap_or_else(|error| panic!("release plan: {error}"));

		// Both members should have the group's planned version, not a standalone one.
		let core_decision = plan
			.decisions
			.iter()
			.find(|d| d.package_id == core.id)
			.unwrap_or_else(|| panic!("expected core decision"));
		let web_decision = plan
			.decisions
			.iter()
			.find(|d| d.package_id == web.id)
			.unwrap_or_else(|| panic!("expected web decision"));

		assert_eq!(
			core_decision.planned_version,
			Some(Version::new(1, 1, 0)),
			"core should have group planned version"
		);
		assert_eq!(
			web_decision.planned_version,
			Some(Version::new(1, 1, 0)),
			"web should have group planned version"
		);
	}
}
