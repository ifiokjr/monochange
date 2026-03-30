use std::path::PathBuf;

use semver::Version;

use crate::default_workflows;
use crate::materialize_dependency_edges;
use crate::render_release_notes;
use crate::BumpSeverity;
use crate::ChangelogFormat;
use crate::ChangelogTarget;
use crate::ChangesetPolicyStatus;
use crate::DependencyKind;
use crate::DeploymentTrigger;
use crate::Ecosystem;
use crate::EcosystemSettings;
use crate::GitHubChangesetBotSettings;
use crate::GroupDefinition;
use crate::PackageDefinition;
use crate::PackageDependency;
use crate::PackageRecord;
use crate::PackageType;
use crate::PublishState;
use crate::ReleaseNotesDocument;
use crate::ReleaseNotesSection;
use crate::ReleaseNotesSettings;
use crate::ReleaseOwnerKind;
use crate::VersionFormat;
use crate::WorkflowStepDefinition;
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

	assert_eq!(package.id, "cargo:crates/core/Cargo.toml");
	assert_eq!(package.current_version, Some(Version::new(1, 2, 3)));
}

#[test]
fn package_record_ids_are_stable_for_relative_and_absolute_roots() {
	let workspace_root = PathBuf::from("fixtures/cargo/workspace");
	let manifest_path = workspace_root.join("crates/core/Cargo.toml");
	let relative = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		manifest_path.clone(),
		workspace_root.clone(),
		Some(Version::new(1, 2, 3)),
		PublishState::Public,
	);
	let absolute_root = std::env::current_dir()
		.unwrap_or_else(|error| panic!("cwd: {error}"))
		.join(&workspace_root);
	let absolute = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		absolute_root.join("crates/core/Cargo.toml"),
		absolute_root,
		Some(Version::new(1, 2, 3)),
		PublishState::Public,
	);

	assert_eq!(relative.id, absolute.id);
	assert_eq!(relative.id, "cargo:crates/core/Cargo.toml");
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
fn changeset_policy_status_renders_stable_strings() {
	assert_eq!(ChangesetPolicyStatus::Passed.as_str(), "passed");
	assert_eq!(ChangesetPolicyStatus::Failed.to_string(), "failed");
	assert_eq!(ChangesetPolicyStatus::Skipped.as_str(), "skipped");
	assert_eq!(ChangesetPolicyStatus::NotRequired.as_str(), "not_required");
}

#[test]
fn github_changeset_bot_settings_default_to_opt_in_enforcement() {
	let settings = GitHubChangesetBotSettings::default();
	assert!(!settings.enabled);
	assert!(settings.required);
	assert!(settings.comment_on_failure);
	assert!(settings.skip_labels.is_empty());
}

#[test]
fn default_workflows_expose_validate_discover_change_and_release() {
	let workflows = default_workflows();
	let workflow_names = workflows
		.iter()
		.map(|workflow| workflow.name.as_str())
		.collect::<Vec<_>>();
	assert_eq!(
		workflow_names,
		vec!["validate", "discover", "change", "release"]
	);
	let validate_workflow = workflows
		.first()
		.unwrap_or_else(|| panic!("expected validate workflow"));
	assert_eq!(
		validate_workflow.steps,
		vec![WorkflowStepDefinition::Validate]
	);
}

#[test]
fn render_release_notes_supports_monochange_and_keep_a_changelog_formats() {
	let document = ReleaseNotesDocument {
		title: "1.2.3".to_string(),
		summary: vec!["Grouped release for `sdk`.".to_string()],
		sections: vec![ReleaseNotesSection {
			title: "Changed".to_string(),
			entries: vec!["add release automation".to_string()],
		}],
	};

	let monochange = render_release_notes(ChangelogFormat::Monochange, &document);
	let keep_a_changelog = render_release_notes(ChangelogFormat::KeepAChangelog, &document);

	assert!(monochange.contains("## 1.2.3"));
	assert!(monochange.contains("Grouped release for `sdk`."));
	assert!(monochange.contains("- add release automation"));
	assert!(!monochange.contains("## [1.2.3]"));
	assert!(keep_a_changelog.contains("## [1.2.3]"));
	assert!(keep_a_changelog.contains("### Changed"));
	assert!(keep_a_changelog.contains("- add release automation"));
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
		release_notes: ReleaseNotesSettings::default(),
		deployments: vec![crate::DeploymentDefinition {
			name: "production".to_string(),
			trigger: DeploymentTrigger::ReleasePrMerge,
			workflow: "deploy-production".to_string(),
			environment: Some("production".to_string()),
			release_targets: vec!["workspace".to_string()],
			requires: vec!["main".to_string()],
			metadata: std::collections::BTreeMap::new(),
		}],
		packages: vec![
			PackageDefinition {
				id: "monochange".to_string(),
				path: PathBuf::from("crates/monochange"),
				package_type: PackageType::Cargo,
				changelog: Some(ChangelogTarget {
					path: PathBuf::from("crates/monochange/changelog.md"),
					format: ChangelogFormat::Monochange,
				}),
				extra_changelog_sections: Vec::new(),
				empty_update_message: None,
				versioned_files: Vec::new(),
				tag: false,
				release: false,
				version_format: VersionFormat::Namespaced,
			},
			PackageDefinition {
				id: "monochange_core".to_string(),
				path: PathBuf::from("crates/monochange_core"),
				package_type: PackageType::Cargo,
				changelog: Some(ChangelogTarget {
					path: PathBuf::from("crates/monochange_core/changelog.md"),
					format: ChangelogFormat::Monochange,
				}),
				extra_changelog_sections: Vec::new(),
				empty_update_message: None,
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
				extra_changelog_sections: Vec::new(),
				empty_update_message: None,
				versioned_files: Vec::new(),
				tag: false,
				release: false,
				version_format: VersionFormat::Namespaced,
			},
		],
		groups: vec![GroupDefinition {
			id: "workspace".to_string(),
			packages: vec!["monochange".to_string(), "monochange_core".to_string()],
			changelog: Some(ChangelogTarget {
				path: PathBuf::from("changelog.md"),
				format: ChangelogFormat::Monochange,
			}),
			extra_changelog_sections: Vec::new(),
			empty_update_message: None,
			versioned_files: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
		}],
		workflows: Vec::new(),
		github: None,
		cargo: EcosystemSettings::default(),
		npm: EcosystemSettings::default(),
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
	}
}
