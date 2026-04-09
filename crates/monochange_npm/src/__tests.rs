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
use serde_yaml_ng::Value as YamlValue;

use crate::adapter;
use crate::detect_npm_manager;
use crate::discover_lockfiles;
use crate::discover_npm_packages;
use crate::discover_package_json_workspace;
use crate::discover_pnpm_workspace;
use crate::expand_member_patterns;
use crate::package_json_declares_workspaces;
use crate::parse_package_json;
use crate::supported_versioned_file_kind;
use crate::update_bun_lock;
use crate::update_bun_lock_binary;
use crate::update_json_dependency_fields;
use crate::update_package_lock;
use crate::update_pnpm_lock;
use crate::workspace_patterns_from_package_json;
use crate::NpmVersionedFileKind;

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

#[test]
fn adapter_reports_npm_ecosystem() {
	assert_eq!(adapter().ecosystem(), Ecosystem::Npm);
}

#[test]
fn supported_versioned_file_kind_recognizes_known_files() {
	assert_eq!(
		supported_versioned_file_kind(Path::new("package.json")),
		Some(NpmVersionedFileKind::Manifest)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("package-lock.json")),
		Some(NpmVersionedFileKind::PackageLock)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("pnpm-lock.yaml")),
		Some(NpmVersionedFileKind::PnpmLock)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("bun.lock")),
		Some(NpmVersionedFileKind::BunLock)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("bun.lockb")),
		Some(NpmVersionedFileKind::BunLockBinary)
	);
	assert_eq!(supported_versioned_file_kind(Path::new("README.md")), None);
}

#[test]
fn discover_lockfiles_prefers_workspace_root_then_manifest_directory() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/npm/lockfile-workspace");
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"pnpm-web",
		fixture_root.join("packages/web/package.json"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert_eq!(
		lockfiles.first(),
		Some(&monochange_core::normalize_path(
			&fixture_root.join("pnpm-lock.yaml")
		))
	);
}

#[test]
fn discover_lockfiles_falls_back_to_manifest_directory() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/manifest-lockfile-workspace");
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"nested-web",
		fixture_root.join("packages/web/package.json"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert_eq!(
		lockfiles.first(),
		Some(&monochange_core::normalize_path(
			&fixture_root.join("packages/web/package-lock.json")
		))
	);
}

#[test]
fn update_json_dependency_fields_only_changes_declared_dependencies() {
	let mut manifest = json!({
		"dependencies": {
			"core": "^1.0.0",
			"left-pad": "1.3.0"
		},
		"devDependencies": {
			"core": "^1.0.0"
		}
	});
	let versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_json_dependency_fields(
		&mut manifest,
		&["dependencies", "devDependencies"],
		&versions,
	);

	assert_eq!(
		manifest.pointer("/dependencies/core"),
		Some(&json!("2.0.0"))
	);
	assert_eq!(
		manifest.pointer("/dependencies/left-pad"),
		Some(&json!("1.3.0"))
	);
	assert_eq!(
		manifest.pointer("/devDependencies/core"),
		Some(&json!("2.0.0"))
	);
}

#[test]
fn update_package_lock_updates_root_packages_and_dependencies() {
	let mut lock = json!({
		"name": "app",
		"version": "1.0.0",
		"packages": {
			"": {
				"name": "app",
				"version": "1.0.0"
			},
			"packages/core": {
				"name": "core",
				"version": "1.0.0"
			},
			"packages/util": {
				"version": "1.0.0"
			}
		},
		"dependencies": {
			"core": {
				"version": "1.0.0"
			}
		}
	});
	let package_paths = BTreeMap::from([
		("util".to_string(), PathBuf::from("packages/util")),
		("core".to_string(), PathBuf::from("packages/core")),
	]);
	let raw_versions = BTreeMap::from([
		("app".to_string(), "2.0.0".to_string()),
		("core".to_string(), "2.1.0".to_string()),
		("util".to_string(), "3.0.0".to_string()),
	]);

	update_package_lock(&mut lock, &package_paths, &raw_versions);

	assert_eq!(lock.pointer("/version"), Some(&json!("2.0.0")));
	assert_eq!(lock.pointer("/packages//version"), Some(&json!("2.0.0")));
	assert_eq!(
		lock.pointer("/packages/packages~1core/version"),
		Some(&json!("2.1.0"))
	);
	assert_eq!(
		lock.pointer("/packages/packages~1util/version"),
		Some(&json!("3.0.0"))
	);
	assert_eq!(
		lock.pointer("/dependencies/core/version"),
		Some(&json!("2.1.0"))
	);
}

