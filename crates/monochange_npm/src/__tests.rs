use std::path::Path;

use monochange_core::materialize_dependency_edges;

use crate::discover_npm_packages;

#[test]
fn discovers_npm_workspace_packages() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.name == "npm-web"));
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.name == "npm-shared"));
	let dependency_edges = materialize_dependency_edges(&discovery.packages);
	assert_eq!(dependency_edges.len(), 1);
}

#[test]
fn discovers_pnpm_workspace_globs() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace-pnpm");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("pnpm discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.name == "pnpm-web"));
}

#[test]
fn discovers_bun_workspace_packages() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace-bun");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("bun discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	let web_package = discovery
		.packages
		.iter()
		.find(|package| package.name == "bun-web")
		.unwrap_or_else(|| panic!("bun web package should exist"));
	assert_eq!(
		web_package.metadata.get("manager").map(String::as_str),
		Some("bun")
	);
}
