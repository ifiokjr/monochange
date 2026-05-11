use tempfile::tempdir;
use toml::Value;
use toml_edit::DocumentMut;

use super::CommandStepDraft;
use super::CommandStepUpdate;
use super::CommandUpdate;
use super::DashboardAction;
use super::SAVE_STEPS_LABEL;
use super::STEP_KIND_SHELL_COMMAND;
use super::cli_command_summaries;
use super::dashboard_actions;
use super::normalize_optional_text;
use super::read_cli_command;
use super::read_config_text;
use super::render_document;
use super::step_choices;
use super::step_label;
use super::upsert_cli_command_document;
use super::validate_command_name;
use super::validate_command_name_for_prompt;
use super::validate_step_draft;
use super::write_config_text;

#[test]
fn upsert_cli_command_document_adds_command_to_empty_config() {
	let update = CommandUpdate {
		original_name: None,
		name: "ship-it".to_string(),
		help_text: Some("Ship the release".to_string()),
		dry_run: false,
		steps: CommandStepUpdate::Replace(vec![CommandStepDraft::built_in("Discover")]),
	};

	let rendered = render_update("", &update);
	let value = parse_rendered_toml(&rendered);
	let command = value
		.get("cli")
		.and_then(|cli| cli.get("ship-it"))
		.and_then(Value::as_table)
		.unwrap_or_else(|| panic!("command table should exist"));

	assert_eq!(
		command.get("help_text").and_then(Value::as_str),
		Some("Ship the release")
	);
	assert!(!command.contains_key("dry_run"));
	let steps = command
		.get("steps")
		.and_then(Value::as_array)
		.unwrap_or_else(|| panic!("steps should be an array"));
	assert_eq!(
		steps[0].get("type").and_then(Value::as_str),
		Some("Discover")
	);
}

#[test]
fn upsert_cli_command_document_edits_existing_command_without_replacing_steps() {
	let config = r#"
[cli.release]
help_text = "Old help"
dry_run = true
steps = [{ type = "PrepareRelease", allow_empty_changesets = true }]
"#;
	let update = CommandUpdate {
		original_name: Some("release".to_string()),
		name: "release".to_string(),
		help_text: Some("New help".to_string()),
		dry_run: false,
		steps: CommandStepUpdate::KeepExisting,
	};

	let rendered = render_update(config, &update);
	let value = parse_rendered_toml(&rendered);
	let command = value["cli"]["release"]
		.as_table()
		.unwrap_or_else(|| panic!("command table should exist"));

	assert_eq!(command["help_text"].as_str(), Some("New help"));
	assert!(!command.contains_key("dry_run"));
	assert_eq!(
		command["steps"][0]["allow_empty_changesets"].as_bool(),
		Some(true)
	);
}

#[test]
fn upsert_cli_command_document_renames_existing_command() {
	let config = r#"
[cli.old-name]
steps = [{ type = "Validate" }]

[cli.keep]
steps = [{ type = "Discover" }]
"#;
	let update = CommandUpdate {
		original_name: Some("old-name".to_string()),
		name: "new-name".to_string(),
		help_text: None,
		dry_run: false,
		steps: CommandStepUpdate::KeepExisting,
	};

	let rendered = render_update(config, &update);
	let value = parse_rendered_toml(&rendered);
	let cli = value["cli"]
		.as_table()
		.unwrap_or_else(|| panic!("cli table should exist"));

	assert!(!cli.contains_key("old-name"));
	assert!(cli.contains_key("new-name"));
	assert!(cli.contains_key("keep"));
	assert_eq!(
		cli["new-name"]["steps"][0]["type"].as_str(),
		Some("Validate")
	);
}

#[test]
fn upsert_cli_command_document_writes_shell_command_steps() {
	let update = CommandUpdate {
		original_name: None,
		name: "lockfiles".to_string(),
		help_text: None,
		dry_run: true,
		steps: CommandStepUpdate::Replace(vec![CommandStepDraft::shell_command(
			"pnpm install --lockfile-only".to_string(),
			Some("generate lockfiles".to_string()),
		)]),
	};

	let rendered = render_update("", &update);
	let value = parse_rendered_toml(&rendered);
	let command = value["cli"]["lockfiles"]
		.as_table()
		.unwrap_or_else(|| panic!("command table should exist"));
	let step = &command["steps"][0];

	assert_eq!(command["dry_run"].as_bool(), Some(true));
	assert_eq!(step["type"].as_str(), Some("Command"));
	assert_eq!(step["name"].as_str(), Some("generate lockfiles"));
	assert_eq!(
		step["command"].as_str(),
		Some("pnpm install --lockfile-only")
	);
}

