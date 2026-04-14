use std::collections::BTreeSet;
use std::fmt;

use inquire::MultiSelect;
use inquire::Select;
use inquire::Text;
use inquire::validator::Validation;
use monochange_core::BumpSeverity;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::WorkspaceConfiguration;

/// A selectable item in the package/group picker.
#[derive(Debug, Clone)]
struct SelectableTarget {
	id: String,
	kind: TargetKind,
	display: String,
	/// Configured change types from `extra_changelog_sections` for this target.
	configured_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKind {
	Package,
	Group,
}

impl fmt::Display for SelectableTarget {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(&self.display)
	}
}

/// Result of the interactive change flow.
pub struct InteractiveChangeResult {
	/// Per-target (package or group id) selections.
	pub targets: Vec<InteractiveTarget>,
	/// Release-note summary.
	pub reason: String,
	/// Optional long-form details.
	pub details: Option<String>,
}

pub struct InteractiveTarget {
	pub id: String,
	pub bump: BumpSeverity,
	pub version: Option<String>,
	pub change_type: Option<String>,
}

/// CLI-provided values that bypass their interactive prompts when present.
#[derive(Debug, Default)]
pub struct InteractiveOptions {
	pub reason: Option<String>,
	pub details: Option<String>,
}

/// Run the interactive change wizard.
///
/// Returns the user's selections or an error if the user cancels.
pub fn run_interactive_change(
	configuration: &WorkspaceConfiguration,
	options: &InteractiveOptions,
) -> MonochangeResult<InteractiveChangeResult> {
	let targets = build_selectable_targets(configuration);

	if targets.is_empty() {
		return Err(MonochangeError::Config(
			"no packages or groups found in workspace configuration".to_string(),
		));
	}

	// Step 1: Select packages/groups
	let selected = prompt_select_targets(&targets)?;

	if selected.is_empty() {
		return Err(MonochangeError::Config(
			"no packages or groups selected".to_string(),
		));
	}

	// Step 2: For each selected target, choose bump, optional version, and optional change type
	let mut interactive_targets = Vec::new();

	for target in &selected {
		let bump = prompt_bump_for_target(target)?;
		let version = prompt_version_for_target(target)?;
		let change_type = prompt_change_type_for_target(target)?;

		interactive_targets.push(InteractiveTarget {
			id: target.id.clone(),
			bump,
			version,
			change_type,
		});
	}

	// Step 3: Reason (required) — use CLI value if provided
	let reason = if let Some(reason) = &options.reason {
		reason.clone()
	} else {
		prompt_reason()?
	};

	// Step 4: Details (optional) — use CLI value if provided
	let details = if let Some(details) = &options.details {
		Some(details.clone())
	} else {
		prompt_optional("Details (optional long-form release notes — leave empty to skip)")?
	};

	Ok(InteractiveChangeResult {
		targets: interactive_targets,
		reason,
		details,
	})
}

fn build_selectable_targets(configuration: &WorkspaceConfiguration) -> Vec<SelectableTarget> {
	let grouped_package_ids = configuration
		.groups
		.iter()
		.flat_map(|group| group.packages.iter().cloned())
		.collect::<BTreeSet<_>>();

	let mut targets = Vec::new();

	// Groups first
	for group in &configuration.groups {
		let members = group.packages.join(", ");
		let configured_types = group
			.extra_changelog_sections
			.iter()
			.flat_map(|section| section.types.iter().cloned())
			.collect::<BTreeSet<_>>()
			.into_iter()
			.collect();
		targets.push(SelectableTarget {
			id: group.id.clone(),
			kind: TargetKind::Group,
			display: format!("[group] {} ({})", group.id, members),

			configured_types,
		});
	}

	// Then standalone packages (not in any group)
	for package in &configuration.packages {
		if !grouped_package_ids.contains(&package.id) {
			let configured_types = package
				.extra_changelog_sections
				.iter()
				.flat_map(|section| section.types.iter().cloned())
				.collect::<BTreeSet<_>>()
				.into_iter()
				.collect();
			targets.push(SelectableTarget {
				id: package.id.clone(),
				kind: TargetKind::Package,
				display: format!("[package] {}", package.id),

				configured_types,
			});
		}
	}

	// Then grouped packages (selectable individually or alongside their group)
	for package in &configuration.packages {
		if grouped_package_ids.contains(&package.id) {
			let group = configuration
				.group_for_package(&package.id)
				.map(|group| format!(" (member of group `{}`)", group.id))
				.unwrap_or_default();
			let configured_types = package
				.extra_changelog_sections
				.iter()
				.flat_map(|section| section.types.iter().cloned())
				.collect::<BTreeSet<_>>()
				.into_iter()
				.collect();
			targets.push(SelectableTarget {
				id: package.id.clone(),
				kind: TargetKind::Package,
				display: format!("[package] {}{group}", package.id),

				configured_types,
			});
		}
	}

	targets
}

