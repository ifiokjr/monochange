use std::path::PathBuf;

use semver::Version;

use crate::materialize_dependency_edges;
use crate::BumpSeverity;
use crate::DependencyKind;
use crate::Ecosystem;
use crate::EcosystemSettings;
use crate::GroupDefinition;
use crate::PackageDefinition;
use crate::PackageDependency;
use crate::PackageRecord;
use crate::PackageType;
use crate::PublishState;
use crate::ReleaseOwnerKind;
use crate::VersionFormat;
use crate::WorkspaceConfiguration;
use crate::WorkspaceDefaults;

#[test]
fn bump_severity_orders_from_none_to_major() {
	assert!(BumpSeverity::Patch > BumpSeverity::None);
	assert!(BumpSeverity::Minor > BumpSeverity::Patch);
	assert!(BumpSeverity::Major > BumpSeverity::Minor);
}

#[test]
fn package_record_uses_manifest_path_for_stable_id() {
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		PathBuf::from("fixtures/cargo/workspace/crates/core/Cargo.toml"),
		PathBuf::from("fixtures/cargo/workspace"),
		Some(Version::new(1, 2, 3)),
		PublishState::Public,
	);

	assert_eq!(
		package.id,
		"cargo:fixtures/cargo/workspace/crates/core/Cargo.toml"
	);
	assert_eq!(package.current_version, Some(Version::new(1, 2, 3)));
}

#[test]
fn package_dependencies_preserve_kind_and_constraint() {
	let dependency = PackageDependency {
		name: "workspace-shared".to_string(),
		kind: DependencyKind::Runtime,
		version_constraint: Some("^1.0.0".to_string()),
		optional: false,
	};

	assert_eq!(dependency.kind, DependencyKind::Runtime);
	assert_eq!(dependency.version_constraint.as_deref(), Some("^1.0.0"));
}

#[test]
fn materialize_dependency_edges_matches_dependency_names_to_packages() {
	let target = PackageRecord::new(
		Ecosystem::Cargo,
		"workspace-shared",
		PathBuf::from("fixtures/cargo/workspace/crates/shared/Cargo.toml"),
		PathBuf::from("fixtures/cargo/workspace"),
		None,
		PublishState::Public,
	);
	let mut source = PackageRecord::new(
		Ecosystem::Cargo,
		"workspace-app",
		PathBuf::from("fixtures/cargo/workspace/crates/app/Cargo.toml"),
		PathBuf::from("fixtures/cargo/workspace"),
		None,
		PublishState::Public,
	);
	source.declared_dependencies.push(PackageDependency {
		name: "workspace-shared".to_string(),
		kind: DependencyKind::Runtime,
		version_constraint: Some("^1.0.0".to_string()),
		optional: false,
	});

	let edges = materialize_dependency_edges(&[source.clone(), target.clone()]);
	assert_eq!(edges.len(), 1);
	let edge = edges.first().unwrap_or_else(|| panic!("expected one edge"));
	assert_eq!(edge.from_package_id, source.id);
	assert_eq!(edge.to_package_id, target.id);
}

#[test]
fn workspace_configuration_can_find_group_membership_for_a_package() {
	let configuration = sample_workspace_configuration();
	let group = configuration
		.group_for_package("monochange")
		.unwrap_or_else(|| panic!("expected package group"));

	assert_eq!(group.id, "workspace");
	assert_eq!(group.packages, vec!["monochange", "monochange_core"]);
}

#[test]
fn workspace_configuration_uses_group_release_identity_for_group_members() {
	let configuration = sample_workspace_configuration();
	let identity = configuration
		.effective_release_identity("monochange")
		.unwrap_or_else(|| panic!("expected release identity"));

	assert_eq!(identity.owner_id, "workspace");
	assert_eq!(identity.owner_kind, ReleaseOwnerKind::Group);
	assert_eq!(identity.group_id.as_deref(), Some("workspace"));
	assert!(identity.tag);
	assert!(identity.release);
	assert_eq!(identity.version_format, VersionFormat::Primary);
	assert_eq!(identity.members, vec!["monochange", "monochange_core"]);
}

#[test]
fn workspace_configuration_uses_package_release_identity_when_not_grouped() {
	let configuration = sample_workspace_configuration();
	let identity = configuration
		.effective_release_identity("monochange_graph")
		.unwrap_or_else(|| panic!("expected release identity"));

	assert_eq!(identity.owner_id, "monochange_graph");
	assert_eq!(identity.owner_kind, ReleaseOwnerKind::Package);
	assert_eq!(identity.group_id, None);
	assert!(!identity.tag);
	assert!(!identity.release);
	assert_eq!(identity.version_format, VersionFormat::Namespaced);
	assert_eq!(identity.members, vec!["monochange_graph"]);
}

fn sample_workspace_configuration() -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: PathBuf::from("."),
		defaults: WorkspaceDefaults::default(),
		packages: vec![
			PackageDefinition {
				id: "monochange".to_string(),
				path: PathBuf::from("crates/monochange"),
				package_type: PackageType::Cargo,
				changelog: Some(PathBuf::from("crates/monochange/CHANGELOG.md")),
				versioned_files: Vec::new(),
				tag: false,
				release: false,
				version_format: VersionFormat::Namespaced,
			},
			PackageDefinition {
				id: "monochange_core".to_string(),
				path: PathBuf::from("crates/monochange_core"),
				package_type: PackageType::Cargo,
				changelog: Some(PathBuf::from("crates/monochange_core/CHANGELOG.md")),
				versioned_files: Vec::new(),
				tag: false,
				release: false,
				version_format: VersionFormat::Namespaced,
			},
			PackageDefinition {
				id: "monochange_graph".to_string(),
				path: PathBuf::from("crates/monochange_graph"),
				package_type: PackageType::Cargo,
				changelog: None,
				versioned_files: Vec::new(),
				tag: false,
				release: false,
				version_format: VersionFormat::Namespaced,
			},
		],
		groups: vec![GroupDefinition {
			id: "workspace".to_string(),
			packages: vec!["monochange".to_string(), "monochange_core".to_string()],
			changelog: Some(PathBuf::from("CHANGELOG.md")),
			versioned_files: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
		}],
		workflows: Vec::new(),
		cargo: EcosystemSettings::default(),
		npm: EcosystemSettings::default(),
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
	}
}
