use std::collections::BTreeMap;
use std::path::Path;

use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use serde_yaml_ng::Value;

use crate::adapter;
use crate::discover_dart_packages;
use crate::discover_lockfiles;
use crate::supported_versioned_file_kind;
use crate::update_dependency_fields;
use crate::update_pubspec_lock;
use crate::DartVersionedFileKind;

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

#[test]
fn adapter_reports_dart_ecosystem() {
	assert_eq!(adapter().ecosystem(), Ecosystem::Dart);
}

#[test]
fn supported_versioned_file_kind_recognizes_pubspec_files() {
	assert_eq!(
		supported_versioned_file_kind(Path::new("pubspec.yaml")),
		Some(DartVersionedFileKind::Manifest)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("pubspec.yml")),
		Some(DartVersionedFileKind::Manifest)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("pubspec.lock")),
		Some(DartVersionedFileKind::Lock)
	);
	assert_eq!(supported_versioned_file_kind(Path::new("README.md")), None);
}

#[test]
fn discover_lockfiles_prefers_workspace_root_then_manifest_directory() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/dart/lockfile-workspace");
	let package = PackageRecord::new(
		Ecosystem::Dart,
		"dart_app",
		fixture_root.join("packages/app/pubspec.yaml"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert_eq!(
		lockfiles.first(),
		Some(&monochange_core::normalize_path(
			&fixture_root.join("pubspec.lock")
		))
	);
}

#[test]
fn update_dependency_fields_only_changes_declared_dependencies() {
	let mut manifest: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(
		r"
dependencies:
  core: ^1.0.0
dev_dependencies:
  test: ^1.0.0
",
	)
	.unwrap_or_else(|error| panic!("pubspec yaml: {error}"));
	let versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_dependency_fields(
		&mut manifest,
		&["dependencies", "dev_dependencies"],
		&versions,
	);

	let rendered = serde_yaml_ng::to_string(&Value::Mapping(manifest))
		.unwrap_or_else(|error| panic!("render manifest: {error}"));
	assert!(rendered.contains("core: 2.0.0"));
	assert!(rendered.contains("test: ^1.0.0"));
}

#[test]
fn update_pubspec_lock_rewrites_known_package_versions() {
	let mut lock: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(
		r"
packages:
  core:
    version: 1.0.0
  app:
    version: 1.0.0
",
	)
	.unwrap_or_else(|error| panic!("pubspec lock yaml: {error}"));
	let versions = BTreeMap::from([
		("core".to_string(), "2.0.0".to_string()),
		("app".to_string(), "1.1.0".to_string()),
	]);

	update_pubspec_lock(&mut lock, &versions);

	let rendered = serde_yaml_ng::to_string(&Value::Mapping(lock))
		.unwrap_or_else(|error| panic!("render pubspec lock: {error}"));
	assert!(rendered.contains("core:\n    version: 2.0.0"));
	assert!(rendered.contains("app:\n    version: 1.1.0"));
}