#[test]
fn update_pnpm_lock_skips_link_and_workspace_dependencies() {
	let mut lock: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(
		r"
importers:
  .:
    dependencies:
      core: 1.0.0
      linked: link:../linked
      workspace_dep: workspace:*
packages:
  core@1.0.0:
    dependencies:
      core: 1.0.0
snapshots:
  core@1.0.0:
    dependencies:
      core:
        version: 1.0.0
      linked:
        version: link:../linked
",
	)
	.unwrap_or_else(|error| panic!("pnpm lock yaml: {error}"));
	let raw_versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_pnpm_lock(&mut lock, &raw_versions);

	let rendered = serde_yaml_ng::to_string(&YamlValue::Mapping(lock))
		.unwrap_or_else(|error| panic!("render pnpm lock: {error}"));
	assert!(rendered.contains("core: 2.0.0"));
	assert!(rendered.contains("linked: link:../linked"));
	assert!(rendered.contains("workspace_dep: workspace:*"));
}

#[test]
fn update_bun_lock_rewrites_matching_versions() {
	let updated = update_bun_lock(
		"{\n  \"core\": \"1.0.0\",\n  \"other\": \"0.1.0\"\n}",
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	);
	assert!(updated.contains("\"core\": \"2.0.0\""));
	assert!(updated.contains("\"other\": \"0.1.0\""));
}

#[test]
fn update_bun_lock_binary_rewrites_all_occurrences() {
	let updated = update_bun_lock_binary(
		b"core@1.0.0\0core@1.0.0\0",
		&BTreeMap::from([("core".to_string(), "1.0.0".to_string())]),
		&BTreeMap::from([("core".to_string(), "2.1.0".to_string())]),
	);
	let rendered = String::from_utf8(updated).unwrap_or_else(|error| panic!("utf8: {error}"));
	assert_eq!(rendered.matches("2.1.0").count(), 2);
}

#[test]
fn adapter_discover_matches_direct_npm_discovery() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace");
	let from_adapter = adapter()
		.discover(&fixture_root)
		.unwrap_or_else(|error| panic!("adapter discovery: {error}"));
	let direct = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("direct discovery: {error}"));
	assert_eq!(from_adapter.packages, direct.packages);
	assert_eq!(from_adapter.warnings, direct.warnings);
}

#[test]
fn discovers_object_style_package_json_workspaces_and_warnings() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/workspace-object-patterns");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));
	assert_eq!(discovery.packages.len(), 3);
	assert!(discovery
		.warnings
		.iter()
		.any(|warning| warning.contains("missing/*") && warning.contains("matched no packages")));
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.name == "root-workspace"));
	let private_package = discovery
		.packages
		.iter()
		.find(|package| package.name == "object-private")
		.unwrap_or_else(|| panic!("expected object-private package"));
	assert_eq!(private_package.publish_state, PublishState::Private);
	let web_package = discovery
		.packages
		.iter()
		.find(|package| package.name == "object-web")
		.unwrap_or_else(|| panic!("expected object-web package"));
	assert_eq!(
		web_package.metadata.get("manager").map(String::as_str),
		Some("npm")
	);
}

#[test]
fn discover_standalone_package_defaults_manager_to_npm() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/npm/standalone-package");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));
	assert_eq!(discovery.warnings, Vec::<String>::new());
	assert_eq!(discovery.packages.len(), 1);
	let package = discovery
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected standalone package"));
	assert_eq!(package.name, "standalone-app");
	assert_eq!(
		package.metadata.get("manager").map(String::as_str),
		Some("npm")
	);
}

#[test]
fn update_json_dependency_fields_ignores_missing_or_non_object_sections() {
	let mut manifest = json!({
		"dependencies": "not-an-object",
		"scripts": {
			"build": "vite build"
		}
	});
	let versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_json_dependency_fields(
		&mut manifest,
		&["dependencies", "devDependencies"],
		&versions,
	);

	assert_eq!(manifest.get("dependencies"), Some(&json!("not-an-object")));
	assert_eq!(
		manifest.get("scripts"),
		Some(&json!({"build": "vite build"}))
	);
}

#[test]
fn workspace_pattern_helpers_cover_array_object_and_missing_cases() {
	assert_eq!(
		workspace_patterns_from_package_json(&json!({"workspaces": ["packages/*"]})),
		vec!["packages/*".to_string()]
	);
	assert_eq!(
		workspace_patterns_from_package_json(&json!({"workspaces": {"packages": ["apps/*"]}})),
		vec!["apps/*".to_string()]
	);
	assert_eq!(
		workspace_patterns_from_package_json(&json!({})),
		Vec::<String>::new()
	);
}

#[test]
fn detect_npm_manager_prefers_bun_then_pnpm_then_npm() {
	let bun_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace-bun");
	assert_eq!(detect_npm_manager(&bun_root), "bun");
	let pnpm_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace-pnpm");
	assert_eq!(detect_npm_manager(&pnpm_root), "pnpm");
	let npm_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/npm/standalone-package");
	assert_eq!(detect_npm_manager(&npm_root), "npm");
}

