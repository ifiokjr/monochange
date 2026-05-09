use std::path::PathBuf;

use monochange_core::DependencyEdge;
use monochange_core::DependencyKind;
use monochange_core::DependencySourceKind;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use proptest::prelude::*;
use semver::Version;

use crate::NormalizedGraph;
use crate::propagation_is_suppressed;
use crate::trigger_priority;

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

#[test]
fn trigger_priority_returns_expected_values() {
	assert_eq!(trigger_priority("direct-change"), 3);
	assert_eq!(trigger_priority("version-group-synchronization"), 2);
	assert_eq!(trigger_priority("transitive-dependency"), 1);
	assert_eq!(trigger_priority("unknown"), 0);
	assert_eq!(trigger_priority("something-else"), 0);

	// Priority ordering
	assert!(trigger_priority("direct-change") > trigger_priority("version-group-synchronization"));
	assert!(
		trigger_priority("version-group-synchronization")
			> trigger_priority("transitive-dependency")
	);
	assert!(trigger_priority("transitive-dependency") > trigger_priority("unknown"));
}

proptest! {
	#[test]
	fn propagation_is_suppressed_reflects_suppression_set(
		dependent_id in "[a-z0-9]{1,20}",
		upstream_sources in prop::collection::btree_set("[a-z0-9]{1,20}", 0..10),
		suppression in prop::collection::btree_map(
			"[a-z0-9]{1,20}",
			prop::collection::btree_set("[a-z0-9]{1,20}", 0..10),
			0..10,
		),
	) {
		let expected = suppression.get(&dependent_id).is_some_and(|suppressed_sources| {
			suppressed_sources
				.iter()
				.any(|source| upstream_sources.contains(source))
		});
		let actual = propagation_is_suppressed(&dependent_id, &upstream_sources, &suppression);
		prop_assert_eq!(actual, expected);
	}

	#[test]
	fn reverse_edges_include_direct_dependents(
		package_ids in prop::collection::btree_set("[a-z0-9]{1,10}", 0..20),
		edge_indices in prop::collection::vec((any::<usize>(), any::<usize>()), 0..30),
	) {
		let package_ids_vec: Vec<String> = package_ids.iter().cloned().collect();
		let edges: Vec<DependencyEdge> = if package_ids_vec.is_empty() {
			Vec::new()
		} else {
			edge_indices
				.into_iter()
				.map(|(from_idx, to_idx)| {
					let from = package_ids_vec.get(from_idx % package_ids_vec.len()).cloned().unwrap_or_default();
					let to = package_ids_vec.get(to_idx % package_ids_vec.len()).cloned().unwrap_or_default();
					DependencyEdge {
						from_package_id: from,
						to_package_id: to,
						dependency_kind: DependencyKind::Runtime,
						source_kind: DependencySourceKind::Manifest,
						version_constraint: None,
						is_optional: false,
						is_direct: true,
					}
				})
				.collect()
		};

		let packages: Vec<PackageRecord> = package_ids_vec
			.iter()
			.map(|id| {
				let mut pkg = package(id, Version::new(1, 0, 0));
				pkg.id.clone_from(id);
				pkg
			})
			.collect();

		let graph = NormalizedGraph::new(&packages, &edges);

		for edge in &edges {
			let dependents = graph.direct_dependents(&edge.to_package_id);
			prop_assert!(
				dependents.contains(&edge.from_package_id.as_str()),
				"expected {} to be a direct dependent of {}",
				edge.from_package_id,
				edge.to_package_id
			);
		}
	}
}
