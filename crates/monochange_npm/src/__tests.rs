use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::materialize_dependency_edges;
use semver::Version;
use serde_json::json;
use serde_yaml_ng::Value as YamlValue;

use crate::NpmVersionedFileKind;
use crate::adapter;
use crate::default_lockfile_commands;
use crate::detect_npm_manager;
use crate::discover_lockfiles;
use crate::discover_npm_packages;
use crate::discover_package_json_workspace;
use crate::discover_pnpm_workspace;
use crate::expand_member_patterns;
use crate::load_configured_npm_package;
use crate::package_json_declares_workspaces;
use crate::parse_package_json;
use crate::supported_versioned_file_kind;
use crate::update_bun_lock;
use crate::update_bun_lock_binary;
use crate::update_json_dependency_fields;
use crate::update_package_lock;
use crate::update_pnpm_lock;
use crate::update_pnpm_lock_text;
use crate::workspace_patterns_from_package_json;

#[test]
fn discovers_npm_workspace_packages() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "npm-web")
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "npm-shared")
	);
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
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "pnpm-web")
	);
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
fn default_lockfile_commands_match_owned_npm_lockfile_kind() {
	let package_lock_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/manifest-lockfile-workspace");
	let package_lock_package = PackageRecord::new(
		Ecosystem::Npm,
		"nested-web",
		package_lock_root.join("packages/web/package.json"),
		package_lock_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert_eq!(
		default_lockfile_commands(&package_lock_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "npm install --package-lock-only".to_string(),
			cwd: monochange_core::normalize_path(&package_lock_root.join("packages/web")),
			shell: monochange_core::ShellConfig::None,
		}]
	);

	let pnpm_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/npm/lockfile-workspace");
	let pnpm_package = PackageRecord::new(
		Ecosystem::Npm,
		"nested-web",
		pnpm_root.join("packages/web/package.json"),
		pnpm_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert_eq!(
		default_lockfile_commands(&pnpm_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "pnpm install --lockfile-only".to_string(),
			cwd: monochange_core::normalize_path(&pnpm_root),
			shell: monochange_core::ShellConfig::None,
		}]
	);

	let bun_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/monochange/bun-lock-release");
	let bun_package = PackageRecord::new(
		Ecosystem::Npm,
		"workflow-app",
		bun_root.join("packages/app/package.json"),
		bun_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert_eq!(
		default_lockfile_commands(&bun_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "bun install --lockfile-only".to_string(),
			cwd: monochange_core::normalize_path(&bun_root.join("packages/app")),
			shell: monochange_core::ShellConfig::None,
		}]
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
fn update_pnpm_lock_text_preserves_existing_formatting() {
	let lock = r#"lockfileVersion: "9.0"

settings:
  autoInstallPeers: true
  excludeLinksFromLockfile: false

importers:
  .:
    dependencies:
      core: 1.0.0
      linked: link:../linked

  npm/skill: {}
"#;
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert_eq!(
		updated,
		r#"lockfileVersion: "9.0"

settings:
  autoInstallPeers: true
  excludeLinksFromLockfile: false

importers:
  .:
    dependencies:
      core: 2.0.0
      linked: link:../linked

  npm/skill: {}
"#
	);
}

#[test]
fn update_pnpm_lock_text_returns_original_contents_when_no_entries_match() {
	let lock = "lockfileVersion: \"9.0\"\n\nimporters:\n  .: {}\n";
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert_eq!(updated, lock);
}

#[test]
fn pnpm_text_helper_functions_cover_edge_cases() {
	let contents = "importers:\n\n  # comment\n";
	let ranges = crate::yaml_line_ranges(contents);
	assert_eq!(
		crate::find_yaml_key_line(contents, &ranges, 0, "importers"),
		Some(0)
	);
	assert!(crate::parse_yaml_line(contents, *ranges.get(1).expect("blank line range")).is_none());
	assert!(crate::parse_yaml_line(": nope", (0, 6)).is_none());
	let mut replacements = Vec::new();
	crate::collect_pnpm_section_replacements(
		contents,
		&ranges,
		1,
		&BTreeMap::new(),
		&mut replacements,
	);
	let outer_blank = "importers:\n\n  .:\n";
	let outer_blank_ranges = crate::yaml_line_ranges(outer_blank);
	let outer_blank_index =
		crate::find_yaml_key_line(outer_blank, &outer_blank_ranges, 0, "importers")
			.unwrap_or_else(|| panic!("expected importers section"));
	crate::collect_pnpm_section_replacements(
		outer_blank,
		&outer_blank_ranges,
		outer_blank_index,
		&BTreeMap::new(),
		&mut replacements,
	);
	crate::collect_pnpm_dependency_replacements(
		contents,
		&ranges,
		1,
		&BTreeMap::new(),
		&mut replacements,
	);
	assert!(replacements.is_empty());
	assert!(crate::yaml_value_span("version: # comment", 0, 8).is_none());
	assert_eq!(crate::find_yaml_quote_end("\"1.0.0\"", '"'), Some(6));
	assert_eq!(crate::find_yaml_quote_end("\"1.0.0", '"'), None);
	assert_eq!(crate::render_yaml_scalar("\"1.0.0\"", "2.0.0"), "\"2.0.0\"");
	assert_eq!(crate::render_yaml_scalar("'1.0.0'", "2.0.0"), "'2.0.0'");
	assert_eq!(crate::render_yaml_scalar("1.0.0", "2.0.0"), "2.0.0");
	assert!(crate::yaml_scalar_is_updatable("1.0.0"));
	assert!(!crate::yaml_scalar_is_updatable("1"));
	assert!(!crate::yaml_scalar_is_updatable("link:../linked"));
	assert!(crate::is_pnpm_dependency_field("dependencies"));
	assert!(!crate::is_pnpm_dependency_field("resolution"));
}

#[test]
fn update_pnpm_lock_text_updates_nested_versions_and_preserves_quotes() {
	let lock = r#"lockfileVersion: '9.0'

importers:
  .:
    dependencies:
      core:
        version: "1.0.0"
      linked:
        version: link:../linked
      numeric:
        version: 1
      missing:
        path: ../missing

snapshots:
  core@1.0.0:
    optionalDependencies:
      core: '1.0.0'
"#;
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert!(updated.contains("version: \"2.0.0\""));
	assert!(updated.contains("core: '2.0.0'"));
	assert!(updated.contains("version: link:../linked"));
	assert!(updated.contains("version: 1"));
	assert!(updated.contains("path: ../missing"));
}

#[test]
fn pnpm_replacement_helpers_skip_invalid_spans_and_blank_lines() {
	let mut replacements = Vec::new();
	crate::push_pnpm_scalar_replacement("link:../linked", (0, 14), "2.0.0", &mut replacements);
	crate::push_pnpm_scalar_replacement("1.0.0", (0, 99), "2.0.0", &mut replacements);
	assert!(replacements.is_empty());

	let contents = r"importers:
  .:

    dependencies:
      other: 1.0.0
      core:
        path: ../core

      next: 1.0.0
";
	let ranges = crate::yaml_line_ranges(contents);
	let section_index = crate::find_yaml_key_line(contents, &ranges, 0, "importers")
		.unwrap_or_else(|| panic!("expected importers section"));
	crate::collect_pnpm_section_replacements(
		contents,
		&ranges,
		section_index,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&mut replacements,
	);
	assert!(replacements.is_empty());
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
	assert!(discovery.warnings.iter().any(|warning| {
		warning.contains("missing/*") && warning.contains("matched no packages")
	}));
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "root-workspace")
	);
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
fn discover_multiple_standalone_packages_keep_unique_manifest_ids() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/standalone-multiple-packages");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));
	assert_eq!(discovery.warnings, Vec::<String>::new());
	assert_eq!(discovery.packages.len(), 2);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.id == "npm:packages/docs/package.json")
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.id == "npm:packages/web/package.json")
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "standalone-docs")
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "standalone-web")
	);
}

#[test]
fn load_configured_npm_package_normalizes_ids_relative_to_root() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/standalone-multiple-packages");
	let package = load_configured_npm_package(&fixture_root, &fixture_root.join("packages/docs"))
		.unwrap_or_else(|error| panic!("configured npm package: {error}"))
		.unwrap_or_else(|| panic!("expected configured npm package"));
	assert_eq!(package.id, "npm:packages/docs/package.json");
	assert_eq!(package.name, "standalone-docs");
}

#[test]
fn normalize_package_id_leaves_existing_id_when_manifest_is_outside_root() {
	let mut package = PackageRecord::new(
		Ecosystem::Npm,
		"standalone-docs",
		PathBuf::from("/tmp/outside-root/package.json"),
		PathBuf::from("/tmp/outside-root"),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let original_id = package.id.clone();
	super::normalize_package_id(Path::new("/tmp/workspace-root"), &mut package);
	assert_eq!(package.id, original_id);
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
	assert!(
		manifests
			.iter()
			.any(|path| path.ends_with("packages/web/package.json"))
	);

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
