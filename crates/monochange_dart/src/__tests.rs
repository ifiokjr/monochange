use std::collections::BTreeMap;
use std::path::Path;

use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use serde_yaml_ng::Value;

use crate::DartVersionedFileKind;
use crate::adapter;
use crate::default_lockfile_commands;
use crate::discover_dart_packages;
use crate::discover_lockfiles;
use crate::discover_workspace_packages;
use crate::has_workspace_section;
use crate::parse_manifest;
use crate::parse_yaml_manifest;
use crate::supported_versioned_file_kind;
use crate::update_dependency_fields;
use crate::update_manifest_text;
use crate::update_pubspec_lock;
use crate::yaml_array_strings;
use crate::yaml_bool;
use crate::yaml_mapping;
use crate::yaml_string;

#[test]
fn discovers_dart_workspace_packages() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dart/workspace");
	let discovery = discover_dart_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("dart discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "dart_shared")
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "dart_app")
	);
}

#[test]
fn marks_flutter_packages_with_flutter_ecosystem() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/flutter/workspace");
	let discovery = discover_dart_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("flutter discovery: {error}"));

	assert!(
		discovery
			.packages
			.iter()
			.all(|package| package.ecosystem.as_str() == "flutter")
	);
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
fn discover_lockfiles_falls_back_to_manifest_directory() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/dart/manifest-lockfile-workspace");
	let package = PackageRecord::new(
		Ecosystem::Dart,
		"nested_dart_app",
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
			&fixture_root.join("packages/app/pubspec.lock")
		))
	);
}

