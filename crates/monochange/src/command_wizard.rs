use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

use inquire::Confirm;
use inquire::Select;
use inquire::Text;
use inquire::validator::Validation;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use serde::Deserialize;
use toml_edit::Array;
use toml_edit::DocumentMut;
use toml_edit::InlineTable;
use toml_edit::Item;
use toml_edit::Table;
use toml_edit::Value;
use toml_edit::value;

const CONFIG_FILE: &str = "monochange.toml";
const STEP_KIND_SHELL_COMMAND: &str = "Command";
const SAVE_STEPS_LABEL: &str = "Save command steps";
const MUTED_TEXT_START: &str = "\x1b[2m";
const MUTED_TEXT_END: &str = "\x1b[0m";
const STEP_SCORE_BASE: i64 = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommandSummary {
	pub(crate) name: String,
	help_text: Option<String>,
	step_count: usize,
}

impl fmt::Display for CommandSummary {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match &self.help_text {
			Some(help_text) => write!(formatter, "{} — {}", self.name, help_text),
			None => write!(formatter, "{} — {} steps", self.name, self.step_count),
		}
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommandStepDraft {
	pub(crate) kind: String,
	pub(crate) name: Option<String>,
	pub(crate) command: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub(crate) struct CommandInputDraft {
	pub(crate) name: String,
	#[serde(rename = "type")]
	pub(crate) kind: String,
	#[serde(default)]
	pub(crate) help_text: Option<String>,
	#[serde(default)]
	pub(crate) required: bool,
	#[serde(default)]
	pub(crate) default: Option<String>,
	#[serde(default)]
	pub(crate) choices: Vec<String>,
	#[serde(default)]
	pub(crate) short: Option<char>,
}

impl CommandStepDraft {
	pub(crate) fn built_in(kind: impl Into<String>) -> Self {
		Self {
			kind: kind.into(),
			name: None,
			command: None,
		}
	}

	fn shell_command(command: String, name: Option<String>) -> Self {
		Self {
			kind: STEP_KIND_SHELL_COMMAND.to_string(),
			name,
			command: Some(command),
		}
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CommandStepUpdate {
	KeepExisting,
	Replace(Vec<CommandStepDraft>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommandUpdate {
	pub(crate) original_name: Option<String>,
	pub(crate) name: String,
	pub(crate) help_text: Option<String>,
	pub(crate) dry_run: bool,
	pub(crate) inputs: Vec<CommandInputDraft>,
	pub(crate) steps: CommandStepUpdate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommandDetails {
	name: String,
	help_text: Option<String>,
	dry_run: bool,
	inputs: Vec<CommandInputDraft>,
	steps: Vec<CommandStepDraft>,
}

#[derive(Clone, Debug, Deserialize, Default)]
struct RawCliRoot {
	#[serde(default)]
	cli: BTreeMap<String, RawCliCommand>,
}

#[derive(Clone, Debug, Deserialize, Default)]
struct RawCliCommand {
	#[serde(default)]
	help_text: Option<String>,
	#[serde(default)]
	inputs: Vec<CommandInputDraft>,
	#[serde(default)]
	steps: Vec<RawCliStep>,
	#[serde(default)]
	dry_run: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct RawCliStep {
	#[serde(rename = "type")]
	kind: String,
	#[serde(default)]
	name: Option<String>,
	#[serde(default)]
	command: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DashboardAction {
	AddCommand,
	EditCommand,
	OpenEditor,
	Quit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StepChoice {
	kind: String,
	description: &'static str,
	is_save: bool,
}

impl StepChoice {
	fn new(kind: impl Into<String>) -> Self {
		let kind = kind.into();
		Self {
			description: step_choice_description(&kind),
			kind,
			is_save: false,
		}
	}

	fn save() -> Self {
		Self {
			kind: SAVE_STEPS_LABEL.to_string(),
			description: "Finish the command after adding at least one step",
			is_save: true,
		}
	}
}

impl fmt::Display for StepChoice {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			formatter,
			"{} {MUTED_TEXT_START}— {}{MUTED_TEXT_END}",
			self.kind, self.description
		)
	}
}

impl fmt::Display for DashboardAction {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::AddCommand => formatter.write_str("Add a new [cli.<name>] command"),
			Self::EditCommand => formatter.write_str("Edit an existing command"),
			Self::OpenEditor => formatter.write_str("Open monochange.toml in $VISUAL/$EDITOR"),
			Self::Quit => formatter.write_str("Quit without changes"),
		}
	}
}

#[coverage(off)]
pub(crate) fn run_command_wizard(root: &Path) -> MonochangeResult<String> {
	let config_path = root.join(CONFIG_FILE);
	let config_text = read_config_text(&config_path)?;
	let summaries = cli_command_summaries(&config_text)?;
	let action = Select::new(
		"Command dashboard — what do you want to do?",
		dashboard_actions(!summaries.is_empty()),
	)
	.prompt()
	.map_err(map_inquire_error)?;

	match action {
		DashboardAction::AddCommand => create_command(root, &config_path, &config_text, &summaries),
		DashboardAction::EditCommand => edit_command(root, &config_path, &config_text, &summaries),
		DashboardAction::OpenEditor => {
			open_config_in_editor(&config_path)?;
			Ok(format!("opened {} in your editor", config_path.display()))
		}
		DashboardAction::Quit => Ok("command wizard exited without changes".to_string()),
	}
}

pub(crate) fn cli_command_summaries(config_text: &str) -> MonochangeResult<Vec<CommandSummary>> {
	let root = parse_cli_root(config_text)?;
	Ok(root
		.cli
		.into_iter()
		.map(|(name, command)| {
			CommandSummary {
				name,
				help_text: command.help_text,
				step_count: command.steps.len(),
			}
		})
		.collect())
}

pub(crate) fn upsert_cli_command_document(
	config_text: &str,
	update: &CommandUpdate,
) -> MonochangeResult<String> {
	validate_command_update(update)?;

	let mut document = parse_document(config_text)?;
	let cli = ensure_cli_table(&mut document)?;
	let original_name = update.original_name.as_deref();
	let is_rename = original_name.is_some_and(|name| name != update.name);
	let has_existing_target = cli.contains_key(&update.name);

	if original_name.is_none() && has_existing_target {
		return Err(config_error(format!(
			"CLI command `{}` already exists; choose edit from the dashboard instead",
			update.name
		)));
	}

	if is_rename && has_existing_target {
		return Err(config_error(format!(
			"CLI command `{}` already exists; choose a different command name",
			update.name
		)));
	}

	if let Some(original_name) = original_name
		&& original_name != update.name
	{
		let existing = cli
			.remove(original_name)
			.ok_or_else(|| config_error(format!("CLI command `{original_name}` does not exist")))?;
		cli.insert(&update.name, existing);
	}

	if !cli.contains_key(&update.name) {
		cli.insert(&update.name, Item::Table(Table::new()));
	}

	let command = cli
		.get_mut(&update.name)
		.and_then(Item::as_table_mut)
		.ok_or_else(|| config_error(format!("[cli.{}] must be a TOML table", update.name)))?;
	write_command_fields(command, update)?;

	Ok(render_document(&document))
}

#[coverage(off)]
fn create_command(
	root: &Path,
	config_path: &Path,
	config_text: &str,
	summaries: &[CommandSummary],
) -> MonochangeResult<String> {
	let update = prompt_command_update(None, summaries)?;
	let rendered = upsert_cli_command_document(config_text, &update)?;
	write_config_text(config_path, &rendered)?;
	let relative_path = config_path.strip_prefix(root).unwrap_or(config_path);

	Ok(format!(
		"updated {} and added CLI command `{}`",
		relative_path.display(),
		update.name
	))
}

#[coverage(off)]
fn edit_command(
	root: &Path,
	config_path: &Path,
	config_text: &str,
	summaries: &[CommandSummary],
) -> MonochangeResult<String> {
	if summaries.is_empty() {
		return Err(config_error(
			"monochange.toml does not define any [cli.<name>] commands yet",
		));
	}

	let selected = Select::new("Choose a command to edit:", summaries.to_owned())
		.prompt()
		.map_err(map_inquire_error)?;
	let details = read_cli_command(config_text, &selected.name)?
		.ok_or_else(|| config_error(format!("CLI command `{}` does not exist", selected.name)))?;
	let update = prompt_command_update(Some(&details), summaries)?;
	let rendered = upsert_cli_command_document(config_text, &update)?;
	write_config_text(config_path, &rendered)?;
	let relative_path = config_path.strip_prefix(root).unwrap_or(config_path);

	if update.original_name.as_deref() == Some(update.name.as_str()) {
		Ok(format!(
			"updated {} and edited CLI command `{}`",
			relative_path.display(),
			update.name
		))
	} else {
		Ok(format!(
			"updated {} and renamed CLI command `{}` to `{}`",
			relative_path.display(),
			update.original_name.as_deref().unwrap_or_default(),
			update.name
		))
	}
}

#[coverage(off)]
fn prompt_command_update(
	existing: Option<&CommandDetails>,
	summaries: &[CommandSummary],
) -> MonochangeResult<CommandUpdate> {
	let existing_names = summaries
		.iter()
		.map(|summary| summary.name.as_str())
		.collect::<Vec<_>>();
	let original_name = existing.map(|command| command.name.as_str());
	let name = prompt_command_name(original_name, &existing_names)?;
	let help_text = prompt_help_text(existing.and_then(|command| command.help_text.as_deref()))?;
	let dry_run = Confirm::new("Run this command in dry-run mode by default?")
		.with_default(existing.is_some_and(|command| command.dry_run))
		.prompt()
		.map_err(map_inquire_error)?;
	let inputs = prompt_command_inputs(existing)?;
	let steps = prompt_step_update(existing)?;

	Ok(CommandUpdate {
		original_name: original_name.map(str::to_string),
		name,
		help_text,
		dry_run,
		inputs,
		steps,
	})
}

#[coverage(off)]
fn prompt_command_name(
	original_name: Option<&str>,
	existing_names: &[&str],
) -> MonochangeResult<String> {
	let original_name_for_validator = original_name.map(str::to_string);
	let existing_names_for_validator = existing_names
		.iter()
		.map(|name| (*name).to_string())
		.collect::<Vec<_>>();
	let mut prompt =
		Text::new("Command name (for [cli.<name>]):").with_validator(move |input: &str| {
			match validate_command_name_for_prompt(
				input,
				original_name_for_validator.as_deref(),
				&existing_names_for_validator,
			) {
				Ok(()) => Ok(Validation::Valid),
				Err(message) => Ok(Validation::Invalid(message.into())),
			}
		});

	if let Some(original_name) = original_name {
		prompt = prompt.with_initial_value(original_name);
	}

	Ok(prompt
		.prompt()
		.map_err(map_inquire_error)?
		.trim()
		.to_string())
}

#[coverage(off)]
fn prompt_help_text(initial: Option<&str>) -> MonochangeResult<Option<String>> {
	let mut prompt = Text::new("Help text (blank to omit):");
	if let Some(initial) = initial {
		prompt = prompt.with_initial_value(initial);
	}
	let help_text = prompt.prompt().map_err(map_inquire_error)?;

	Ok(normalize_optional_text(&help_text))
}

#[coverage(off)]
fn prompt_step_update(existing: Option<&CommandDetails>) -> MonochangeResult<CommandStepUpdate> {
	if let Some(existing) = existing
		&& !existing.steps.is_empty()
	{
		let step_summary = existing
			.steps
			.iter()
			.map(step_label)
			.collect::<Vec<_>>()
			.join(", ");
		let keep_existing = Confirm::new(&format!("Keep existing steps ({step_summary})?"))
			.with_default(true)
			.prompt()
			.map_err(map_inquire_error)?;

		if keep_existing {
			return Ok(CommandStepUpdate::KeepExisting);
		}
	}

	Ok(CommandStepUpdate::Replace(prompt_replacement_steps()?))
}

#[coverage(off)]
fn prompt_replacement_steps() -> MonochangeResult<Vec<CommandStepDraft>> {
	let mut steps = Vec::new();

	loop {
		let selected = Select::new(
			"Step dashboard — add the next step or save:",
			step_choices(),
		)
		.with_scorer(&step_choice_scorer)
		.with_sorter(&step_choice_sorter)
		.with_formatter(&|answer| answer.value.kind.clone())
		.prompt()
		.map_err(map_inquire_error)?;

		if selected.is_save {
			if steps.is_empty() {
				return Err(config_error("a CLI command needs at least one step"));
			}
			return Ok(steps);
		}

		if selected.kind == STEP_KIND_SHELL_COMMAND {
			steps.push(prompt_shell_command_step()?);
		} else {
			steps.push(CommandStepDraft::built_in(selected.kind));
		}
	}
}

#[coverage(off)]
fn prompt_shell_command_step() -> MonochangeResult<CommandStepDraft> {
	let command = Text::new("Shell command to run:")
		.with_validator(|input: &str| {
			if input.trim().is_empty() {
				Ok(Validation::Invalid("command cannot be empty".into()))
			} else {
				Ok(Validation::Valid)
			}
		})
		.prompt()
		.map_err(map_inquire_error)?;
	let step_name = Text::new("Step display name (blank to omit):")
		.prompt()
		.map_err(map_inquire_error)?;
	let name = normalize_optional_text(&step_name);

	Ok(CommandStepDraft::shell_command(
		command.trim().to_string(),
		name,
	))
}

#[coverage(off)]
fn prompt_command_inputs(
	existing: Option<&CommandDetails>,
) -> MonochangeResult<Vec<CommandInputDraft>> {
	if let Some(existing) = existing
		&& !existing.inputs.is_empty()
	{
		let input_summary = existing
			.inputs
			.iter()
			.map(command_input_label)
			.collect::<Vec<_>>()
			.join(", ");
		let keep_existing =
			Confirm::new(&format!("Keep existing command inputs ({input_summary})?"))
				.with_default(true)
				.prompt()
				.map_err(map_inquire_error)?;

		if keep_existing {
			return Ok(existing.inputs.clone());
		}
	}

	let add_inputs = Confirm::new("Add or replace top-level command inputs?")
		.with_default(false)
		.prompt()
		.map_err(map_inquire_error)?;
	if !add_inputs {
		return Ok(Vec::new());
	}

	prompt_replacement_inputs()
}

#[coverage(off)]
fn prompt_replacement_inputs() -> MonochangeResult<Vec<CommandInputDraft>> {
	let mut inputs = Vec::new();

	loop {
		let input = prompt_command_input(&inputs)?;
		inputs.push(input);

		let add_another = Confirm::new("Add another command input?")
			.with_default(false)
			.prompt()
			.map_err(map_inquire_error)?;
		if !add_another {
			return Ok(inputs);
		}
	}
}

#[coverage(off)]
fn prompt_command_input(existing: &[CommandInputDraft]) -> MonochangeResult<CommandInputDraft> {
	let existing_names = existing
		.iter()
		.map(|input| input.name.clone())
		.collect::<Vec<_>>();
	let name = Text::new("Input name (for --<name>):")
		.with_validator(move |input: &str| {
			match validate_command_input_name_for_prompt(input, &existing_names) {
				Ok(()) => Ok(Validation::Valid),
				Err(message) => Ok(Validation::Invalid(message.into())),
			}
		})
		.prompt()
		.map_err(map_inquire_error)?
		.trim()
		.to_string();
	let kind = Select::new("Input type:", command_input_kind_choices())
		.prompt()
		.map_err(map_inquire_error)?;
	let help_text = prompt_help_text(None)?;
	let required = Confirm::new("Require this input?")
		.with_default(false)
		.prompt()
		.map_err(map_inquire_error)?;
	let default = prompt_command_input_default(kind.as_str())?;
	let choices = prompt_command_input_choices(kind.as_str())?;
	let short = prompt_command_input_short()?;

	Ok(CommandInputDraft {
		name,
		kind,
		help_text,
		required,
		default,
		choices,
		short,
	})
}

#[coverage(off)]
fn prompt_command_input_default(kind: &str) -> MonochangeResult<Option<String>> {
	if kind == "string_list" {
		return Ok(None);
	}

	let default = Text::new("Default value (blank to omit):")
		.prompt()
		.map_err(map_inquire_error)?;
	Ok(normalize_optional_text(&default))
}

#[coverage(off)]
fn prompt_command_input_choices(kind: &str) -> MonochangeResult<Vec<String>> {
	if kind != "choice" {
		return Ok(Vec::new());
	}

	let choices = Text::new("Choices (comma-separated):")
		.with_validator(|input: &str| {
			if comma_separated_values(input).is_empty() {
				Ok(Validation::Invalid(
					"choice inputs need at least one choice".into(),
				))
			} else {
				Ok(Validation::Valid)
			}
		})
		.prompt()
		.map_err(map_inquire_error)?;
	Ok(comma_separated_values(&choices))
}

#[coverage(off)]
fn prompt_command_input_short() -> MonochangeResult<Option<char>> {
	let short = Text::new("Short flag character (blank to omit):")
		.with_validator(|input: &str| {
			match normalize_short_flag(input) {
				Ok(_) => Ok(Validation::Valid),
				Err(message) => Ok(Validation::Invalid(message.into())),
			}
		})
		.prompt()
		.map_err(map_inquire_error)?;
	normalize_short_flag(&short).map_err(config_error)
}

fn dashboard_actions(has_commands: bool) -> Vec<DashboardAction> {
	let mut actions = vec![DashboardAction::AddCommand];
	if has_commands {
		actions.push(DashboardAction::EditCommand);
	}
	actions.push(DashboardAction::OpenEditor);
	actions.push(DashboardAction::Quit);
	actions
}

fn read_cli_command(config_text: &str, name: &str) -> MonochangeResult<Option<CommandDetails>> {
	let root = parse_cli_root(config_text)?;
	Ok(root.cli.get(name).map(|command| {
		CommandDetails {
			name: name.to_string(),
			help_text: command.help_text.clone(),
			dry_run: command.dry_run,
			inputs: command.inputs.clone(),
			steps: command
				.steps
				.iter()
				.map(|step| {
					CommandStepDraft {
						kind: step.kind.clone(),
						name: step.name.clone(),
						command: step.command.clone(),
					}
				})
				.collect(),
		}
	}))
}

fn write_command_fields(command: &mut Table, update: &CommandUpdate) -> MonochangeResult<()> {
	match &update.help_text {
		Some(help_text) => {
			command["help_text"] = value(help_text.as_str());
		}
		None => {
			command.remove("help_text");
		}
	}

	if update.dry_run {
		command["dry_run"] = value(true);
	} else {
		command.remove("dry_run");
	}

	if update.inputs.is_empty() {
		command.remove("inputs");
	} else {
		command["inputs"] = Item::Value(Value::Array(command_input_array(&update.inputs)?));
	}

	if let CommandStepUpdate::Replace(steps) = &update.steps {
		command["steps"] = Item::Value(Value::Array(step_array(steps)?));
	}

	Ok(())
}

fn command_input_array(inputs: &[CommandInputDraft]) -> MonochangeResult<Array> {
	let mut array = Array::new();
	for input in inputs {
		array.push(Value::InlineTable(command_input_inline_table(input)?));
	}
	Ok(array)
}

fn command_input_inline_table(input: &CommandInputDraft) -> MonochangeResult<InlineTable> {
	validate_command_input_draft(input)?;
	let mut table = InlineTable::new();
	table.insert("name", Value::from(input.name.as_str()));
	table.insert("type", Value::from(input.kind.as_str()));

	if let Some(help_text) = &input.help_text {
		table.insert("help_text", Value::from(help_text.as_str()));
	}
	if input.required {
		table.insert("required", Value::from(true));
	}
	if let Some(default) = &input.default {
		table.insert("default", Value::from(default.as_str()));
	}
	if !input.choices.is_empty() {
		table.insert("choices", Value::Array(string_array(&input.choices)));
	}
	if let Some(short) = input.short {
		let short = short.to_string();
		table.insert("short", Value::from(short.as_str()));
	}

	Ok(table)
}

fn step_array(steps: &[CommandStepDraft]) -> MonochangeResult<Array> {
	let mut array = Array::new();
	for step in steps {
		array.push(Value::InlineTable(step_inline_table(step)?));
	}
	Ok(array)
}

fn step_inline_table(step: &CommandStepDraft) -> MonochangeResult<InlineTable> {
	validate_step_draft(step)?;
	let mut table = InlineTable::new();
	table.insert("type", Value::from(step.kind.as_str()));

	if let Some(name) = &step.name {
		table.insert("name", Value::from(name.as_str()));
	}

	if let Some(command) = &step.command {
		table.insert("command", Value::from(command.as_str()));
	}

	Ok(table)
}

fn step_choices() -> Vec<StepChoice> {
	let mut choices = monochange_core::all_step_variants()
		.into_iter()
		.map(|step| StepChoice::new(step.kind_name()))
		.collect::<Vec<_>>();
	if !choices
		.iter()
		.any(|choice| choice.kind == STEP_KIND_SHELL_COMMAND)
	{
		choices.push(StepChoice::new(STEP_KIND_SHELL_COMMAND));
	}
	choices.sort_by_key(|choice| unfiltered_step_choice_rank(&choice.kind));
	choices.push(StepChoice::save());
	choices
}

fn step_choice_scorer(
	input: &str,
	option: &StepChoice,
	_display: &str,
	_index: usize,
) -> Option<i64> {
	let input = input.trim().to_lowercase();
	if input.is_empty() {
		return Some(STEP_SCORE_BASE - unfiltered_step_choice_rank(&option.kind) as i64);
	}

	let searchable = format!("{} {}", option.kind, option.description).to_lowercase();
	if !searchable.contains(&input) {
		return None;
	}

	Some(STEP_SCORE_BASE - filtered_step_choice_rank(&option.kind) as i64)
}

fn step_choice_sorter(options: &mut [(usize, i64)]) {
	options.sort_unstable_by_key(|(_index, score)| std::cmp::Reverse(*score));
}

fn unfiltered_step_choice_rank(kind: &str) -> usize {
	match kind {
		"PrepareRelease" => 0,
		STEP_KIND_SHELL_COMMAND => 1,
		"CreateChangeFile" => 2,
		"Validate" => 3,
		"Discover" => 4,
		"DisplayVersions" => 5,
		"CommitRelease" => 6,
		"PublishRelease" => 7,
		"PublishPackages" => 8,
		"OpenReleaseRequest" => 9,
		"AffectedPackages" => 10,
		"Config" => 11,
		"VerifyReleaseBranch" => 12,
		"PlanPublishRateLimits" => 13,
		"PlaceholderPublish" => 14,
		"CommentReleasedIssues" => 15,
		"DiagnoseChangesets" => 16,
		"RetargetRelease" => 17,
		SAVE_STEPS_LABEL => usize::MAX,
		_ => 100 + filtered_step_choice_rank(kind),
	}
}

fn filtered_step_choice_rank(kind: &str) -> usize {
	let mut labels = monochange_core::all_step_variants()
		.into_iter()
		.map(|step| step.kind_name().to_string())
		.collect::<Vec<_>>();
	if !labels.iter().any(|label| label == STEP_KIND_SHELL_COMMAND) {
		labels.push(STEP_KIND_SHELL_COMMAND.to_string());
	}
	labels.push(SAVE_STEPS_LABEL.to_string());
	labels.sort();
	labels
		.iter()
		.position(|label| label == kind)
		.unwrap_or(labels.len())
}

fn step_choice_description(kind: &str) -> &'static str {
	match kind {
		"Config" => "Load workspace configuration for later steps",
		"Validate" => "Check config, changesets, and package manifests",
		"Discover" => "List packages across supported ecosystems",
		"DisplayVersions" => "Show planned package and group versions",
		"CreateChangeFile" => "Create an interactive or prefilled changeset",
		"PrepareRelease" => "Plan a release and expose release context",
		"CommitRelease" => "Commit prepared release files locally",
		"VerifyReleaseBranch" => "Ensure a release commit is on an allowed branch",
		"PublishRelease" => "Create or update hosted releases",
		"PlaceholderPublish" => "Publish placeholder versions for missing packages",
		"PublishPackages" => "Publish prepared package artifacts",
		"PlanPublishRateLimits" => "Group publish work around registry rate limits",
		"OpenReleaseRequest" => "Open or update a release pull request",
		"CommentReleasedIssues" => "Comment on issues included in a release",
		"AffectedPackages" => "Report packages affected by changed files",
		"DiagnoseChangesets" => "Explain changeset and release-plan decisions",
		"RetargetRelease" => "Retarget an existing release to another commit",
		STEP_KIND_SHELL_COMMAND => "Run a custom shell command",
		SAVE_STEPS_LABEL => "Finish the command after adding at least one step",
		_ => "Add this CLI step",
	}
}

fn step_label(step: &CommandStepDraft) -> String {
	match (&step.name, &step.command) {
		(Some(name), _) => format!("{} ({name})", step.kind),
		(None, Some(command)) => format!("{} ({command})", step.kind),
		(None, None) => step.kind.clone(),
	}
}

fn validate_command_update(update: &CommandUpdate) -> MonochangeResult<()> {
	validate_command_name(&update.name)?;
	if let Some(original_name) = &update.original_name {
		validate_command_name(original_name)?;
	}

	validate_command_inputs(&update.inputs)?;

	match &update.steps {
		CommandStepUpdate::KeepExisting => {}
		CommandStepUpdate::Replace(steps) => {
			if steps.is_empty() {
				return Err(config_error("a CLI command needs at least one step"));
			}
			for step in steps {
				validate_step_draft(step)?;
			}
		}
	}

	Ok(())
}

pub(crate) fn validate_command_name(name: &str) -> MonochangeResult<()> {
	validate_command_name_message(name).map_err(config_error)
}

fn validate_command_name_for_prompt(
	name: &str,
	original_name: Option<&str>,
	existing_names: &[String],
) -> Result<(), String> {
	validate_command_name_message(name)?;
	let name = name.trim();
	if original_name != Some(name) && existing_names.iter().any(|existing| existing == name) {
		return Err(format!("CLI command `{name}` already exists"));
	}
	Ok(())
}

fn validate_command_name_message(name: &str) -> Result<(), String> {
	let trimmed = name.trim();
	if trimmed.is_empty() {
		return Err("command name cannot be empty".to_string());
	}
	if name != trimmed {
		return Err("command name cannot include leading or trailing whitespace".to_string());
	}
	let name = trimmed;
	if monochange_config::RESERVED_CLI_COMMAND_NAMES.contains(&name) {
		return Err(format!(
			"CLI command `{name}` collides with a reserved built-in command"
		));
	}
	if name.starts_with("step:") {
		return Err("command names cannot start with `step:`".to_string());
	}
	if !name
		.bytes()
		.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
	{
		return Err("use lowercase letters, digits, and hyphens only, e.g. release-pr".to_string());
	}
	if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
		return Err("hyphens must separate words, e.g. release-pr".to_string());
	}
	Ok(())
}

fn validate_command_inputs(inputs: &[CommandInputDraft]) -> MonochangeResult<()> {
	let mut names = BTreeSet::new();
	let mut shorts = BTreeSet::new();

	for input in inputs {
		validate_command_input_draft(input)?;
		if !names.insert(input.name.as_str()) {
			return Err(config_error(format!(
				"duplicate CLI input `{}`; input names must be unique",
				input.name
			)));
		}
		if let Some(short) = input.short
			&& !shorts.insert(short)
		{
			return Err(config_error(format!(
				"duplicate CLI input short flag `{short}`; short flags must be unique"
			)));
		}
	}

	Ok(())
}

fn validate_command_input_draft(input: &CommandInputDraft) -> MonochangeResult<()> {
	validate_command_input_name(&input.name)?;
	if !command_input_kind_is_known(&input.kind) {
		return Err(config_error(format!(
			"unknown CLI input type `{}`",
			input.kind
		)));
	}
	if input.kind != "choice" && !input.choices.is_empty() {
		return Err(config_error(format!(
			"only `choice` inputs can define choices (input `{}`)",
			input.name
		)));
	}
	if input.kind == "choice" && input.choices.is_empty() {
		return Err(config_error(format!(
			"choice input `{}` needs at least one choice",
			input.name
		)));
	}
	if input.kind == "string_list" && input.default.is_some() {
		return Err(config_error(format!(
			"string_list input `{}` cannot define a scalar default",
			input.name
		)));
	}
	if let Some(short) = input.short
		&& !short.is_ascii_alphanumeric()
	{
		return Err(config_error(format!(
			"input `{}` short flag must be an ASCII letter or digit",
			input.name
		)));
	}
	Ok(())
}

fn validate_command_input_name(name: &str) -> MonochangeResult<()> {
	validate_command_input_name_message(name).map_err(config_error)
}

fn validate_command_input_name_for_prompt(
	name: &str,
	existing_names: &[String],
) -> Result<(), String> {
	validate_command_input_name_message(name)?;
	let name = name.trim();
	if existing_names.iter().any(|existing| existing == name) {
		return Err(format!("CLI input `{name}` already exists"));
	}
	Ok(())
}

fn validate_command_input_name_message(name: &str) -> Result<(), String> {
	let trimmed = name.trim();
	if trimmed.is_empty() {
		return Err("input name cannot be empty".to_string());
	}
	if name != trimmed {
		return Err("input name cannot include leading or trailing whitespace".to_string());
	}
	if !trimmed
		.bytes()
		.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
	{
		return Err(
			"use lowercase letters, digits, and hyphens only, e.g. release-type".to_string(),
		);
	}
	if trimmed.starts_with('-') || trimmed.ends_with('-') || trimmed.contains("--") {
		return Err("hyphens must separate words, e.g. release-type".to_string());
	}
	Ok(())
}

fn command_input_kind_is_known(kind: &str) -> bool {
	command_input_kind_choices()
		.iter()
		.any(|choice| choice == kind)
}

fn command_input_kind_choices() -> Vec<String> {
	["string", "string_list", "path", "choice", "boolean"]
		.into_iter()
		.map(str::to_string)
		.collect()
}

fn comma_separated_values(input: &str) -> Vec<String> {
	input
		.split(',')
		.filter_map(normalize_optional_text)
		.collect()
}

fn normalize_short_flag(input: &str) -> Result<Option<char>, String> {
	let Some(value) = normalize_optional_text(input) else {
		return Ok(None);
	};
	let mut chars = value.chars();
	let short = chars
		.next()
		.expect("normalize_optional_text returns non-empty strings");
	if chars.next().is_some() {
		return Err("short flag must be exactly one character".to_string());
	}
	if !short.is_ascii_alphanumeric() {
		return Err("short flag must be an ASCII letter or digit".to_string());
	}
	Ok(Some(short))
}

fn command_input_label(input: &CommandInputDraft) -> String {
	let required = if input.required { ", required" } else { "" };
	format!("{} ({}{required})", input.name, input.kind)
}

fn string_array(values: &[String]) -> Array {
	let mut array = Array::new();
	for value in values {
		array.push(value.as_str());
	}
	array
}

fn validate_step_draft(step: &CommandStepDraft) -> MonochangeResult<()> {
	if !step_kind_is_known(&step.kind) {
		return Err(config_error(format!(
			"unknown CLI step type `{}`",
			step.kind
		)));
	}
	if step.kind == STEP_KIND_SHELL_COMMAND {
		let command = step.command.as_deref().unwrap_or_default().trim();
		if command.is_empty() {
			return Err(config_error(
				"Command steps need a non-empty `command` value",
			));
		}
	} else if step.command.is_some() {
		return Err(config_error(format!(
			"only `{STEP_KIND_SHELL_COMMAND}` steps can define `command`"
		)));
	}
	Ok(())
}

fn step_kind_is_known(kind: &str) -> bool {
	kind == STEP_KIND_SHELL_COMMAND
		|| monochange_core::all_step_variants()
			.into_iter()
			.any(|step| step.kind_name() == kind)
}

fn ensure_cli_table(document: &mut DocumentMut) -> MonochangeResult<&mut Table> {
	if !document.as_table().contains_key("cli") {
		document["cli"] = Item::Table(Table::new());
	}
	document["cli"]
		.as_table_mut()
		.ok_or_else(|| config_error("[cli] must be a TOML table"))
}

fn parse_cli_root(config_text: &str) -> MonochangeResult<RawCliRoot> {
	toml::from_str(config_text).map_err(|error| config_error(error.to_string()))
}

fn parse_document(config_text: &str) -> MonochangeResult<DocumentMut> {
	config_text
		.parse::<DocumentMut>()
		.map_err(|error| config_error(error.to_string()))
}

fn render_document(document: &DocumentMut) -> String {
	let mut rendered = document.to_string();
	if !rendered.ends_with('\n') {
		rendered.push('\n');
	}
	rendered
}

fn read_config_text(config_path: &Path) -> MonochangeResult<String> {
	match fs::read_to_string(config_path) {
		Ok(contents) => Ok(contents),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
		Err(error) => {
			Err(config_error(format!(
				"failed to read {}: {error}",
				config_path.display()
			)))
		}
	}
}

fn write_config_text(config_path: &Path, contents: &str) -> MonochangeResult<()> {
	fs::write(config_path, contents).map_err(|error| {
		config_error(format!(
			"failed to write {}: {error}",
			config_path.display()
		))
	})
}

#[coverage(off)]
fn open_config_in_editor(config_path: &Path) -> MonochangeResult<()> {
	if !config_path.exists() {
		write_config_text(config_path, "")?;
	}

	let editor = std::env::var("VISUAL")
		.or_else(|_| std::env::var("EDITOR"))
		.map_err(|_| config_error("set $VISUAL or $EDITOR before choosing the editor action"))?;
	let mut command_parts = shlex::split(&editor)
		.filter(|parts| !parts.is_empty())
		.ok_or_else(|| config_error("$VISUAL/$EDITOR could not be parsed as a shell command"))?;
	let program = command_parts.remove(0);
	let status = ProcessCommand::new(program)
		.args(command_parts)
		.arg(config_path)
		.status()
		.map_err(|error| config_error(format!("failed to start editor: {error}")))?;

	if status.success() {
		Ok(())
	} else {
		Err(config_error(format!("editor exited with status {status}")))
	}
}

fn normalize_optional_text(value: &str) -> Option<String> {
	let trimmed = value.trim();
	if trimmed.is_empty() {
		None
	} else {
		Some(trimmed.to_string())
	}
}

#[coverage(off)]
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

fn config_error(message: impl Into<String>) -> MonochangeError {
	MonochangeError::Config(message.into())
}

#[cfg(test)]
#[path = "__tests__/command_wizard_tests.rs"]
mod tests;