#[test]
fn explicit_file_workspace_patterns_discover_package_manifests() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/workspace-explicit-file");
	let mut warnings = Vec::new();
	let manifests = expand_member_patterns(
		&fixture_root,
		&["packages/web/package.json".to_string()],
		&mut warnings,
	);
	assert_eq!(warnings, Vec::<String>::new());
	assert_eq!(manifests.len(), 1);
	assert!(manifests
		.iter()
		.any(|path| path.ends_with("packages/web/package.json")));

	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));
	assert_eq!(discovery.packages.len(), 1);
	assert_eq!(
		discovery
			.packages
			.first()
			.unwrap_or_else(|| panic!("expected explicit package"))
			.name,
		"explicit-web"
	);
}

#[test]
fn package_json_parsing_and_workspace_detection_report_parse_errors() {
	let invalid_workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join(
		"../../fixtures/tests/npm/invalid-workspace-package-json/invalid-workspace-package.json",
	);
	let error = package_json_declares_workspaces(&invalid_workspace)
		.err()
		.unwrap_or_else(|| panic!("expected invalid workspace parse error"));
	assert!(error.to_string().contains("failed to parse"));

	let workspace_error = discover_package_json_workspace(&invalid_workspace)
		.err()
		.unwrap_or_else(|| panic!("expected workspace discovery error"));
	assert!(workspace_error.to_string().contains("failed to parse"));

	let invalid_pnpm = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/invalid-pnpm-workspace/invalid-pnpm-workspace.yaml");
	let pnpm_error = discover_pnpm_workspace(&invalid_pnpm)
		.err()
		.unwrap_or_else(|| panic!("expected pnpm parse error"));
	assert!(pnpm_error.to_string().contains("failed to parse"));

	let invalid_package = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/invalid-package-json/invalid-package.json");
	let package_error = parse_package_json(&invalid_package, Path::new("."), "npm")
		.err()
		.unwrap_or_else(|| panic!("expected package parse error"));
	assert!(package_error.to_string().contains("failed to parse"));
}

#[test]
fn update_package_lock_ignores_unmapped_root_and_non_object_entries() {
	let mut lock = json!({
		"name": "app",
		"version": "1.0.0",
		"packages": {
			"": {"name": "app", "version": "1.0.0"},
			"packages/core": "not-an-object",
			"packages/util": {"version": "1.0.0"}
		},
		"dependencies": {
			"core": "1.0.0",
			"util": {"version": "1.0.0"}
		}
	});
	let package_paths = BTreeMap::from([("util".to_string(), PathBuf::from("packages/util"))]);
	let raw_versions = BTreeMap::from([("util".to_string(), "2.0.0".to_string())]);

	update_package_lock(&mut lock, &package_paths, &raw_versions);

	assert_eq!(lock.pointer("/version"), Some(&json!("1.0.0")));
	assert_eq!(lock.pointer("/packages//version"), Some(&json!("1.0.0")));
	assert_eq!(
		lock.pointer("/packages/packages~1core"),
		Some(&json!("not-an-object"))
	);
	assert_eq!(
		lock.pointer("/packages/packages~1util/version"),
		Some(&json!("2.0.0"))
	);
	assert_eq!(lock.pointer("/dependencies/core"), Some(&json!("1.0.0")));
	assert_eq!(
		lock.pointer("/dependencies/util/version"),
		Some(&json!("2.0.0"))
	);
}

#[test]
fn update_pnpm_lock_covers_missing_sections_and_non_string_versions() {
	let mut lock: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(
		r"
importers:
  .:
    devDependencies:
      core: 1.0.0
packages:
  ignored: plain-text
snapshots:
  core@1.0.0:
    peerDependencies:
      core:
        version: 1
      linked:
        version: link:../linked
",
	)
	.unwrap_or_else(|error| panic!("pnpm lock yaml: {error}"));
	let raw_versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_pnpm_lock(&mut lock, &raw_versions);

	let rendered = serde_yaml_ng::to_string(&YamlValue::Mapping(lock))
		.unwrap_or_else(|error| panic!("render pnpm lock: {error}"));
	assert!(rendered.contains("core: 2.0.0"));
	assert!(rendered.contains("version: 1"));
	assert!(rendered.contains("link:../linked"));
}

#[test]
fn update_bun_lock_and_binary_skip_unusable_replacements() {
	let unchanged = update_bun_lock(
		"{\n  \"core\": \"1.0.0\n}",
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	);
	assert_eq!(unchanged, "{\n  \"core\": \"1.0.0\n}");

	let binary = update_bun_lock_binary(
		b"core@1.0.0\0same@2.0.0\0empty@\0",
		&BTreeMap::from([
			("missing".to_string(), "1.0.0".to_string()),
			("same".to_string(), "2.0.0".to_string()),
			("empty".to_string(), String::new()),
		]),
		&BTreeMap::from([
			("same".to_string(), "2.0.0".to_string()),
			("empty".to_string(), "3.0.0".to_string()),
		]),
	);
	assert_eq!(binary, b"core@1.0.0\0same@2.0.0\0empty@\0");
}
