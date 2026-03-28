use std::path::Path;

use crate::discover_dart_packages;

#[test]
fn discovers_dart_workspace_packages() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dart/workspace");
	let discovery = discover_dart_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("dart discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.name == "dart_shared"));
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.name == "dart_app"));
}

#[test]
fn marks_flutter_packages_with_flutter_ecosystem() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/flutter/workspace");
	let discovery = discover_dart_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("flutter discovery: {error}"));

	assert!(discovery
		.packages
		.iter()
		.all(|package| package.ecosystem.as_str() == "flutter"));
}