#[test]
fn default_lockfile_commands_choose_dart_or_flutter_pub_get() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/dart/manifest-lockfile-workspace");
	let dart_package = PackageRecord::new(
		Ecosystem::Dart,
		"nested_dart_app",
		fixture_root.join("packages/app/pubspec.yaml"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert_eq!(
		default_lockfile_commands(&dart_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "dart pub get".to_string(),
			cwd: monochange_core::normalize_path(&fixture_root.join("packages/app")),
			shell: monochange_core::ShellConfig::None,
		}]
	);

	let flutter_package = PackageRecord::new(
		Ecosystem::Flutter,
		"nested_flutter_app",
		fixture_root.join("packages/app/pubspec.yaml"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert_eq!(
		default_lockfile_commands(&flutter_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "flutter pub get".to_string(),
			cwd: monochange_core::normalize_path(&fixture_root.join("packages/app")),
			shell: monochange_core::ShellConfig::None,
		}]
	);

	let cargo_package = PackageRecord::new(
		Ecosystem::Cargo,
		"not-dart",
		fixture_root.join("packages/app/pubspec.yaml"),
		fixture_root,
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert!(default_lockfile_commands(&cargo_package).is_empty());
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
fn update_manifest_text_preserves_pubspec_formatting() {
	let manifest = r"name: sample_app
version: '1.0.0' # keep quote

dependencies:
  shared:
    path: ../shared
    version: ^1.0.0
  http: ^1.0.0

dev_dependencies:
  test: ^1.0.0
";
	let updated = update_manifest_text(
		manifest,
		Some("2.0.0"),
		&["dependencies", "dev_dependencies"],
		&BTreeMap::from([
			("shared".to_string(), "^2.0.0".to_string()),
			("test".to_string(), "^2.0.0".to_string()),
		]),
	)
	.unwrap_or_else(|error| panic!("update pubspec text: {error}"));
	assert!(updated.contains("version: '2.0.0' # keep quote"));
	assert!(updated.contains("path: ../shared"));
	assert!(updated.contains("version: ^2.0.0"));
	assert!(updated.contains("test: ^2.0.0"));
	assert!(updated.contains("http: ^1.0.0"));
}

#[test]
fn yaml_helper_functions_cover_missing_and_inline_paths() {
	let contents = "version: # comment only\n\n  nested: value\nshared:\n  path: ../shared\n";
	let ranges = crate::yaml_line_ranges(contents);
	assert_eq!(ranges.len(), 6);
	assert!(crate::parse_yaml_line(contents, *ranges.get(1).expect("blank line range")).is_none());
	assert!(crate::parse_yaml_line(": nope", (0, 6)).is_none());
	assert!(crate::yaml_value_span("version: # comment", 0, 8).is_none());
	assert_eq!(crate::find_yaml_quote_end("\"1.0.0\"", '"'), Some(6));
	assert_eq!(crate::find_yaml_quote_end("\"1.0.0", '"'), None);
	assert_eq!(crate::render_yaml_scalar("\"1.0.0\"", "2.0.0"), "\"2.0.0\"");
	assert_eq!(crate::render_yaml_scalar("'1.0.0'", "2.0.0"), "'2.0.0'");
	assert_eq!(crate::render_yaml_scalar("1.0.0", "2.0.0"), "2.0.0");

	let nested = r"dependencies:
  shared:
    path: ../shared

    # keep spacing
  other: ^1.0.0
";
	let nested_ranges = crate::yaml_line_ranges(nested);
	let section_index = crate::find_yaml_key_line(nested, &nested_ranges, 0, "dependencies")
		.unwrap_or_else(|| panic!("expected dependencies section"));
	assert_eq!(
		crate::find_yaml_dependency_scalar(nested, &nested_ranges, section_index, "shared"),
		None
	);
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

#[test]
fn update_pubspec_lock_ignores_missing_package_section() {
	let mut lock: serde_yaml_ng::Mapping = serde_yaml_ng::from_str("root: true\n")
		.unwrap_or_else(|error| panic!("pubspec lock yaml: {error}"));
	update_pubspec_lock(
		&mut lock,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	);
	let rendered = serde_yaml_ng::to_string(&Value::Mapping(lock))
		.unwrap_or_else(|error| panic!("render pubspec lock: {error}"));
	assert_eq!(rendered, "root: true\n");
}

#[test]
fn workspace_and_manifest_helpers_cover_yaml_and_error_paths() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/dart/workspace-pattern-warnings");
	let workspace_manifest = fixture_root.join("pubspec.yaml");
	assert!(has_workspace_section(&workspace_manifest).unwrap());
	let parsed = parse_yaml_manifest(&workspace_manifest)
		.unwrap_or_else(|error| panic!("workspace yaml: {error}"));
	assert_eq!(
		yaml_array_strings(&parsed, "workspace"),
		vec!["packages/*".to_string(), "missing/*".to_string()]
	);
	assert_eq!(yaml_string(&parsed, "name"), None);
	assert_eq!(yaml_bool(&parsed, "publish_to"), None);
	assert_eq!(yaml_mapping(&parsed, "dependencies"), None);

	let app_manifest = fixture_root.join("packages/app/pubspec.yaml");
	let app = parse_manifest(&app_manifest, &fixture_root)
		.unwrap_or_else(|error| panic!("parse app manifest: {error}"))
		.unwrap_or_else(|| panic!("expected app package"));
	assert_eq!(app.ecosystem, Ecosystem::Dart);
	assert_eq!(app.publish_state, PublishState::Public);
	assert_eq!(
		app.current_version
			.as_ref()
			.map(ToString::to_string)
			.as_deref(),
		Some("1.2.3")
	);
	assert!(app.declared_dependencies.iter().any(|dependency| {
		dependency.name == "shared" && dependency.version_constraint.as_deref() == Some("^1.0.0")
	}));

	let private_manifest = fixture_root.join("packages/private/pubspec.yaml");
	let private = parse_manifest(&private_manifest, &fixture_root)
		.unwrap_or_else(|error| panic!("parse private manifest: {error}"))
		.unwrap_or_else(|| panic!("expected private package"));
	assert_eq!(private.ecosystem, Ecosystem::Flutter);
	assert_eq!(private.publish_state, PublishState::Private);
	assert_eq!(private.current_version, None);

	let discovery = discover_workspace_packages(&workspace_manifest)
		.unwrap_or_else(|error| panic!("workspace discovery: {error}"));
	assert_eq!(discovery.0.len(), 2);
	assert!(discovery.1.iter().any(|warning| {
		warning.contains("missing/*") && warning.contains("matched no packages")
	}));

	let nameless_manifest: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(
		r"
dependencies:
  core: ^1.0.0
",
	)
	.unwrap_or_else(|error| panic!("yaml: {error}"));
	assert_eq!(yaml_string(&nameless_manifest, "name"), None);
	assert!(yaml_mapping(&nameless_manifest, "dependencies").is_some());
	assert_eq!(yaml_bool(&nameless_manifest, "publish_to"), None);

	let invalid_workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/dart/invalid-workspace/invalid-workspace.yaml");
	let invalid_workspace_error = has_workspace_section(&invalid_workspace)
		.err()
		.unwrap_or_else(|| panic!("expected invalid workspace error"));
	assert!(
		invalid_workspace_error
			.to_string()
			.contains("failed to parse")
	);

	let invalid_package = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/dart/invalid-package/invalid-package.yaml");
	let invalid_package_error = parse_manifest(&invalid_package, Path::new("."))
		.err()
		.unwrap_or_else(|| panic!("expected invalid package error"));
	assert!(
		invalid_package_error
			.to_string()
			.contains("failed to parse")
	);
}
