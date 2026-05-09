use inquire::validator::Validation;
use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::ChangelogSettings;
use monochange_core::ChangesetSettings;
use monochange_core::EcosystemSettings;
use monochange_core::GroupChangelogInclude;
use monochange_core::GroupDefinition;
use monochange_core::MonochangeError;
use monochange_core::PackageDefinition;
use monochange_core::PackageType;
use monochange_core::PublishSettings;
use monochange_core::VersionFormat;
use monochange_core::WorkspaceConfiguration;
use monochange_core::WorkspaceDefaults;
use monochange_test_helpers::current_test_name;

use super::InteractiveOptions;
use super::SelectableTarget;
use super::TargetKind;
use super::build_selectable_targets;
use super::bump_options;
use super::bump_prompt_label;
use super::change_type_options;
use super::change_type_prompt_label;
use super::map_inquire_error;
use super::normalize_optional_text;
use super::parse_change_type_selection;
use super::parse_selected_bump;
use super::prompt_change_type_for_target;
use super::run_interactive_change;
use super::validate_reason_input;
use super::validate_semver_input;
use super::version_prompt_label;

fn fixture_path(relative: &str) -> std::path::PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_scenario_workspace(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn package_target(configured_types: Vec<String>) -> SelectableTarget {
	SelectableTarget {
		id: "core".to_string(),
		kind: TargetKind::Package,
		display: "core".to_string(),
		configured_types,
	}
}

fn empty_configuration() -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: std::path::PathBuf::from("."),
		defaults: WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		packages: Vec::new(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: EcosystemSettings::default(),
		npm: EcosystemSettings::default(),
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
		python: EcosystemSettings::default(),
		go: EcosystemSettings::default(),
	}
}

#[test]
fn shared_fs_test_support_helpers_cover_names_and_fixture_copying() {
	assert_eq!(
		current_test_name(),
		"shared_fs_test_support_helpers_cover_names_and_fixture_copying"
	);
	let named = std::thread::Builder::new()
		.name("case_1_interactive_helper_thread".to_string())
		.spawn(current_test_name)
		.unwrap_or_else(|error| panic!("spawn thread: {error}"))
		.join()
		.unwrap_or_else(|error| panic!("join thread: {error:?}"));
	assert_eq!(named, "interactive_helper_thread");
	let fixture = setup_fixture("test-support/setup-fixture");
	assert_eq!(
		std::fs::read_to_string(fixture.path().join("root.txt"))
			.unwrap_or_else(|error| panic!("read fixture: {error}")),
		"root fixture\n"
	);
	let scenario = setup_scenario_workspace("test-support/scenario-root");
	assert_eq!(
		std::fs::read_to_string(scenario.path().join("root-only.txt"))
			.unwrap_or_else(|error| panic!("read scenario: {error}")),
		"root scenario\n"
	);
}

#[test]
fn selectable_target_display_uses_display_label() {
	assert_eq!(package_target(Vec::new()).to_string(), "core");
}

#[test]
fn run_interactive_change_rejects_empty_workspace_configuration() {
	let error = run_interactive_change(&empty_configuration(), &InteractiveOptions::default())
		.err()
		.unwrap_or_else(|| panic!("expected interactive error"));
	assert!(
		error
			.to_string()
			.contains("no packages or groups found in workspace configuration")
	);
}

#[test]
fn bump_options_include_none() {
	assert_eq!(bump_options(), vec!["none", "patch", "minor", "major"]);
}

#[test]
fn parse_selected_bump_supports_none() {
	assert_eq!(parse_selected_bump("none"), BumpSeverity::None);
	assert_eq!(parse_selected_bump("minor"), BumpSeverity::Minor);
	assert_eq!(parse_selected_bump("major"), BumpSeverity::Major);
	assert_eq!(parse_selected_bump("patch"), BumpSeverity::Patch);
}

