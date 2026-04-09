use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::materialize_dependency_edges;
use monochange_core::ChangeSignal;
use monochange_core::CompatibilityAssessment;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_semver::CompatibilityProvider;
use tempfile::tempdir;
use toml::Value;

use crate::adapter;
use crate::dependency_constraint;
use crate::discover_cargo_packages;
use crate::discover_lockfiles;
use crate::discover_workspace_packages;
use crate::has_workspace_section;
use crate::parse_package_manifest;
use crate::parse_package_version;
use crate::supported_versioned_file_kind;
use crate::update_versioned_file_text;
use crate::validate_workspace_version_groups;
use crate::workspace_package_version;
use crate::CargoVersionedFileKind;
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
fn validate_workspace_version_groups_rejects_mismatched_workspace_version_groups() {
	let workspace_root = PathBuf::from("/tmp/workspace");
	let mut core = PackageRecord::new(
		Ecosystem::Cargo,
		"workspace-core",
		workspace_root.join("crates/core/Cargo.toml"),
		workspace_root.clone(),
		None,
		PublishState::Public,
	);
	core.metadata
		.insert("config_id".to_string(), "core".to_string());
	core.metadata
		.insert("uses_workspace_version".to_string(), "true".to_string());
	core.version_group_id = Some("sdk".to_string());

	let mut app = PackageRecord::new(
		Ecosystem::Cargo,
		"workspace-app",
		workspace_root.join("crates/app/Cargo.toml"),
		workspace_root,
		None,
		PublishState::Public,
	);
	app.metadata
		.insert("config_id".to_string(), "app".to_string());
	app.metadata
		.insert("uses_workspace_version".to_string(), "true".to_string());

	let error = validate_workspace_version_groups(&[core, app])
		.err()
		.unwrap_or_else(|| panic!("expected validation error"));
	assert!(error.to_string().contains(
		"cargo packages using `version.workspace = true` must belong to the same version group"
	));
}

#[test]
fn validate_workspace_version_groups_accepts_matching_workspace_version_groups() {
	let workspace_root = PathBuf::from("/tmp/workspace");
	let mut core = PackageRecord::new(
		Ecosystem::Cargo,
		"workspace-core",
		workspace_root.join("crates/core/Cargo.toml"),
		workspace_root.clone(),
		None,
		PublishState::Public,
	);
	core.metadata
		.insert("config_id".to_string(), "core".to_string());
	core.metadata
		.insert("uses_workspace_version".to_string(), "true".to_string());
	core.version_group_id = Some("sdk".to_string());

	let mut app = PackageRecord::new(
		Ecosystem::Cargo,
		"workspace-app",
		workspace_root.join("crates/app/Cargo.toml"),
		workspace_root,
		None,
		PublishState::Public,
	);
	app.metadata
		.insert("config_id".to_string(), "app".to_string());
	app.metadata
		.insert("uses_workspace_version".to_string(), "true".to_string());
	app.version_group_id = Some("sdk".to_string());

	validate_workspace_version_groups(&[core, app])
		.unwrap_or_else(|error| panic!("validation: {error}"));
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

#[test]
fn adapter_reports_cargo_ecosystem() {
	assert_eq!(adapter().ecosystem(), Ecosystem::Cargo);
}

#[test]
fn supported_versioned_file_kind_recognizes_manifest_and_lockfiles() {
	assert_eq!(
		supported_versioned_file_kind(Path::new("Cargo.toml")),
		Some(CargoVersionedFileKind::Manifest)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("Cargo.lock")),
		Some(CargoVersionedFileKind::Lock)
	);
	assert_eq!(supported_versioned_file_kind(Path::new("README.md")), None);
}

#[test]
fn discover_lockfiles_prefers_workspace_root_then_manifest_directory() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/monochange/cargo-lock-release");
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"workflow-core",
		fixture_root.join("crates/core/Cargo.toml"),
		fixture_root.clone(),
		None,
		PublishState::Public,
	);
	let lockfiles = discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert_eq!(
		lockfiles.first(),
		Some(&monochange_core::normalize_path(
			&fixture_root.join("Cargo.lock")
		))
	);
}

#[test]
fn discover_lockfiles_falls_back_to_manifest_directory() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo/manifest-lockfile-workspace");
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"lockfile-core",
		fixture_root.join("crates/core/Cargo.toml"),
		fixture_root.clone(),
		None,
		PublishState::Public,
	);
	let lockfiles = discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert_eq!(
		lockfiles.first(),
		Some(&monochange_core::normalize_path(
			&fixture_root.join("crates/core/Cargo.lock")
		))
	);
}

