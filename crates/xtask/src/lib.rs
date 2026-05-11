use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// Post-process a generated JSON schema by adding `$id`, `title`, and `description`.
pub fn post_process(schema: &mut serde_json::Value, id: &str, title: &str, description: &str) {
	let Some(obj) = schema.as_object_mut() else {
		return;
	};
	obj.insert("$id".to_string(), serde_json::Value::String(id.to_string()));
	obj.insert(
		"title".to_string(),
		serde_json::Value::String(title.to_string()),
	);
	obj.insert(
		"description".to_string(),
		serde_json::Value::String(description.to_string()),
	);
}

/// Post-process a release-record schema.
pub fn post_process_release(schema: &mut serde_json::Value, id: &str, title: &str) {
	post_process(
		schema,
		id,
		title,
		"Durable commit-embedded release record schema for monochange artifact version 0.1.",
	);
	let Some(obj) = schema.as_object_mut() else {
		return;
	};
	obj.insert(
		"additionalProperties".to_string(),
		serde_json::Value::Bool(false),
	);
	// Override kind to use const (single allowed value) — schemaVersion keeps default since it evolves
	let Some(props) = schema
		.pointer_mut("/properties")
		.and_then(|v| v.as_object_mut())
	else {
		return;
	};
	// Override kind to use const — schemaVersion keeps default since it evolves
	if let Some(kind_obj) = props.get_mut("kind").and_then(|kind| kind.as_object_mut()) {
		kind_obj.remove("default");
		kind_obj.insert(
			"const".to_string(),
			serde_json::Value::String("monochange.releaseRecord".to_string()),
		);
	}
}

/// Post-process a config schema by adding additionalProperties: false to all $defs objects.
pub fn post_process_config(schema: &mut serde_json::Value, id: &str, title: &str) {
	post_process(
		schema,
		id,
		title,
		"JSON Schema for monochange.toml workspace configuration files.",
	);

	// Walk $defs and add additionalProperties: false to all object definitions with properties
	#[allow(clippy::option_map_unit_fn)]
	schema
		.pointer_mut("/$defs")
		.and_then(|v| v.as_object_mut())
		.map(|defs| {
			for (_key, def) in defs.iter_mut() {
				if let Some(obj) = def.as_object_mut()
					&& obj.contains_key("properties")
				{
					obj.insert(
						"additionalProperties".to_string(),
						serde_json::Value::Bool(false),
					);
				}
			}
		});
}

/// Generate schema JSON strings and write them to disk (update_mode) or compare to disk (check mode).
///
/// Returns `Ok(())` on success, or an error message describing the mismatch.
pub fn run(update_mode: bool) -> Result<(), String> {
	let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	let workspace_dir = crate_dir.parent().unwrap().parent().unwrap();
	let schemas_dir = workspace_dir.join("crates/monochange_schema/schemas");
	let docs_schemas_dir = workspace_dir.join("docs/src/schemas");
	run_with_paths(update_mode, &schemas_dir, &docs_schemas_dir)
}