#[test]
fn cli_command_summaries_returns_configured_commands() {
	let config = r#"
[cli.discover]
help_text = "Discover packages"
steps = [{ type = "Discover" }]

[cli.release-pr]
steps = [{ type = "PrepareRelease" }, { type = "OpenReleaseRequest" }]
"#;

	let summaries = cli_command_summaries(config)
		.unwrap_or_else(|error| panic!("commands should parse: {error}"));
	let labels = summaries
		.into_iter()
		.map(|summary| summary.to_string())
		.collect::<Vec<_>>();

	assert_eq!(
		labels,
		vec![
			"discover — Discover packages".to_string(),
			"release-pr — 2 steps".to_string(),
		]
	);
}

#[test]
fn dashboard_actions_offer_edit_only_when_commands_exist() {
	assert_eq!(
		dashboard_actions(false),
		vec![
			DashboardAction::AddCommand,
			DashboardAction::OpenEditor,
			DashboardAction::Quit,
		]
	);
	assert_eq!(
		dashboard_actions(true),
		vec![
			DashboardAction::AddCommand,
			DashboardAction::EditCommand,
			DashboardAction::OpenEditor,
			DashboardAction::Quit,
		]
	);
	assert_eq!(
		DashboardAction::AddCommand.to_string(),
		"Add a new [cli.<name>] command"
	);
	assert_eq!(
		DashboardAction::EditCommand.to_string(),
		"Edit an existing command"
	);
	assert_eq!(
		DashboardAction::OpenEditor.to_string(),
		"Open monochange.toml in $VISUAL/$EDITOR"
	);
	assert_eq!(DashboardAction::Quit.to_string(), "Quit without changes");
}

#[test]
fn read_cli_command_returns_details_for_existing_command() {
	let config = r#"
[cli.lockfiles]
help_text = "Generate lockfiles"
dry_run = true
steps = [{ type = "Command", name = "install", command = "pnpm install --lockfile-only" }]
"#;

	let command = read_cli_command(config, "lockfiles")
		.unwrap_or_else(|error| panic!("command should parse: {error}"))
		.unwrap_or_else(|| panic!("command should exist"));
	let missing = read_cli_command(config, "missing")
		.unwrap_or_else(|error| panic!("command lookup should parse: {error}"));

	assert!(missing.is_none());
	assert_eq!(command.name, "lockfiles");
	assert_eq!(command.help_text.as_deref(), Some("Generate lockfiles"));
	assert!(command.dry_run);
	assert_eq!(command.steps.len(), 1);
	assert_eq!(command.steps[0].kind, STEP_KIND_SHELL_COMMAND);
	assert_eq!(command.steps[0].name.as_deref(), Some("install"));
	assert_eq!(
		command.steps[0].command.as_deref(),
		Some("pnpm install --lockfile-only")
	);
}

#[test]
fn step_choices_and_labels_include_shell_command_and_save_action() {
	let choices = step_choices();

	assert!(
		choices
			.iter()
			.any(|choice| choice == STEP_KIND_SHELL_COMMAND)
	);
	assert_eq!(choices.last().map(String::as_str), Some(SAVE_STEPS_LABEL));
	assert_eq!(
		step_label(&CommandStepDraft {
			kind: "Validate".to_string(),
			name: Some("lint".to_string()),
			command: None,
		}),
		"Validate (lint)"
	);
	assert_eq!(
		step_label(&CommandStepDraft {
			kind: STEP_KIND_SHELL_COMMAND.to_string(),
			name: None,
			command: Some("cargo test".to_string()),
		}),
		"Command (cargo test)"
	);
	assert_eq!(
		step_label(&CommandStepDraft::built_in("Discover")),
		"Discover"
	);
}

#[test]
fn command_name_validation_reports_prompt_and_shape_errors() {
	assert!(validate_command_name("release-pr").is_ok());

	for (name, expected) in [
		("", "command name cannot be empty"),
		(
			" release",
			"command name cannot include leading or trailing whitespace",
		),
		("step:release", "command names cannot start with `step:`"),
		("-release", "hyphens must separate words"),
	] {
		let error = validate_command_name(name)
			.err()
			.unwrap_or_else(|| panic!("{name:?} should be invalid"));
		assert!(
			error.to_string().contains(expected),
			"expected {error} to contain {expected}"
		);
	}

	let existing_names = vec!["release".to_string()];
	let duplicate = validate_command_name_for_prompt("release", None, &existing_names)
		.err()
		.unwrap_or_else(|| panic!("duplicate command should be rejected"));
	assert_eq!(duplicate, "CLI command `release` already exists");
	assert!(validate_command_name_for_prompt("release", Some("release"), &existing_names).is_ok());
}