#[test]
fn update_versioned_file_updates_manifest_and_workspace_dependencies() {
	let manifest = r#"
[package]
name = "app"
version = "1.0.0"

[dependencies]
core = "1.0.0"
shared = { workspace = true }

[workspace.package]
version = "1.0.0"

[workspace.dependencies]
core = { version = "1.0.0" }
"#;
	let versioned_deps = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);
	let raw_versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);
	let updated = update_versioned_file_text(
		manifest,
		CargoVersionedFileKind::Manifest,
		&["dependencies"],
		Some("2.0.0"),
		Some("3.0.0"),
		&versioned_deps,
		&raw_versions,
	)
	.unwrap_or_else(|error| panic!("update manifest: {error}"));
	let manifest: Value =
		toml::from_str(&updated).unwrap_or_else(|error| panic!("manifest toml: {error}"));

	assert_eq!(
		manifest
			.get("package")
			.and_then(Value::as_table)
			.and_then(|table| table.get("version"))
			.and_then(Value::as_str),
		Some("2.0.0")
	);
	assert_eq!(
		manifest
			.get("dependencies")
			.and_then(Value::as_table)
			.and_then(|table| table.get("core"))
			.and_then(Value::as_str),
		Some("2.0.0")
	);
	assert_eq!(
		manifest
			.get("workspace")
			.and_then(Value::as_table)
			.and_then(|table| table.get("package"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("version"))
			.and_then(Value::as_str),
		Some("3.0.0")
	);
	assert_eq!(
		manifest
			.get("workspace")
			.and_then(Value::as_table)
			.and_then(|table| table.get("dependencies"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("core"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("version"))
			.and_then(Value::as_str),
		Some("2.0.0")
	);
	assert_eq!(
		manifest
			.get("dependencies")
			.and_then(Value::as_table)
			.and_then(|table| table.get("shared"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("workspace"))
			.and_then(Value::as_bool),
		Some(true)
	);
}

#[test]
fn update_versioned_file_updates_lock_packages() {
	let lock = r#"
[[package]]
name = "core"
version = "1.0.0"

[[package]]
name = "app"
version = "1.0.0"
"#;
	let raw_versions = BTreeMap::from([
		("core".to_string(), "2.0.0".to_string()),
		("app".to_string(), "1.1.0".to_string()),
	]);

	let updated = update_versioned_file_text(
		lock,
		CargoVersionedFileKind::Lock,
		&[],
		None,
		None,
		&BTreeMap::new(),
		&raw_versions,
	)
	.unwrap_or_else(|error| panic!("update lock: {error}"));
	let lock: Value = toml::from_str(&updated).unwrap_or_else(|error| panic!("lock toml: {error}"));

	let packages = lock
		.get("package")
		.and_then(Value::as_array)
		.unwrap_or_else(|| panic!("expected package array"));
	assert!(packages.iter().any(|package| {
		package["name"].as_str() == Some("core") && package["version"].as_str() == Some("2.0.0")
	}));
	assert!(packages.iter().any(|package| {
		package["name"].as_str() == Some("app") && package["version"].as_str() == Some("1.1.0")
	}));
}

#[test]
fn update_versioned_file_covers_workspace_owned_and_unstructured_entries() {
	let manifest = r#"
[package]
name = "app"
version = { workspace = true }

[dependencies]
core = { workspace = true }
serde = { version = "1.0.0", optional = true }
weird = true

[workspace.package]
version = "1.0.0"

[workspace.dependencies]
core = "1.0.0"
serde = { version = "1.0.0" }
"#;
	let versioned_deps = BTreeMap::from([
		("core".to_string(), "2.0.0".to_string()),
		("serde".to_string(), "1.1.0".to_string()),
	]);

	let updated = update_versioned_file_text(
		manifest,
		CargoVersionedFileKind::Manifest,
		&["dependencies", "dev-dependencies"],
		Some("9.9.9"),
		None,
		&versioned_deps,
		&BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("update manifest: {error}"));
	let manifest: Value =
		toml::from_str(&updated).unwrap_or_else(|error| panic!("manifest toml: {error}"));

	assert_eq!(
		manifest
			.get("package")
			.and_then(Value::as_table)
			.and_then(|table| table.get("version"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("workspace"))
			.and_then(Value::as_bool),
		Some(true)
	);
	assert_eq!(
		manifest
			.get("dependencies")
			.and_then(Value::as_table)
			.and_then(|table| table.get("core"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("workspace"))
			.and_then(Value::as_bool),
		Some(true)
	);
	assert_eq!(
		manifest
			.get("dependencies")
			.and_then(Value::as_table)
			.and_then(|table| table.get("serde"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("version"))
			.and_then(Value::as_str),
		Some("1.1.0")
	);
	assert_eq!(
		manifest
			.get("dependencies")
			.and_then(Value::as_table)
			.and_then(|table| table.get("weird"))
			.and_then(Value::as_bool),
		Some(true)
	);
	assert_eq!(
		manifest
			.get("workspace")
			.and_then(Value::as_table)
			.and_then(|table| table.get("package"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("version"))
			.and_then(Value::as_str),
		Some("1.0.0")
	);
	assert_eq!(
		manifest
			.get("workspace")
			.and_then(Value::as_table)
			.and_then(|table| table.get("dependencies"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("core"))
			.and_then(Value::as_str),
		Some("2.0.0")
	);
	assert_eq!(
		manifest
			.get("workspace")
			.and_then(Value::as_table)
			.and_then(|table| table.get("dependencies"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("serde"))
			.and_then(Value::as_table)
			.and_then(|table| table.get("version"))
			.and_then(Value::as_str),
		Some("1.1.0")
	);

	let lock = r#"
[[package]]
version = "1.0.0"

[[package]]
name = "core"
version = "1.0.0"

[[package]]
name = "ignored"
version = "0.1.0"
"#;
	let updated = update_versioned_file_text(
		lock,
		CargoVersionedFileKind::Lock,
		&[],
		None,
		None,
		&BTreeMap::new(),
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update lock: {error}"));
	let lock: Value = toml::from_str(&updated).unwrap_or_else(|error| panic!("lock toml: {error}"));
	let packages = lock
		.get("package")
		.and_then(Value::as_array)
		.unwrap_or_else(|| panic!("expected package array"));
	assert!(packages.iter().any(|package| {
		package.get("name").and_then(Value::as_str) == Some("core")
			&& package.get("version").and_then(Value::as_str) == Some("2.0.0")
	}));
	assert!(packages.iter().any(|package| {
		package.get("name").and_then(Value::as_str) == Some("ignored")
			&& package.get("version").and_then(Value::as_str) == Some("0.1.0")
	}));
}

#[test]
fn adapter_discover_matches_direct_cargo_discovery() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cargo/workspace");
	let from_adapter = adapter()
		.discover(&fixture_root)
		.unwrap_or_else(|error| panic!("adapter discovery: {error}"));
	let direct = discover_cargo_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("direct discovery: {error}"));
	assert_eq!(from_adapter.packages, direct.packages);
	assert_eq!(from_adapter.warnings, direct.warnings);
}

#[test]
fn discover_cargo_packages_reports_workspace_warnings_and_private_packages() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo/workspace-pattern-warnings");
	let discovery = discover_cargo_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("cargo discovery: {error}"));

	assert_eq!(discovery.packages.len(), 3);
	assert!(discovery
		.warnings
		.iter()
		.any(|warning| warning.contains("missing/*") && warning.contains("matched no packages")));
	let excluded_package = discovery
		.packages
		.iter()
		.find(|package| package.name == "warning-excluded")
		.unwrap_or_else(|| panic!("expected excluded package to be discovered standalone"));
	assert_eq!(
		excluded_package.workspace_root,
		monochange_core::normalize_path(&fixture_root.join("crates/excluded"))
	);
	let private_package = discovery
		.packages
		.iter()
		.find(|package| package.name == "warning-private")
		.unwrap_or_else(|| panic!("expected private package"));
	assert_eq!(private_package.publish_state, PublishState::Private);
	let workspace_versioned = discovery
		.packages
		.iter()
		.find(|package| package.name == "warning-core")
		.unwrap_or_else(|| panic!("expected warning-core package"));
	assert_eq!(
		workspace_versioned
			.current_version
			.as_ref()
			.map(ToString::to_string)
			.as_deref(),
		Some("3.2.1")
	);
	assert_eq!(
		workspace_versioned
			.metadata
			.get("uses_workspace_version")
			.map(String::as_str),
		Some("true")
	);
}

#[test]
fn parse_package_version_prefers_workspace_version_when_requested() {
	let workspace_version = semver::Version::new(2, 3, 4);
	let version = parse_package_version(
		&toml::from_str::<Value>("workspace = true")
			.unwrap_or_else(|error| panic!("workspace version toml: {error}")),
		Some(&workspace_version),
	);
	assert_eq!(version, Some(workspace_version));
}

#[test]
fn dependency_constraint_supports_strings_and_tables() {
	assert_eq!(
		dependency_constraint(&Value::String("^1.2.3".to_string())),
		Some("^1.2.3".to_string())
	);
	let table_value = toml::from_str::<Value>("version = \"~2.0\"")
		.unwrap_or_else(|error| panic!("dependency table toml: {error}"));
	assert_eq!(
		dependency_constraint(&table_value),
		Some("~2.0".to_string())
	);
	assert_eq!(dependency_constraint(&Value::Boolean(true)), None);
}

#[test]
fn rust_semver_provider_defaults_unknown_severity_to_none() {
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
		evidence_refs: vec!["cargo-semver:unexpected:manual review needed".to_string()],
		notes: None,
		details: None,
		change_type: None,
		source_path: PathBuf::from(".changeset/feature.md"),
	};
	let assessment = RustSemverProvider
		.assess(package, &signal)
		.unwrap_or_else(|| panic!("expected assessment"));
	assert_eq!(assessment.severity, monochange_core::BumpSeverity::None);
	assert_eq!(assessment.summary, "manual review needed");
}

#[test]
fn cargo_manifest_helpers_cover_workspace_and_error_paths() {
	let versioned_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cargo/workspace-versioned");
	let root_manifest = versioned_root.join("Cargo.toml");
	assert!(has_workspace_section(&root_manifest).unwrap());
	let parsed = toml::from_str::<Value>(
		&std::fs::read_to_string(&root_manifest)
			.unwrap_or_else(|error| panic!("read workspace manifest: {error}")),
	)
	.unwrap_or_else(|error| panic!("parse workspace manifest: {error}"));
	assert_eq!(
		workspace_package_version(&parsed)
			.as_ref()
			.map(ToString::to_string)
			.as_deref(),
		Some("2.3.4")
	);

	let virtual_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/cargo/virtual-manifest");
	let virtual_manifest = virtual_root.join("Cargo.toml");
	assert_eq!(
		parse_package_manifest(&virtual_manifest, &virtual_root, None)
			.unwrap_or_else(|error| panic!("parse virtual manifest: {error}")),
		None
	);

	let invalid_workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo/invalid-workspace/invalid-workspace.toml");
	let invalid_workspace_error = has_workspace_section(&invalid_workspace)
		.err()
		.unwrap_or_else(|| panic!("expected invalid workspace error"));
	assert!(invalid_workspace_error
		.to_string()
		.contains("failed to parse"));

	let invalid_package_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo/invalid-package-name");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::create_dir_all(tempdir.path().join("crates/core"))
		.unwrap_or_else(|error| panic!("create temp package dir: {error}"));
	std::fs::copy(
		invalid_package_root.join("invalid-workspace.toml"),
		tempdir.path().join("invalid-workspace.toml"),
	)
	.unwrap_or_else(|error| panic!("copy workspace manifest: {error}"));
	std::fs::copy(
		invalid_package_root.join("crates/core/invalid-package.toml"),
		tempdir.path().join("crates/core/Cargo.toml"),
	)
	.unwrap_or_else(|error| panic!("copy package manifest: {error}"));
	let discovery_error =
		discover_workspace_packages(&tempdir.path().join("invalid-workspace.toml"))
			.err()
			.unwrap_or_else(|| panic!("expected invalid package discovery error"));
	assert!(discovery_error.to_string().contains("missing package.name"));
}

#[test]
fn rust_semver_provider_returns_none_for_non_cargo_packages() {
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"web",
		PathBuf::from("packages/web/package.json"),
		PathBuf::from("."),
		None,
		PublishState::Public,
	);
	let signal = ChangeSignal {
		package_id: package.id.clone(),
		requested_bump: None,
		explicit_version: None,
		change_origin: "direct-change".to_string(),
		evidence_refs: vec!["rust-semver:minor:new API".to_string()],
		notes: None,
		details: None,
		change_type: None,
		source_path: PathBuf::from(".changeset/feature.md"),
	};
	assert_eq!(
		RustSemverProvider.assess(&package, &signal),
		None::<CompatibilityAssessment>
	);
}