#[test]
fn prompt_label_helpers_cover_package_and_group_variants() {
	let package = package_target(Vec::new());
	let group = SelectableTarget {
		id: "sdk".to_string(),
		kind: TargetKind::Group,
		display: "sdk".to_string(),
		configured_types: vec!["docs".to_string()],
	};
	assert_eq!(bump_prompt_label(&package), "Bump for package `core`:");
	assert_eq!(bump_prompt_label(&group), "Bump for group `sdk`:");
	assert_eq!(
		version_prompt_label(&package),
		"Pin explicit version for `core`? (leave empty to skip):"
	);
	assert_eq!(
		version_prompt_label(&group),
		"Pin explicit version for group `sdk`? (leave empty to skip):"
	);
	assert_eq!(
		change_type_prompt_label(&package),
		"Change type for `core`:"
	);
	assert_eq!(
		change_type_prompt_label(&group),
		"Change type for group `sdk`:"
	);
}

#[test]
fn validation_helpers_cover_semver_reason_and_optional_normalization() {
	assert!(matches!(validate_semver_input(""), Validation::Valid));
	assert!(matches!(validate_semver_input("1.2.3"), Validation::Valid));
	assert!(matches!(
		validate_semver_input("not-a-version"),
		Validation::Invalid(_)
	));
	assert!(matches!(
		validate_reason_input("ship it"),
		Validation::Valid
	));
	assert!(matches!(
		validate_reason_input("   "),
		Validation::Invalid(_)
	));
	assert_eq!(normalize_optional_text(String::new()), None);
	assert_eq!(normalize_optional_text("   ".to_string()), None);
	assert_eq!(
		normalize_optional_text("details".to_string()),
		Some("details".to_string())
	);
}

#[test]
fn prompt_change_type_for_target_returns_none_for_targets_without_configured_types() {
	assert_eq!(
		prompt_change_type_for_target(&package_target(Vec::new())).unwrap(),
		None
	);
}

#[test]
fn change_type_options_return_none_for_targets_without_configured_types() {
	assert_eq!(change_type_options(&package_target(Vec::new())), None);
}

#[test]
fn change_type_options_include_none_and_configured_values() {
	assert_eq!(
		change_type_options(&package_target(vec![
			"docs".to_string(),
			"test".to_string()
		])),
		Some(vec![
			"(none)".to_string(),
			"docs".to_string(),
			"test".to_string(),
		])
	);
}

#[test]
fn parse_change_type_selection_handles_none_and_values() {
	assert_eq!(parse_change_type_selection("(none)"), None);
	assert_eq!(
		parse_change_type_selection("security"),
		Some("security".to_string())
	);
}

#[test]
fn build_selectable_targets_lists_groups_then_standalone_then_group_members() {
	let configuration = load_workspace_configuration(&fixture_path("monochange/release-base"))
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let targets = build_selectable_targets(&configuration);
	let displays = targets
		.iter()
		.map(|target| target.display.clone())
		.collect::<Vec<_>>();
	assert_eq!(
		displays,
		vec![
			"[group] sdk (core, app)".to_string(),
			"[package] app (member of group `sdk`)".to_string(),
			"[package] core (member of group `sdk`)".to_string(),
		]
	);
}

#[test]
fn build_selectable_targets_collects_configured_change_types_per_target() {
	let configuration = load_workspace_configuration(&fixture_path(
		"changeset-target-metadata/cli-type-only-change",
	))
	.unwrap_or_else(|error| panic!("configuration: {error}"));
	let targets = build_selectable_targets(&configuration);
	let core = targets
		.iter()
		.find(|target| target.id == "core")
		.unwrap_or_else(|| panic!("expected core target"));
	assert_eq!(core.kind, TargetKind::Package);
	assert_eq!(
		core.configured_types,
		vec!["docs".to_string(), "test".to_string()]
	);
}