fn prompt_select_targets(targets: &[SelectableTarget]) -> MonochangeResult<Vec<SelectableTarget>> {
	let selected = MultiSelect::new(
		"Select packages/groups to include in this changeset:",
		targets.to_vec(),
	)
	.with_page_size(15)
	.prompt()
	.map_err(map_inquire_error)?;

	Ok(selected)
}

fn bump_options() -> Vec<&'static str> {
	vec!["none", "patch", "minor", "major"]
}

fn parse_selected_bump(selection: &str) -> BumpSeverity {
	match selection {
		"none" => BumpSeverity::None,
		"patch" => BumpSeverity::Patch,
		"minor" => BumpSeverity::Minor,
		"major" => BumpSeverity::Major,
		_other => BumpSeverity::Patch,
	}
}

fn bump_prompt_label(target: &SelectableTarget) -> String {
	match target.kind {
		TargetKind::Group => format!("Bump for group `{}`:", target.id),
		TargetKind::Package => format!("Bump for package `{}`:", target.id),
	}
}

fn prompt_bump_for_target(target: &SelectableTarget) -> MonochangeResult<BumpSeverity> {
	let selection = Select::new(&bump_prompt_label(target), bump_options())
		.with_starting_cursor(0)
		.prompt()
		.map_err(map_inquire_error)?;

	Ok(parse_selected_bump(selection))
}

fn version_prompt_label(target: &SelectableTarget) -> String {
	match target.kind {
		TargetKind::Group => {
			format!(
				"Pin explicit version for group `{}`? (leave empty to skip):",
				target.id
			)
		}
		TargetKind::Package => {
			format!(
				"Pin explicit version for `{}`? (leave empty to skip):",
				target.id
			)
		}
	}
}

fn validate_semver_input(input: &str) -> Validation {
	if input.is_empty() {
		return Validation::Valid;
	}
	match semver::Version::parse(input) {
		Ok(_) => Validation::Valid,
		Err(error) => Validation::Invalid(format!("invalid semver: {error}").into()),
	}
}

fn normalize_optional_text(value: String) -> Option<String> {
	if value.trim().is_empty() {
		None
	} else {
		Some(value)
	}
}

fn prompt_version_for_target(target: &SelectableTarget) -> MonochangeResult<Option<String>> {
	let version = Text::new(&version_prompt_label(target))
		.with_validator(|input: &str| Ok(validate_semver_input(input)))
		.prompt()
		.map_err(map_inquire_error)?;

	Ok(normalize_optional_text(version))
}

fn change_type_options(target: &SelectableTarget) -> Option<Vec<String>> {
	(!target.configured_types.is_empty()).then(|| {
		let mut options = vec!["(none)".to_string()];
		options.extend(target.configured_types.iter().cloned());
		options
	})
}

fn parse_change_type_selection(selection: &str) -> Option<String> {
	match selection {
		"(none)" => None,
		_ => Some(selection.to_string()),
	}
}

fn change_type_prompt_label(target: &SelectableTarget) -> String {
	match target.kind {
		TargetKind::Group => format!("Change type for group `{}`:", target.id),
		TargetKind::Package => format!("Change type for `{}`:", target.id),
	}
}

fn prompt_change_type_for_target(target: &SelectableTarget) -> MonochangeResult<Option<String>> {
	let Some(options) = change_type_options(target) else {
		return Ok(None);
	};

	let selection = Select::new(&change_type_prompt_label(target), options)
		.with_starting_cursor(0)
		.prompt()
		.map_err(map_inquire_error)?;

	Ok(parse_change_type_selection(&selection))
}

fn validate_reason_input(input: &str) -> Validation {
	if input.trim().is_empty() {
		Validation::Invalid("reason cannot be empty".into())
	} else {
		Validation::Valid
	}
}

fn prompt_reason() -> MonochangeResult<String> {
	let reason = Text::new("Release-note summary (required):")
		.with_validator(|input: &str| Ok(validate_reason_input(input)))
		.prompt()
		.map_err(map_inquire_error)?;

	Ok(reason)
}

