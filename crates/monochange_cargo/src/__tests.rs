use std::path::Path;
use std::path::PathBuf;

use monochange_core::materialize_dependency_edges;
use monochange_core::ChangeSignal;
use monochange_semver::CompatibilityProvider;

use crate::discover_cargo_packages;
use crate::RustSemverProvider;

#[test]
fn discovers_cargo_workspace_members() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cargo/workspace");
	let discovery = discover_cargo_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("cargo discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.name == "cargo-core"));
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.name == "cargo-app"));
	let dependency_edges = materialize_dependency_edges(&discovery.packages);
	assert_eq!(dependency_edges.len(), 1);
	assert!(dependency_edges
		.iter()
		.any(|edge| edge.to_package_id.contains("crates/core/Cargo.toml")));
}

#[test]
fn cargo_workspace_members_inherit_workspace_package_versions() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cargo/workspace-versioned");
	let discovery = discover_cargo_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("cargo discovery: {error}"));
	let package = discovery
		.packages
		.iter()
		.find(|package| package.name == "workspace-core")
		.unwrap_or_else(|| panic!("expected workspace-core package"));

	assert_eq!(
		package
			.current_version
			.as_ref()
			.map(ToString::to_string)
			.as_deref(),
		Some("2.3.4")
	);
}

#[test]
fn cargo_workspace_members_mark_uses_workspace_version_metadata() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cargo/workspace-versioned");
	let discovery = discover_cargo_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("cargo discovery: {error}"));

	let core_package = discovery
		.packages
		.iter()
		.find(|package| package.name == "workspace-core")
		.unwrap_or_else(|| panic!("expected workspace-core package"));
	assert_eq!(
		core_package
			.metadata
			.get("uses_workspace_version")
			.map(String::as_str),
		Some("true")
	);

	let app_package = discovery
		.packages
		.iter()
		.find(|package| package.name == "workspace-app")
		.unwrap_or_else(|| panic!("expected workspace-app package"));
	assert_eq!(
		app_package
			.metadata
			.get("uses_workspace_version")
			.map(String::as_str),
		None
	);
}

#[test]
fn rust_semver_provider_parses_compatibility_evidence() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cargo/workspace");
	let discovery = discover_cargo_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("cargo discovery: {error}"));
	let package = discovery
		.packages
		.iter()
		.find(|package| package.name == "cargo-core")
		.unwrap_or_else(|| panic!("expected cargo-core package"));
	let signal = ChangeSignal {
		package_id: package.id.clone(),
		requested_bump: None,
		explicit_version: None,
		change_origin: "direct-change".to_string(),
		evidence_refs: vec!["rust-semver:major:public API break detected".to_string()],
		notes: Some("breaking change".to_string()),
		details: None,
		change_type: None,
		source_path: PathBuf::from(".changeset/feature.md"),
	};
	let provider = RustSemverProvider;
	let assessment = provider
		.assess(package, &signal)
		.unwrap_or_else(|| panic!("expected semver assessment"));

	assert_eq!(provider.provider_id(), "rust-semver");
	assert_eq!(assessment.severity.to_string(), "major");
	assert_eq!(assessment.summary, "public API break detected");
}
