use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::materialize_dependency_edges;
use semver::Version;
use serde_json::json;
use tempfile::tempdir;

use crate::DenoVersionedFileKind;
use crate::adapter;
use crate::default_lockfile_commands;
use crate::discover_deno_packages;
use crate::discover_lockfiles;
use crate::supported_versioned_file_kind;
use crate::update_lockfile;

#[test]
fn discovers_deno_workspace_packages() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/deno/workspace");
	let discovery = discover_deno_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("deno discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "deno-tool")
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "deno-shared")
	);
	let dependency_edges = materialize_dependency_edges(&discovery.packages);
	assert_eq!(dependency_edges.len(), 1);
	assert_eq!(
		dependency_edges.first().unwrap().source_field.as_deref(),
		Some("dependencies")
	);
}

#[test]
fn adapter_reports_deno_ecosystem() {
	assert_eq!(adapter().ecosystem(), Ecosystem::Deno);
}

#[test]
fn supported_versioned_file_kind_recognizes_manifest_and_lockfiles() {
	assert_eq!(
		supported_versioned_file_kind(Path::new("deno.json")),
		Some(DenoVersionedFileKind::Manifest)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("deno.jsonc")),
		Some(DenoVersionedFileKind::Manifest)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("deno.lock")),
		Some(DenoVersionedFileKind::Lock)
	);
	assert_eq!(supported_versioned_file_kind(Path::new("README.md")), None);
}

#[test]
fn discovers_deno_jsonc_manifests_with_comments() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/deno/jsonc-manifest");
	let discovery = discover_deno_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("discover deno jsonc: {error}"));
	assert_eq!(discovery.packages.len(), 1);
	let package = discovery.packages.first().expect("discovered deno package");
	assert_eq!(package.name, "jsonc-tool");
	assert_eq!(
		package
			.current_version
			.as_ref()
			.map(ToString::to_string)
			.as_deref(),
		Some("1.0.0")
	);
}

#[test]
fn update_lockfile_rewrites_npm_dependency_versions() {
	let mut lock = json!({
		"packages": {
			"jsr:@scope/pkg": "1.0.0",
			"npm:core@1.0.0": {
				"integrity": "sha512-test"
			},
			"other": "core@1.0.0"
		}
	});
	let versions = BTreeMap::from([("core".to_string(), "2.1.0".to_string())]);

	update_lockfile(&mut lock, &versions);

	let rendered = serde_json::to_string(&lock).unwrap_or_else(|error| panic!("json: {error}"));
	assert!(rendered.contains("npm:core@2.1.0"));
	assert!(rendered.contains("core@2.1.0"));
}

#[test]
fn discover_lockfiles_prefers_workspace_root_then_manifest_directory() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/monochange/deno-lock-release");
	let package = PackageRecord::new(
		Ecosystem::Deno,
		"workflow-app",
		fixture_root.join("packages/app/deno.json"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert_eq!(
		lockfiles.first(),
		Some(&monochange_core::normalize_path(
			&fixture_root.join("packages/app/deno.lock")
		))
	);
}

#[test]
fn discover_lockfiles_returns_empty_when_no_lockfile_exists() {
	let workspace_root = PathBuf::from("/tmp/deno-workspace");
	let package = PackageRecord::new(
		Ecosystem::Deno,
		"tool",
		workspace_root.join("tools/deno.json"),
		workspace_root,
		None,
		PublishState::Public,
	);
	assert!(discover_lockfiles(&package).is_empty());
}

#[test]
fn default_lockfile_commands_do_not_infer_a_deno_command() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/monochange/deno-lock-release");
	let package = PackageRecord::new(
		Ecosystem::Deno,
		"workflow-app",
		fixture_root.join("packages/app/deno.json"),
		fixture_root,
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert!(default_lockfile_commands(&package).is_empty());
}

#[test]
fn default_dependency_version_prefix_is_correct() {
	assert_eq!(super::default_dependency_version_prefix(), "^");
}

#[test]
fn default_dependency_fields_are_non_empty() {
	assert!(!super::default_dependency_fields().is_empty());
}

#[test]
fn validate_versioned_file_accepts_valid_deno_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("deno.json");
	fs::write(&path, r#"{"version": "1.0.0"}"#).unwrap_or_else(|error| panic!("write: {error}"));
	assert!(super::validate_versioned_file(&path, "deno.json", None).is_ok());
}

#[test]
fn validate_versioned_file_accepts_custom_field() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("deno.json");
	fs::write(&path, r#"{"customVersion": "1.0.0"}"#)
		.unwrap_or_else(|error| panic!("write: {error}"));
	let custom_fields = vec!["customVersion".to_string()];
	assert!(super::validate_versioned_file(&path, "deno.json", Some(&custom_fields)).is_ok());
}

#[test]
fn validate_versioned_file_rejects_invalid_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("deno.json");
	fs::write(&path, "not json").unwrap_or_else(|error| panic!("write: {error}"));
	let result = super::validate_versioned_file(&path, "deno.json", None);
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("not valid JSON"));
}

#[test]
fn validate_versioned_file_rejects_missing_version() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("deno.json");
	fs::write(&path, r#"{"name": "test"}"#).unwrap_or_else(|error| panic!("write: {error}"));
	let result = super::validate_versioned_file(&path, "deno.json", None);
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("does not contain a `version` string field")
	);
}

#[test]
fn validate_versioned_file_rejects_missing_file() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("missing.json");
	let result = super::validate_versioned_file(&path, "missing.json", None);
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("not readable"));
}
