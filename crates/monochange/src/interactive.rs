use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;

use inquire::validator::Validation;
use inquire::MultiSelect;
use inquire::Select;
use inquire::Text;
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
	/// Group member package ids (empty for standalone packages).
	member_package_ids: BTreeSet<String>,
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
			member_package_ids: group.packages.iter().cloned().collect(),
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
				member_package_ids: BTreeSet::new(),
				configured_types,
			});
		}
	}

	// Then grouped packages (selectable individually, but conflicts with group selection
	// are prevented)
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
				member_package_ids: BTreeSet::new(),
				configured_types,
			});
		}
	}

	targets
}

fn prompt_select_targets(targets: &[SelectableTarget]) -> MonochangeResult<Vec<SelectableTarget>> {
	let group_members: BTreeMap<String, BTreeSet<String>> = targets
		.iter()
		.filter(|target| target.kind == TargetKind::Group)
		.map(|target| (target.id.clone(), target.member_package_ids.clone()))
		.collect();

	let package_to_group: BTreeMap<String, String> = group_members
		.iter()
		.flat_map(|(group_id, members)| {
			members
				.iter()
				.map(move |member| (member.clone(), group_id.clone()))
		})
		.collect();

	let validator = move |selections: &[inquire::list_option::ListOption<&SelectableTarget>]| {
		let selected_group_ids: BTreeSet<&str> = selections
			.iter()
			.filter(|selection| selection.value.kind == TargetKind::Group)
			.map(|selection| selection.value.id.as_str())
			.collect();

		// Check: if a group is selected, none of its member packages should be selected
		for selection in selections {
			if selection.value.kind == TargetKind::Package {
				if let Some(owning_group) = package_to_group.get(&selection.value.id) {
					if selected_group_ids.contains(owning_group.as_str()) {
						return Ok(Validation::Invalid(
							format!(
								"cannot select both group `{owning_group}` and its member `{}`; select only the group or individual members",
								selection.value.id
							)
							.into(),
						));
					}
				}
			}
		}

		Ok(Validation::Valid)
	};

	let selected = MultiSelect::new(
		"Select packages/groups to include in this changeset:",
		targets.to_vec(),
	)
	.with_validator(validator)
	.with_page_size(15)
	.prompt()
	.map_err(|error| {
		MonochangeError::Config(format!("interactive selection cancelled: {error}"))
	})?;

	Ok(selected)
}

fn bump_options() -> Vec<&'static str> {
	vec!["none", "patch", "minor", "major"]
}

fn parse_selected_bump(selection: &str) -> BumpSeverity {
	match selection {
		"none" => BumpSeverity::None,
		"minor" => BumpSeverity::Minor,
		"major" => BumpSeverity::Major,
		_ => BumpSeverity::Patch,
	}
}

fn prompt_bump_for_target(target: &SelectableTarget) -> MonochangeResult<BumpSeverity> {
	let label = match target.kind {
		TargetKind::Group => format!("Bump for group `{}`:", target.id),
		TargetKind::Package => format!("Bump for package `{}`:", target.id),
	};

	let selection = Select::new(&label, bump_options())
		.with_starting_cursor(0)
		.prompt()
		.map_err(|error| MonochangeError::Config(format!("bump selection cancelled: {error}")))?;

	Ok(parse_selected_bump(selection))
}

fn prompt_version_for_target(target: &SelectableTarget) -> MonochangeResult<Option<String>> {
	let label = match target.kind {
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
	};

	let version = Text::new(&label)
		.with_validator(|input: &str| {
			if input.is_empty() {
				return Ok(Validation::Valid);
			}
			match semver::Version::parse(input) {
				Ok(_) => Ok(Validation::Valid),
				Err(error) => Ok(Validation::Invalid(
					format!("invalid semver: {error}").into(),
				)),
			}
		})
		.prompt()
		.map_err(|error| MonochangeError::Config(format!("version input cancelled: {error}")))?;

	if version.is_empty() {
		Ok(None)
	} else {
		Ok(Some(version))
	}
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

fn prompt_change_type_for_target(target: &SelectableTarget) -> MonochangeResult<Option<String>> {
	let Some(options) = change_type_options(target) else {
		return Ok(None);
	};

	let label = match target.kind {
		TargetKind::Group => format!("Change type for group `{}`:", target.id),
		TargetKind::Package => format!("Change type for `{}`:", target.id),
	};

	let selection = Select::new(&label, options)
		.with_starting_cursor(0)
		.prompt()
		.map_err(|error| {
			MonochangeError::Config(format!("change type selection cancelled: {error}"))
		})?;

	Ok(parse_change_type_selection(&selection))
}

fn prompt_reason() -> MonochangeResult<String> {
	let reason = Text::new("Release-note summary (required):")
		.with_validator(|input: &str| {
			if input.trim().is_empty() {
				Ok(Validation::Invalid("reason cannot be empty".into()))
			} else {
				Ok(Validation::Valid)
			}
		})
		.prompt()
		.map_err(|error| MonochangeError::Config(format!("reason input cancelled: {error}")))?;

	Ok(reason)
}

fn prompt_optional(label: &str) -> MonochangeResult<Option<String>> {
	let value = Text::new(label)
		.prompt()
		.map_err(|error| MonochangeError::Config(format!("input cancelled: {error}")))?;

	if value.trim().is_empty() {
		Ok(None)
	} else {
		Ok(Some(value))
	}
}

#[cfg(test)]
mod __tests {
	use std::collections::BTreeSet;

	use monochange_config::load_workspace_configuration;
	use monochange_core::BumpSeverity;

	use super::build_selectable_targets;
	use super::bump_options;
	use super::change_type_options;
	use super::parse_change_type_selection;
	use super::parse_selected_bump;
	use super::prompt_change_type_for_target;
	use super::SelectableTarget;
	use super::TargetKind;

	fn fixture_path(relative: &str) -> std::path::PathBuf {
		std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../fixtures/tests")
			.join(relative)
	}

	fn package_target(configured_types: Vec<String>) -> SelectableTarget {
		SelectableTarget {
			id: "core".to_string(),
			kind: TargetKind::Package,
			display: "core".to_string(),
			member_package_ids: BTreeSet::new(),
			configured_types,
		}
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
}
