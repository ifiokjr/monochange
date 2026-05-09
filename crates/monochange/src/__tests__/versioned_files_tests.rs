use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::EcosystemType;

use super::CachedDocument;
use super::VersionedFileUpdateContext;
use super::apply_versioned_file_definition;
use super::inferred_lockfile_ecosystem_type;
use super::inferred_lockfile_paths;
use super::read_cached_document;
use super::versioned_file_kind;

fn fixture_path(relative: &str) -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

#[test]
fn go_versioned_file_kind_and_lockfile_inference_are_supported() {
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let module_dir = tempdir.path().join("api");
	std::fs::create_dir(&module_dir)
		.unwrap_or_else(|error| panic!("create api module dir: {error}"));
	std::fs::write(module_dir.join("go.sum"), "")
		.unwrap_or_else(|error| panic!("write go.sum: {error}"));
	let package = monochange_core::PackageRecord {
		ecosystem: Ecosystem::Go,
		manifest_path: module_dir.join("go.mod"),
		..monochange_core::PackageRecord::new(
			Ecosystem::Go,
			"github.com/example/repo/api",
			module_dir.join("go.mod"),
			tempdir.path().to_path_buf(),
			None,
			monochange_core::PublishState::Public,
		)
	};

	assert!(versioned_file_kind(EcosystemType::Go, Path::new("go.mod")).is_some());
	assert_eq!(
		inferred_lockfile_ecosystem_type(&configuration, Ecosystem::Go),
		Some(EcosystemType::Go)
	);
	assert_eq!(
		inferred_lockfile_paths(&package),
		vec![module_dir.join("go.sum")]
	);
}

#[test]
fn read_cached_document_handles_go_text_and_invalid_utf8() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let go_mod = tempdir.path().join("go.mod");
	std::fs::write(&go_mod, "module github.com/example/repo\n")
		.unwrap_or_else(|error| panic!("write go.mod: {error}"));
	let mut updates = BTreeMap::new();

	let document = read_cached_document(&mut updates, &go_mod, EcosystemType::Go)
		.unwrap_or_else(|error| panic!("go text document: {error}"));
	assert!(matches!(document, CachedDocument::Text(_)));

	std::fs::write(&go_mod, [0xff, 0xfe])
		.unwrap_or_else(|error| panic!("write invalid go.mod: {error}"));
	let error = read_cached_document(&mut updates, &go_mod, EcosystemType::Go)
		.expect_err("invalid go.mod should fail");
	assert!(error.to_string().contains("failed to parse"));
}

#[test]
fn read_cached_document_reports_go_for_unsupported_go_versioned_file() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let notes = tempdir.path().join("notes.txt");
	std::fs::write(&notes, "version = 1.0.0\n")
		.unwrap_or_else(|error| panic!("write notes: {error}"));
	let mut updates = BTreeMap::new();

	let error = read_cached_document(&mut updates, &notes, EcosystemType::Go)
		.expect_err("unsupported go versioned file");

	assert!(error.to_string().contains("ecosystem `go`"));
}

#[test]
fn apply_versioned_file_definition_reports_go_for_unsupported_glob_match() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::write(tempdir.path().join("notes.txt"), "version = 1.0.0\n")
		.unwrap_or_else(|error| panic!("write notes: {error}"));
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	let mut released_versions = BTreeMap::new();
	released_versions.insert("lib".to_string(), "1.2.3".to_string());
	let context = VersionedFileUpdateContext {
		package_by_config_id: BTreeMap::new(),
		package_by_native_name: BTreeMap::new(),
		current_versions_by_native_name: BTreeMap::new(),
		released_versions_by_native_name: released_versions,
		configuration: &configuration,
	};
	let definition = monochange_core::VersionedFileDefinition {
		path: "*.txt".to_string(),
		ecosystem_type: Some(EcosystemType::Go),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let mut updates = BTreeMap::new();

	let error = apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"1.2.3",
		None,
		&["lib".to_string()],
		&context,
	)
	.expect_err("unsupported go glob match");

	assert!(error.to_string().contains("ecosystem `go`"));
}

#[test]
fn apply_versioned_file_definition_updates_go_mod_dependencies() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let go_mod = tempdir.path().join("go.mod");
	std::fs::write(
		&go_mod,
		"module github.com/example/app\n\ngo 1.22\n\nrequire github.com/example/lib v1.0.0\n",
	)
	.unwrap_or_else(|error| panic!("write go.mod: {error}"));
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	let mut released_versions = BTreeMap::new();
	released_versions.insert("lib".to_string(), "1.2.3".to_string());
	let context = VersionedFileUpdateContext {
		package_by_config_id: BTreeMap::new(),
		package_by_native_name: BTreeMap::new(),
		current_versions_by_native_name: BTreeMap::new(),
		released_versions_by_native_name: released_versions,
		configuration: &configuration,
	};
	let definition = monochange_core::VersionedFileDefinition {
		path: "go.mod".to_string(),
		ecosystem_type: Some(EcosystemType::Go),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let mut updates = BTreeMap::new();

	apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"1.2.3",
		None,
		&["lib".to_string()],
		&context,
	)
	.unwrap_or_else(|error| panic!("apply go update: {error}"));
	let updated_document = updates
		.into_values()
		.next()
		.unwrap_or_else(|| panic!("updated go.mod"));
	assert!(matches!(
		updated_document,
		CachedDocument::Text(contents) if contents.contains("github.com/example/lib v1.2.3")
	));
}

#[test]
fn inferred_lockfile_ecosystem_type_maps_python_when_commands_are_not_configured() {
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));

	assert_eq!(
		inferred_lockfile_ecosystem_type(&configuration, Ecosystem::Python),
		Some(EcosystemType::Python)
	);
}