#[test]
fn validate_step_draft_rejects_unknown_or_misconfigured_steps() {
	assert_config_error(
		validate_step_draft(&CommandStepDraft::built_in("NotARealStep")),
		"unknown CLI step type `NotARealStep`",
	);
	assert_config_error(
		validate_step_draft(&CommandStepDraft {
			kind: STEP_KIND_SHELL_COMMAND.to_string(),
			name: None,
			command: Some("   ".to_string()),
		}),
		"Command steps need a non-empty `command` value",
	);
	assert_config_error(
		validate_step_draft(&CommandStepDraft {
			kind: "Discover".to_string(),
			name: None,
			command: Some("cargo test".to_string()),
		}),
		"only `Command` steps can define `command`",
	);
}

#[test]
fn upsert_cli_command_document_rejects_create_and_rename_conflicts() {
	let config = r#"
[cli.release]
steps = [{ type = "Validate" }]

[cli.deploy]
steps = [{ type = "Discover" }]
"#;
	let duplicate_add = CommandUpdate {
		original_name: None,
		name: "release".to_string(),
		help_text: None,
		dry_run: false,
		steps: CommandStepUpdate::Replace(vec![CommandStepDraft::built_in("Validate")]),
	};
	let duplicate_rename = CommandUpdate {
		original_name: Some("release".to_string()),
		name: "deploy".to_string(),
		help_text: None,
		dry_run: false,
		steps: CommandStepUpdate::KeepExisting,
	};

	assert_config_error(
		upsert_cli_command_document(config, &duplicate_add).map(|_| ()),
		"CLI command `release` already exists; choose edit from the dashboard instead",
	);
	assert_config_error(
		upsert_cli_command_document(config, &duplicate_rename).map(|_| ()),
		"CLI command `deploy` already exists; choose a different command name",
	);
}

#[test]
fn upsert_cli_command_document_rejects_empty_replacement_steps() {
	let update = CommandUpdate {
		original_name: None,
		name: "release-pr".to_string(),
		help_text: None,
		dry_run: false,
		steps: CommandStepUpdate::Replace(Vec::new()),
	};

	assert_config_error(
		upsert_cli_command_document("", &update).map(|_| ()),
		"a CLI command needs at least one step",
	);
}

#[test]
fn render_document_and_optional_text_normalization_are_stable() {
	let document = ""
		.parse::<DocumentMut>()
		.unwrap_or_else(|error| panic!("empty document should parse: {error}"));

	assert_eq!(render_document(&document), "\n");
	assert_eq!(normalize_optional_text(" \n\t "), None);
	assert_eq!(
		normalize_optional_text(" release "),
		Some("release".to_string())
	);
}

#[test]
fn config_text_helpers_handle_missing_files_and_io_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let config_path = tempdir.path().join("monochange.toml");

	assert_eq!(
		read_config_text(&config_path).unwrap_or_else(|error| panic!("missing config: {error}")),
		""
	);
	write_config_text(&config_path, "name = \"demo\"\n")
		.unwrap_or_else(|error| panic!("write config: {error}"));
	assert_eq!(
		read_config_text(&config_path).unwrap_or_else(|error| panic!("read config: {error}")),
		"name = \"demo\"\n"
	);

	let read_error = read_config_text(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("reading a directory should fail"));
	assert!(read_error.to_string().contains("failed to read"));

	let missing_parent_path = tempdir.path().join("missing").join("monochange.toml");
	let write_error = write_config_text(&missing_parent_path, "")
		.err()
		.unwrap_or_else(|| panic!("writing through a missing parent should fail"));
	assert!(write_error.to_string().contains("failed to write"));
	assert!(!missing_parent_path.exists());
}

#[test]
fn validate_command_name_rejects_reserved_command_name() {
	let error = validate_command_name("command")
		.err()
		.unwrap_or_else(|| panic!("command should be reserved"));

	assert!(
		error
			.to_string()
			.contains("collides with a reserved built-in command")
	);
}

#[test]
fn validate_command_name_rejects_non_kebab_case_names() {
	let error = validate_command_name("Release_PR")
		.err()
		.unwrap_or_else(|| panic!("uppercase names should be rejected"));

	assert!(
		error
			.to_string()
			.contains("use lowercase letters, digits, and hyphens only")
	);
}

fn assert_config_error(result: monochange_core::MonochangeResult<()>, expected: &str) {
	let error = result
		.err()
		.unwrap_or_else(|| panic!("expected config error containing {expected}"));
	assert!(
		error.to_string().contains(expected),
		"expected {error} to contain {expected}"
	);
}

fn render_update(config: &str, update: &CommandUpdate) -> String {
	upsert_cli_command_document(config, update)
		.unwrap_or_else(|error| panic!("command update should render: {error}"))
}

fn parse_rendered_toml(rendered: &str) -> Value {
	toml::from_str(rendered).unwrap_or_else(|error| panic!("rendered TOML should parse: {error}"))
}