fn prompt_optional(label: &str) -> MonochangeResult<Option<String>> {
	let value = Text::new(label).prompt().map_err(map_inquire_error)?;

	Ok(normalize_optional_text(value))
}

fn map_inquire_error(error: inquire::error::InquireError) -> MonochangeError {
	match error {
		inquire::error::InquireError::OperationInterrupted
		| inquire::error::InquireError::OperationCanceled => MonochangeError::Cancelled,
		other => {
			MonochangeError::Interactive {
				message: other.to_string(),
			}
		}
	}
}

#[cfg(test)]
mod __tests {
	use inquire::validator::Validation;
	use monochange_config::load_workspace_configuration;
	use monochange_core::BumpSeverity;
	use monochange_core::ChangesetSettings;
	use monochange_core::EcosystemSettings;
	use monochange_core::ExtraChangelogSection;
	use monochange_core::GroupChangelogInclude;
	use monochange_core::GroupDefinition;
	use monochange_core::MonochangeError;
	use monochange_core::PackageDefinition;
	use monochange_core::PackageType;
	use monochange_core::PublishSettings;
	use monochange_core::ReleaseNotesSettings;
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
		monochange_test_helpers::fs::setup_scenario_workspace_from(
			env!("CARGO_MANIFEST_DIR"),
			relative,
		)
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
			release_notes: ReleaseNotesSettings::default(),
			packages: Vec::new(),
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: ChangesetSettings::default(),
			source: None,
			cargo: EcosystemSettings::default(),
			npm: EcosystemSettings::default(),
			deno: EcosystemSettings::default(),
			dart: EcosystemSettings::default(),
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
			release_notes: ReleaseNotesSettings::default(),
			packages: vec![
				PackageDefinition {
					id: "app".to_string(),
					path: std::path::PathBuf::from("crates/app"),
					package_type: PackageType::Cargo,
					changelog: None,
					extra_changelog_sections: Vec::new(),
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
					extra_changelog_sections: Vec::new(),
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
					extra_changelog_sections: Vec::new(),
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
				extra_changelog_sections: Vec::new(),
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
			cargo: EcosystemSettings::default(),
			npm: EcosystemSettings::default(),
			deno: EcosystemSettings::default(),
			dart: EcosystemSettings::default(),
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
			release_notes: ReleaseNotesSettings::default(),
			packages: Vec::new(),
			groups: vec![GroupDefinition {
				id: "empty-group".to_string(),
				packages: Vec::new(),
				changelog: None,
				changelog_include: GroupChangelogInclude::All,
				extra_changelog_sections: Vec::new(),
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
			cargo: EcosystemSettings::default(),
			npm: EcosystemSettings::default(),
			deno: EcosystemSettings::default(),
			dart: EcosystemSettings::default(),
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
			release_notes: ReleaseNotesSettings::default(),
			packages: vec![
				PackageDefinition {
					id: "alpha".to_string(),
					path: std::path::PathBuf::from("crates/alpha"),
					package_type: PackageType::Cargo,
					changelog: None,
					extra_changelog_sections: Vec::new(),
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
					extra_changelog_sections: Vec::new(),
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
			cargo: EcosystemSettings::default(),
			npm: EcosystemSettings::default(),
			deno: EcosystemSettings::default(),
			dart: EcosystemSettings::default(),
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
			release_notes: ReleaseNotesSettings::default(),
			packages: vec![PackageDefinition {
				id: "web".to_string(),
				path: std::path::PathBuf::from("packages/web"),
				package_type: PackageType::Npm,
				changelog: None,
				extra_changelog_sections: vec![
					ExtraChangelogSection {
						name: "Docs".to_string(),
						types: vec!["test".to_string(), "docs".to_string()],
						default_bump: None,
						description: None,
					},
					ExtraChangelogSection {
						name: "More".to_string(),
						types: vec!["docs".to_string(), "security".to_string()],
						default_bump: None,
						description: None,
					},
				],
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
			cargo: EcosystemSettings::default(),
			npm: EcosystemSettings::default(),
			deno: EcosystemSettings::default(),
			dart: EcosystemSettings::default(),
		};
		let target = build_selectable_targets(&configuration)
			.into_iter()
			.next()
			.unwrap_or_else(|| panic!("expected target"));
		assert_eq!(
			target.configured_types,
			vec![
				"docs".to_string(),
				"security".to_string(),
				"test".to_string()
			]
		);
	}
}