/// Core schema generation logic with configurable output directories.
pub fn run_with_paths(
	update_mode: bool,
	schemas_dir: &PathBuf,
	docs_schemas_dir: &PathBuf,
) -> Result<(), String> {
	let version = monochange_schema::CURRENT_SCHEMA_VERSION_TEXT;

	// Release record schema
	let release_schema = monochange_core::schema::release_record();
	let mut release_value = release_schema.to_value();
	post_process_release(
		&mut release_value,
		"https://monochange.github.io/monochange/schemas/release-record.schema.json",
		"monochange release record",
	);

	// Config schema from raw TOML types
	let config_schema = monochange_config::schema::workspace_configuration();
	let mut config_value = config_schema.to_value();
	post_process_config(
		&mut config_value,
		"https://monochange.github.io/monochange/schemas/monochange.schema.json",
		"monochange configuration",
	);

	let release_json = serde_json::to_string_pretty(&release_value).unwrap();
	let config_json = serde_json::to_string_pretty(&config_value).unwrap();

	// Versioned schemas (identical content, different $id)
	let mut release_versioned_value = release_value.clone();
	let mut config_versioned_value = config_value.clone();
	post_process_release(
		&mut release_versioned_value,
		&format!(
			"https://monochange.github.io/monochange/schemas/release-record.v{version}.schema.json"
		),
		"monochange release record",
	);
	post_process_config(
		&mut config_versioned_value,
		&format!(
			"https://monochange.github.io/monochange/schemas/monochange.v{version}.schema.json"
		),
		"monochange configuration",
	);
	let release_versioned_json = serde_json::to_string_pretty(&release_versioned_value).unwrap();
	let config_versioned_json = serde_json::to_string_pretty(&config_versioned_value).unwrap();

	let release_path = schemas_dir.join("release-record.schema.json");
	let config_path = schemas_dir.join("monochange.schema.json");

	if update_mode {
		fs::create_dir_all(schemas_dir).unwrap();
		fs::create_dir_all(docs_schemas_dir).unwrap();

		fs::write(&release_path, &release_json).unwrap();
		fs::write(&config_path, &config_json).unwrap();

		// Also write to docs directory (unversioned aliases)
		fs::write(
			docs_schemas_dir.join("release-record.schema.json"),
			&release_json,
		)
		.unwrap();
		fs::write(
			docs_schemas_dir.join("monochange.schema.json"),
			&config_json,
		)
		.unwrap();

		// Write versioned schemas
		fs::write(
			docs_schemas_dir.join(format!("release-record.v{version}.schema.json")),
			&release_versioned_json,
		)
		.unwrap();
		fs::write(
			docs_schemas_dir.join(format!("monochange.v{version}.schema.json")),
			&config_versioned_json,
		)
		.unwrap();

		println!("Schemas updated successfully.");
		return Ok(());
	}

	// Check mode: compare existing files
	check_schemas(&[
		(&release_path, release_json.as_str()),
		(&config_path, config_json.as_str()),
	])
}

const COMMANDS_INVENTORY_START: &str = "<!-- xtask:commands:start -->";
const COMMANDS_INVENTORY_END: &str = "<!-- xtask:commands:end -->";

/// Check or update the generated command inventory in the monochange skill package.
pub fn run_skill_commands(update_mode: bool) -> Result<(), String> {
	let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	let workspace_dir = crate_dir.parent().unwrap().parent().unwrap();
	let commands_path = workspace_dir.join("packages/monochange__skill/skills/commands.md");
	run_skill_commands_with_paths(update_mode, workspace_dir, &commands_path)
}

/// Check or update a commands skill file against the CLI source and workspace config.
pub fn run_skill_commands_with_paths(
	update_mode: bool,
	workspace_dir: &Path,
	commands_path: &Path,
) -> Result<(), String> {
	let expected = render_current_commands_inventory(workspace_dir)?;
	let current = fs::read_to_string(commands_path)
		.map_err(|error| format!("Could not read {}: {error}", commands_path.display()))?;
	let updated = replace_commands_inventory(&current, &expected)?;

	if update_mode {
		fs::write(commands_path, updated)
			.map_err(|error| format!("Could not write {}: {error}", commands_path.display()))?;
		println!("Skill command inventory updated successfully.");
		return Ok(());
	}

	if current == updated {
		println!("Skill command inventory is up to date.");
		Ok(())
	} else {
		Err(format!(
			"Skill command inventory is out of date: {}\nRun `cargo xtask skill commands update`.",
			commands_path.display()
		))
	}
}

fn render_current_commands_inventory(workspace_dir: &Path) -> Result<String, String> {
	let cli_source_path = workspace_dir.join("crates/monochange/src/cli.rs");
	let cli_source = fs::read_to_string(&cli_source_path)
		.map_err(|error| format!("Could not read {}: {error}", cli_source_path.display()))?;
	let built_in = command_literals_from_cli_source(&cli_source);
	let configured = configured_command_names(workspace_dir)?;
	let step_commands = monochange_core::all_step_variants()
		.into_iter()
		.map(|step| format!("step:{}", step.step_kebab_name()))
		.collect::<BTreeSet<_>>();

	Ok(render_commands_inventory(
		&built_in,
		&configured,
		&step_commands,
	))
}

