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
	/// Configured change types from `changelog_sections` for this target.
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
	/// Package or group ids that caused this dependent change.
	pub caused_by: Vec<String>,
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
	pub caused_by: Vec<String>,
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
		caused_by: options.caused_by.clone(),
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
		let configured_types = configuration
			.changelog
			.types
			.keys()
			.filter(|&key| !group.excluded_changelog_types.contains(key))
			.cloned()
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
			let configured_types = configuration
				.changelog
				.types
				.keys()
				.filter(|&key| !package.excluded_changelog_types.contains(key))
				.cloned()
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
			let configured_types = configuration
				.changelog
				.types
				.keys()
				.filter(|&key| !package.excluded_changelog_types.contains(key))
				.cloned()
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
#[path = "__tests/interactive.rs"]
mod __tests;