#[test]
fn build_selectable_targets_includes_standalone_packages_before_group_members() {
	let configuration = WorkspaceConfiguration {
		root_path: std::path::PathBuf::from("."),
		defaults: WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		packages: vec![
			PackageDefinition {
				id: "app".to_string(),
				path: std::path::PathBuf::from("crates/app"),
				package_type: PackageType::Cargo,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				publish: PublishSettings::default(),
				version_format: VersionFormat::Primary,
			},
			PackageDefinition {
				id: "core".to_string(),
				path: std::path::PathBuf::from("crates/core"),
				package_type: PackageType::Cargo,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				publish: PublishSettings::default(),
				version_format: VersionFormat::Primary,
			},
			PackageDefinition {
				id: "web".to_string(),
				path: std::path::PathBuf::from("packages/web"),
				package_type: PackageType::Npm,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				publish: PublishSettings::default(),
				version_format: VersionFormat::Primary,
			},
		],
		groups: vec![GroupDefinition {
			id: "sdk".to_string(),
			packages: vec!["app".to_string(), "core".to_string()],
			changelog: None,
			changelog_include: GroupChangelogInclude::All,
			excluded_changelog_types: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
		}],
		cli: Vec::new(),
		changesets: ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: EcosystemSettings::default(),
		npm: EcosystemSettings::default(),
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
		python: EcosystemSettings::default(),
		go: EcosystemSettings::default(),
	};
	let displays = build_selectable_targets(&configuration)
		.into_iter()
		.map(|target| target.display)
		.collect::<Vec<_>>();
	assert_eq!(
		displays,
		vec![
			"[group] sdk (app, core)".to_string(),
			"[package] web".to_string(),
			"[package] app (member of group `sdk`)".to_string(),
			"[package] core (member of group `sdk`)".to_string(),
		]
	);
}

#[test]
fn parse_selected_bump_falls_back_to_patch_for_unrecognized_input() {
	assert_eq!(parse_selected_bump("unknown"), BumpSeverity::Patch);
	assert_eq!(parse_selected_bump(""), BumpSeverity::Patch);
	assert_eq!(parse_selected_bump("MAJOR"), BumpSeverity::Patch);
	assert_eq!(parse_selected_bump("Minor"), BumpSeverity::Patch);
}

#[test]
fn parse_change_type_selection_returns_none_only_for_placeholder() {
	assert_eq!(parse_change_type_selection("(none)"), None);
	assert_eq!(
		parse_change_type_selection("docs"),
		Some("docs".to_string())
	);
	assert_eq!(
		parse_change_type_selection("security"),
		Some("security".to_string())
	);
	assert_eq!(
		parse_change_type_selection("  spaced  "),
		Some("  spaced  ".to_string())
	);
}

#[test]
fn build_selectable_targets_returns_empty_vec_when_no_packages_or_groups() {
	let configuration = empty_configuration();
	let targets = build_selectable_targets(&configuration);
	assert!(targets.is_empty());
}

#[test]
fn build_selectable_targets_handles_group_with_empty_packages() {
	let configuration = WorkspaceConfiguration {
		root_path: std::path::PathBuf::from("."),
		defaults: WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		packages: Vec::new(),
		groups: vec![GroupDefinition {
			id: "empty-group".to_string(),
			packages: Vec::new(),
			changelog: None,
			changelog_include: GroupChangelogInclude::All,
			excluded_changelog_types: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
		}],
		cli: Vec::new(),
		changesets: ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: EcosystemSettings::default(),
		npm: EcosystemSettings::default(),
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
		python: EcosystemSettings::default(),
		go: EcosystemSettings::default(),
	};
	let targets = build_selectable_targets(&configuration);
	assert_eq!(targets.len(), 1);
	assert_eq!(targets[0].kind, TargetKind::Group);
	assert_eq!(targets[0].id, "empty-group");
	assert_eq!(targets[0].display, "[group] empty-group ()");
}

