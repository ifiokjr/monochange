use std::path::Path;

use monochange_core::materialize_dependency_edges;

use crate::discover_deno_packages;

#[test]
fn discovers_deno_workspace_packages() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/deno/workspace");
	let discovery = discover_deno_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("deno discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.name == "deno-tool"));
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.name == "deno-shared"));
	let dependency_edges = materialize_dependency_edges(&discovery.packages);
	assert_eq!(dependency_edges.len(), 1);
}
