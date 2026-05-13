use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::BumpSeverity;

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
pub fn post_process_release(
	schema: &mut serde_json::Value,
	id: &str,
	title: &str,
	schema_version: &str,
) {
	post_process(
		schema,
		id,
		title,
		"Durable commit-embedded release record schema for monochange release records.",
	);
	let Some(obj) = schema.as_object_mut() else {
		return;
	};
	obj.insert(
		"additionalProperties".to_string(),
		serde_json::Value::Bool(false),
	);
	let Some(props) = schema
		.pointer_mut("/properties")
		.and_then(|value| value.as_object_mut())
	else {
		return;
	};
	if let Some(schema_version_obj) = props
		.get_mut("schemaVersion")
		.and_then(|schema_version| schema_version.as_object_mut())
	{
		schema_version_obj.insert(
			"default".to_string(),
			serde_json::Value::String(schema_version.to_string()),
		);
	}
	if let Some(kind_obj) = props.get_mut("kind").and_then(|kind| kind.as_object_mut()) {
		kind_obj.remove("default");
		kind_obj.insert(
			"const".to_string(),
			serde_json::Value::String(monochange_schema::release_record::KIND.to_string()),
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
	let schema_version_path = workspace_dir.join("crates/monochange_schema/SCHEMA_VERSION");
	let version = expected_schema_version(workspace_dir)?;
	run_with_paths(
		update_mode,
		&schemas_dir,
		&docs_schemas_dir,
		&schema_version_path,
		&version,
	)
}

/// Core schema generation logic with configurable output directories.
pub fn run_with_paths(
	update_mode: bool,
	schemas_dir: &PathBuf,
	docs_schemas_dir: &PathBuf,
	schema_version_path: &PathBuf,
	version: &str,
) -> Result<(), String> {
	// Release record schema
	let release_schema = monochange_core::schema::release_record();
	let mut release_value = release_schema.to_value();
	post_process_release(
		&mut release_value,
		"https://monochange.github.io/monochange/schemas/release-record.schema.json",
		"monochange release record",
		version,
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
	let release_record_artifact_json =
		monochange_schema::release_record::populated_artifact_json(version);
	let config_artifact_json = monochange_schema::config::populated_artifact_json();
	let migration_changelog_json = format!(
		"{}\n",
		monochange_schema::migration_changelog::to_json_pretty().unwrap()
	);

	// Versioned schemas (identical content, different $id)
	let mut release_versioned_value = release_value.clone();
	let mut config_versioned_value = config_value.clone();
	post_process_release(
		&mut release_versioned_value,
		&format!(
			"https://monochange.github.io/monochange/schemas/release-record.v{version}.schema.json"
		),
		"monochange release record",
		version,
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
	let migration_changelog_path = schemas_dir.join("migration-changelog.json");
	let artifacts_dir = schemas_dir.join("artifacts");
	let release_record_artifact_current_path = artifacts_dir.join("release-record.current.json");
	let release_record_artifact_versioned_path =
		artifacts_dir.join(format!("release-record.v{version}.json"));
	let config_artifact_current_path = artifacts_dir.join("monochange.current.json");
	let config_artifact_versioned_path = artifacts_dir.join(format!("monochange.v{version}.json"));
	let docs_release_path = docs_schemas_dir.join("release-record.schema.json");
	let docs_config_path = docs_schemas_dir.join("monochange.schema.json");
	let docs_release_versioned_path =
		docs_schemas_dir.join(format!("release-record.v{version}.schema.json"));
	let docs_config_versioned_path =
		docs_schemas_dir.join(format!("monochange.v{version}.schema.json"));

	if update_mode {
		fs::create_dir_all(schemas_dir).unwrap();
		fs::create_dir_all(docs_schemas_dir).unwrap();
		fs::create_dir_all(&artifacts_dir).unwrap();
		if let Some(parent) = schema_version_path.parent() {
			fs::create_dir_all(parent).unwrap();
		}

		fs::write(schema_version_path, schema_version_file_contents(version)).unwrap();
		fs::write(&release_path, &release_json).unwrap();
		fs::write(&config_path, &config_json).unwrap();
		fs::write(&migration_changelog_path, &migration_changelog_json).unwrap();
		fs::write(
			&release_record_artifact_current_path,
			&release_record_artifact_json,
		)
		.unwrap();
		fs::write(
			&release_record_artifact_versioned_path,
			&release_record_artifact_json,
		)
		.unwrap();
		fs::write(&config_artifact_current_path, &config_artifact_json).unwrap();
		fs::write(&config_artifact_versioned_path, &config_artifact_json).unwrap();

		// Also write to docs directory (unversioned aliases)
		fs::write(&docs_release_path, &release_json).unwrap();
		fs::write(&docs_config_path, &config_json).unwrap();

		// Write versioned schemas
		fs::write(&docs_release_versioned_path, &release_versioned_json).unwrap();
		fs::write(&docs_config_versioned_path, &config_versioned_json).unwrap();

		println!("Schemas updated successfully.");
		return Ok(());
	}

	// Check mode: compare existing files
	let mut errors = Vec::new();
	if let Err(error) = check_text_files(&[(
		schema_version_path,
		schema_version_file_contents(version).as_str(),
	)]) {
		errors.push(error);
	}
	if let Err(error) = check_schemas(&[
		(&release_path, release_json.as_str()),
		(&config_path, config_json.as_str()),
		(&migration_changelog_path, migration_changelog_json.as_str()),
		(
			&release_record_artifact_current_path,
			release_record_artifact_json.as_str(),
		),
		(
			&release_record_artifact_versioned_path,
			release_record_artifact_json.as_str(),
		),
		(&config_artifact_current_path, config_artifact_json.as_str()),
		(
			&config_artifact_versioned_path,
			config_artifact_json.as_str(),
		),
		(&docs_release_path, release_json.as_str()),
		(&docs_config_path, config_json.as_str()),
		(
			&docs_release_versioned_path,
			release_versioned_json.as_str(),
		),
		(&docs_config_versioned_path, config_versioned_json.as_str()),
	]) {
		errors.push(error);
	}
	if errors.is_empty() {
		println!("Schemas are up to date.");
		Ok(())
	} else {
		Err(errors.join("\n"))
	}
}

fn schema_version_file_contents(version: &str) -> String {
	format!("{version}\n")
}

/// Derive the expected public schema version from the next `monochange_schema` release.
pub fn expected_schema_version(workspace_dir: &Path) -> Result<String, String> {
	let package_version = schema_package_manifest_version(workspace_dir)?;
	let current_version = semver::Version::parse(&package_version).map_err(|error| {
		format!("Could not parse monochange_schema package version `{package_version}`: {error}")
	})?;
	let next_version = planned_schema_bump(workspace_dir)?.apply_to_version(&current_version);
	monochange_schema::SchemaVersion::from_package_version(&next_version.to_string())
		.map(|schema_version| schema_version.to_string())
		.map_err(|error| format!("Could not derive schema version from `{next_version}`: {error}"))
}

fn schema_package_manifest_version(workspace_dir: &Path) -> Result<String, String> {
	let manifest_path = workspace_dir.join("crates/monochange_schema/Cargo.toml");
	let manifest = fs::read_to_string(&manifest_path)
		.map_err(|error| format!("Could not read {}: {error}", manifest_path.display()))?;
	package_version_from_manifest(&manifest).ok_or_else(|| {
		format!(
			"Could not find [package] version in {}",
			manifest_path.display()
		)
	})
}

fn package_version_from_manifest(manifest: &str) -> Option<String> {
	let mut in_package = false;
	for line in manifest.lines() {
		let trimmed = line.trim();
		if trimmed == "[package]" {
			in_package = true;
			continue;
		}
		if trimmed.starts_with('[') {
			in_package = false;
			continue;
		}
		if !in_package {
			continue;
		}
		let Some((key, value)) = trimmed.split_once('=') else {
			continue;
		};
		if key.trim() == "version" {
			return Some(clean_changeset_scalar(value).to_string());
		}
	}
	None
}

fn planned_schema_bump(workspace_dir: &Path) -> Result<BumpSeverity, String> {
	let changeset_dir = workspace_dir.join(".changeset");
	let Ok(entries) = fs::read_dir(&changeset_dir) else {
		return Ok(BumpSeverity::None);
	};
	let mut bump = BumpSeverity::None;
	for entry in entries {
		let entry = entry.map_err(|error| {
			format!(
				"Could not read entry in {}: {error}",
				changeset_dir.display()
			)
		})?;
		let path = entry.path();
		if path.extension().and_then(|value| value.to_str()) != Some("md") {
			continue;
		}
		let contents = fs::read_to_string(&path)
			.map_err(|error| format!("Could not read {}: {error}", path.display()))?;
		bump = bump.max(changeset_bump_for_package(&contents, "monochange_schema"));
	}
	Ok(bump)
}

fn changeset_bump_for_package(contents: &str, package: &str) -> BumpSeverity {
	let normalized = contents.replace("\r\n", "\n").replace('\r', "\n");
	let Some(without_opening) = normalized.strip_prefix("---") else {
		return BumpSeverity::None;
	};
	let Some((frontmatter, _body)) = without_opening.split_once("\n---") else {
		return BumpSeverity::None;
	};

	let mut bump = BumpSeverity::None;
	let mut active_package = false;
	for line in frontmatter.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}

		if line.starts_with(' ') || line.starts_with('\t') {
			if active_package {
				bump = bump.max(nested_bump(trimmed));
			}
			continue;
		}

		active_package = false;
		let Some((raw_key, raw_value)) = trimmed.split_once(':') else {
			continue;
		};
		let key = clean_changeset_scalar(raw_key);
		if key != package {
			continue;
		}

		active_package = true;
		bump = bump.max(inline_bump(raw_value));
	}
	bump
}

fn nested_bump(line: &str) -> BumpSeverity {
	let Some((key, value)) = line.split_once(':') else {
		return BumpSeverity::None;
	};
	if clean_changeset_scalar(key) != "bump" {
		return BumpSeverity::None;
	}
	bump_from_text(clean_changeset_scalar(value))
}

fn inline_bump(value: &str) -> BumpSeverity {
	let value = value.trim();
	if value.is_empty() {
		return BumpSeverity::None;
	}
	let direct = bump_from_text(clean_changeset_scalar(value));
	if direct != BumpSeverity::None {
		return direct;
	}

	let inline_table = value.trim_start_matches('{').trim_end_matches('}');
	inline_table
		.split(',')
		.map(nested_bump)
		.max()
		.unwrap_or(BumpSeverity::None)
}

fn bump_from_text(value: &str) -> BumpSeverity {
	match clean_changeset_scalar(value) {
		"major" => BumpSeverity::Major,
		"minor" => BumpSeverity::Minor,
		"patch" => BumpSeverity::Patch,
		_ => BumpSeverity::None,
	}
}

fn clean_changeset_scalar(value: &str) -> &str {
	value
		.trim()
		.trim_matches(',')
		.trim_matches('"')
		.trim_matches('\'')
		.trim()
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

/// Compare expected text strings against files on disk.
fn check_text_files(paths: &[(&PathBuf, &str)]) -> Result<(), String> {
	let mut errors = Vec::new();
	for (path, expected) in paths {
		if path.exists() {
			let existing = fs::read_to_string(path).unwrap();
			if existing != *expected {
				errors.push(format!("Generated file mismatch: {}", path.display()));
			}
		} else {
			errors.push(format!("Generated file missing: {}", path.display()));
		}
	}
	if errors.is_empty() {
		Ok(())
	} else {
		Err(errors.join("\n"))
	}
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
		Ok(())
	} else {
		Err(errors.join("\n"))
	}
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
