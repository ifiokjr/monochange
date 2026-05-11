use toml::Value;

use super::CommandStepDraft;
use super::CommandStepUpdate;
use super::CommandUpdate;
use super::cli_command_summaries;
use super::upsert_cli_command_document;
use super::validate_command_name;

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
		steps: CommandStepUpdate::Replace(vec![CommandStepDraft {
			kind: "Command".to_string(),
			name: Some("generate lockfiles".to_string()),
			command: Some("pnpm install --lockfile-only".to_string()),
		}]),
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

fn render_update(config: &str, update: &CommandUpdate) -> String {
	upsert_cli_command_document(config, update)
		.unwrap_or_else(|error| panic!("command update should render: {error}"))
}

fn parse_rendered_toml(rendered: &str) -> Value {
	toml::from_str(rendered).unwrap_or_else(|error| panic!("rendered TOML should parse: {error}"))
}