fn configured_command_names(workspace_dir: &Path) -> Result<BTreeSet<String>, String> {
	let configuration = monochange_config::load_workspace_configuration(workspace_dir)
		.map_err(|error| format!("Could not load monochange.toml: {error}"))?;
	Ok(configuration
		.cli
		.into_iter()
		.map(|command| command.name)
		.collect())
}

fn command_literals_from_cli_source(source: &str) -> BTreeSet<String> {
	let mut commands = BTreeSet::new();
	let mut remaining = source;
	let needle = "Command::new(\"";

	while let Some((_, after_needle)) = remaining.split_once(needle) {
		let Some((command, after_command)) = after_needle.split_once('\"') else {
			break;
		};
		commands.insert(command.to_string());
		remaining = after_command;
	}

	commands
}

fn render_commands_inventory(
	built_in: &BTreeSet<String>,
	configured: &BTreeSet<String>,
	step_commands: &BTreeSet<String>,
) -> String {
	let mut inventory = String::new();
	inventory.push_str(COMMANDS_INVENTORY_START);
	inventory.push_str("\n\n");
	inventory.push_str(
		"This inventory is generated by `cargo xtask skill commands update` and checked by `cargo xtask skill commands check`.\n\n",
	);
	push_inventory_group(
		&mut inventory,
		"Command literals in `crates/monochange/src/cli.rs`",
		built_in,
	);
	push_inventory_group(
		&mut inventory,
		"Configured workflow commands in this repository's `monochange.toml`",
		configured,
	);
	push_inventory_group(
		&mut inventory,
		"Built-in `mc step:*` commands from `CliStepDefinition`",
		step_commands,
	);
	inventory.push_str(COMMANDS_INVENTORY_END);
	inventory
}

fn push_inventory_group(inventory: &mut String, title: &str, commands: &BTreeSet<String>) {
	inventory.push_str("### ");
	inventory.push_str(title);
	inventory.push_str("\n\n");

	for command in commands {
		inventory.push_str("- `");
		inventory.push_str(command);
		inventory.push_str("`\n");
	}
	inventory.push('\n');
}

fn replace_commands_inventory(current: &str, expected: &str) -> Result<String, String> {
	let Some((before, rest)) = current.split_once(COMMANDS_INVENTORY_START) else {
		return Err(format!(
			"Missing command inventory start marker `{COMMANDS_INVENTORY_START}`"
		));
	};
	let Some((_, after)) = rest.split_once(COMMANDS_INVENTORY_END) else {
		return Err(format!(
			"Missing command inventory end marker `{COMMANDS_INVENTORY_END}`"
		));
	};

	let mut updated = String::new();
	updated.push_str(before);
	updated.push_str(expected);
	updated.push_str(after);
	Ok(updated)
}

/// Compare expected schema JSON strings against files on disk.
fn check_schemas(paths: &[(&PathBuf, &str)]) -> Result<(), String> {
	let mut errors = Vec::new();
	for (path, expected) in paths {
		if path.exists() {
			let existing = fs::read_to_string(path).unwrap();
			let existing_value: serde_json::Value = match serde_json::from_str(&existing) {
				Ok(v) => v,
				Err(_) => {
					errors.push(format!(
						"Schema mismatch (invalid JSON): {}",
						path.display()
					));
					continue;
				}
			};
			let expected_value: serde_json::Value = match serde_json::from_str(expected) {
				Ok(v) => v,
				Err(_) => {
					errors.push(format!(
						"Generated schema contains invalid JSON for {}",
						path.display()
					));
					continue;
				}
			};
			if existing_value != expected_value {
				errors.push(format!("Schema mismatch: {}", path.display()));
			}
		} else {
			errors.push(format!("Schema file missing: {}", path.display()));
		}
	}
	if errors.is_empty() {
		println!("Schemas are up to date.");
		Ok(())
	} else {
		Err(errors.join("\n"))
	}
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