#[test]
fn build_selectable_targets_with_only_standalone_packages() {
	let configuration = WorkspaceConfiguration {
		root_path: std::path::PathBuf::from("."),
		defaults: WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		packages: vec![
			PackageDefinition {
				id: "alpha".to_string(),
				path: std::path::PathBuf::from("crates/alpha"),
				package_type: PackageType::Cargo,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				publish: PublishSettings::default(),
				version_format: VersionFormat::Primary,
			},
			PackageDefinition {
				id: "beta".to_string(),
				path: std::path::PathBuf::from("crates/beta"),
				package_type: PackageType::Cargo,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				publish: PublishSettings::default(),
				version_format: VersionFormat::Primary,
			},
		],
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: EcosystemSettings::default(),
		npm: EcosystemSettings::default(),
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
		python: EcosystemSettings::default(),
		go: EcosystemSettings::default(),
	};
	let targets = build_selectable_targets(&configuration);
	let ids: Vec<&str> = targets.iter().map(|t| t.id.as_str()).collect();
	assert_eq!(ids, vec!["alpha", "beta"]);
	assert!(targets.iter().all(|t| t.kind == TargetKind::Package));
}

#[test]
fn map_inquire_error_converts_interrupted_to_cancelled() {
	let error = inquire::error::InquireError::OperationInterrupted;
	assert!(matches!(
		map_inquire_error(error),
		MonochangeError::Cancelled
	));
}

#[test]
fn map_inquire_error_converts_canceled_to_cancelled() {
	let error = inquire::error::InquireError::OperationCanceled;
	assert!(matches!(
		map_inquire_error(error),
		MonochangeError::Cancelled
	));
}

#[test]
fn map_inquire_error_converts_other_errors_to_interactive() {
	let error = inquire::error::InquireError::Custom(inquire::error::CustomUserError::from(
		"test error".to_string(),
	));
	assert!(matches!(
		map_inquire_error(error),
		MonochangeError::Interactive { .. }
	));
}

#[test]
fn run_interactive_change_returns_cancelled_variant_is_distinct_from_config_error() {
	let config_error =
		run_interactive_change(&empty_configuration(), &InteractiveOptions::default())
			.err()
			.unwrap_or_else(|| panic!("expected error"));
	assert!(
		matches!(config_error, MonochangeError::Config(..)),
		"empty workspace should produce Config error, not Cancelled"
	);
	assert!(
		!matches!(config_error, MonochangeError::Cancelled),
		"empty workspace should not produce Cancelled"
	);
}

#[test]
fn build_selectable_targets_deduplicates_and_sorts_change_types() {
	let configuration = WorkspaceConfiguration {
		root_path: std::path::PathBuf::from("."),
		defaults: WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		packages: vec![PackageDefinition {
			id: "web".to_string(),
			path: std::path::PathBuf::from("packages/web"),
			package_type: PackageType::Npm,
			changelog: None,
			excluded_changelog_types: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			ignore_ecosystem_versioned_files: false,
			ignored_paths: Vec::new(),
			additional_paths: Vec::new(),
			tag: true,
			release: true,
			publish: PublishSettings::default(),
			version_format: VersionFormat::Primary,
		}],
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: EcosystemSettings::default(),
		npm: EcosystemSettings::default(),
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
		python: EcosystemSettings::default(),
		go: EcosystemSettings::default(),
	};
	let target = build_selectable_targets(&configuration)
		.into_iter()
		.next()
		.unwrap_or_else(|| panic!("expected target"));
	assert_eq!(
		target.configured_types,
		vec![
			"breaking".to_string(),
			"change".to_string(),
			"docs".to_string(),
			"feat".to_string(),
			"fix".to_string(),
			"major".to_string(),
			"minor".to_string(),
			"none".to_string(),
			"patch".to_string(),
			"refactor".to_string(),
			"security".to_string(),
			"test".to_string()
		]
	);
}
