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

/// Run the interactive change wizard.
///
/// Returns the user's selections or an error if the user cancels.
pub fn run_interactive_change(
	configuration: &WorkspaceConfiguration,
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

	// Step 3: Reason (required)
	let reason = prompt_reason()?;

	// Step 4: Details (optional)
	let details =
		prompt_optional("Details (optional long-form release notes — leave empty to skip)")?;

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

fn prompt_bump_for_target(target: &SelectableTarget) -> MonochangeResult<BumpSeverity> {
	let label = match target.kind {
		TargetKind::Group => format!("Bump for group `{}`:", target.id),
		TargetKind::Package => format!("Bump for package `{}`:", target.id),
	};

	let options = vec!["patch", "minor", "major"];
	let selection = Select::new(&label, options)
		.with_starting_cursor(0)
		.prompt()
		.map_err(|error| MonochangeError::Config(format!("bump selection cancelled: {error}")))?;

	match selection {
		"minor" => Ok(BumpSeverity::Minor),
		"major" => Ok(BumpSeverity::Major),
		_ => Ok(BumpSeverity::Patch),
	}
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

fn prompt_change_type_for_target(target: &SelectableTarget) -> MonochangeResult<Option<String>> {
	if target.configured_types.is_empty() {
		// No configured types — offer a free-text input
		return prompt_optional(&format!(
			"Change type for `{}`? (e.g. security, note — leave empty to skip):",
			target.id
		));
	}

	// Build options from configured types + "none" + "custom"
	let mut options = vec!["(none)".to_string()];
	options.extend(target.configured_types.iter().cloned());
	options.push("(custom)".to_string());

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

	match selection.as_str() {
		"(none)" => Ok(None),
		"(custom)" => {
			let custom = Text::new("Enter custom change type:")
				.prompt()
				.map_err(|error| {
					MonochangeError::Config(format!("custom type input cancelled: {error}"))
				})?;
			if custom.trim().is_empty() {
				Ok(None)
			} else {
				Ok(Some(custom))
			}
		}
		_ => Ok(Some(selection)),
	}
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
