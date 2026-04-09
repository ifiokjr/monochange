use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::materialize_dependency_edges;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use serde_json::json;

use crate::adapter;
use crate::default_lockfile_commands;
use crate::discover_deno_packages;
use crate::discover_lockfiles;
use crate::supported_versioned_file_kind;
use crate::update_lockfile;
use crate::DenoVersionedFileKind;

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
	let tempdir = std::env::temp_dir().join(format!(
		"monochange-deno-jsonc-{}-{}",
		std::process::id(),
		std::thread::current().name().unwrap_or("unnamed")
	));
	let _ = std::fs::remove_dir_all(&tempdir);
	std::fs::create_dir_all(&tempdir)
		.unwrap_or_else(|error| panic!("create tempdir {}: {error}", tempdir.display()));
	std::fs::write(
		tempdir.join("deno.jsonc"),
		r#"{
  // keep comment
  "name": "jsonc-tool",
  "version": "1.0.0",
  "imports": {
    "core": "^1.0.0"
  }
}
"#,
	)
	.unwrap_or_else(|error| panic!("write deno.jsonc: {error}"));
	let discovery = discover_deno_packages(&tempdir)
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
	std::fs::remove_dir_all(&tempdir)
		.unwrap_or_else(|error| panic!("cleanup tempdir {}: {error}", tempdir.display()));
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
