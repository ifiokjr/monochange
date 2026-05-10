use std::fs;
use std::path::PathBuf;

/// Post-process a generated JSON schema by adding `$id`, `title`, and `description`.
pub fn post_process(schema: &mut serde_json::Value, id: &str, title: &str, description: &str) {
	if let Some(obj) = schema.as_object_mut() {
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
	// Override schemaVersion and kind to use const (single allowed value)
	let Some(props) = schema
		.pointer_mut("/properties")
		.and_then(|v| v.as_object_mut())
	else {
		return;
	};
	if let Some(sv_obj) = props
		.get_mut("schemaVersion")
		.and_then(|sv| sv.as_object_mut())
	{
		sv_obj.remove("default");
		sv_obj.insert(
			"const".to_string(),
			serde_json::Value::String("0.1".to_string()),
		);
	}
	if let Some(kind_obj) = props.get_mut("kind").and_then(|kind| kind.as_object_mut()) {
		kind_obj.remove("default");
		kind_obj.insert(
			"const".to_string(),
			serde_json::Value::String("monochange.releaseRecord".to_string()),
		);
	}
}

/// Post-process a config schema by renaming $defs and adding additionalProperties: false.
pub fn post_process_config(schema: &mut serde_json::Value, id: &str, title: &str) {
	post_process(
		schema,
		id,
		title,
		"JSON Schema for monochange.toml workspace configuration files.",
	);
	// Rename $defs from Raw* to camelCase equivalents, update $refs, add additionalProperties: false
	let replacements: std::collections::HashMap<String, String> = [
		(
			"RawPackageDefinition".to_string(),
			"packageDefinition".to_string(),
		),
		(
			"RawGroupDefinition".to_string(),
			"groupDefinition".to_string(),
		),
		(
			"RawCliCommandDefinition".to_string(),
			"cliCommand".to_string(),
		),
		(
			"RawEcosystemSettings".to_string(),
			"ecosystemSettings".to_string(),
		),
		("RawSourceConfiguration".to_string(), "source".to_string()),
		("RawWorkspaceDefaults".to_string(), "defaults".to_string()),
	]
	.into_iter()
	.collect();

	// Walk the tree and rename $refs
	fn rename_refs(value: &mut serde_json::Value, map: &std::collections::HashMap<String, String>) {
		match value {
			serde_json::Value::Object(obj) => {
				for (k, v) in obj.iter_mut() {
					if k == "$ref"
						&& let serde_json::Value::String(s) = v
					{
						for (old, new) in map {
							*s = s.replace(&format!("#/$defs/{old}"), &format!("#/$defs/{new}"));
						}
					} else {
						rename_refs(v, map);
					}
				}
			}
			serde_json::Value::Array(arr) => {
				for v in arr.iter_mut() {
					rename_refs(v, map);
				}
			}
			_ => {}
		}
	}
	rename_refs(schema, &replacements);

	#[allow(clippy::option_map_unit_fn)]
	// Rename $defs keys and add additionalProperties: false
	schema
		.pointer_mut("/$defs")
		.and_then(|v| v.as_object_mut())
		.map(|defs| {
			let keys_to_rename: Vec<(String, String, serde_json::Value)> = defs
				.iter_mut()
				.filter_map(|(k, v)| {
					replacements.get(k).map(|new| {
						if let Some(obj) = v.as_object_mut()
							&& obj.contains_key("properties")
						{
							obj.insert(
								"additionalProperties".to_string(),
								serde_json::Value::Bool(false),
							);
						}
						(k.clone(), new.clone(), v.clone())
					})
				})
				.collect();
			for (old, new, value) in keys_to_rename {
				defs.remove(&old);
				defs.insert(new, value);
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
	let version = monochange_schema::CURRENT_SCHEMA_VERSION_TEXT;

	// Release record schema
	let release_schema = schemars::schema_for!(monochange_core::ReleaseRecord);
	let mut release_value = release_schema.to_value();
	post_process_release(
		&mut release_value,
		"https://monochange.github.io/monochange/schemas/release-record.schema.json",
		"monochange release record",
	);

	// Config schema from raw TOML types
	let config_schema = schemars::schema_for!(monochange_config::RawWorkspaceConfiguration);
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
		fs::create_dir_all(&schemas_dir).unwrap();
		fs::create_dir_all(&docs_schemas_dir).unwrap();

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

/// Compare expected schema JSON strings against files on disk.
fn check_schemas(paths: &[(&PathBuf, &str)]) -> Result<(), String> {
	let mut errors = Vec::new();
	for (path, expected) in paths {
		if path.exists() {
			let existing = fs::read_to_string(path).unwrap();
			if existing != *expected {
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

/// CLI entry point that parses the subcommand and delegates to [`run`].
///
/// Returns an error message if schema generation or validation fails.
pub fn run_cli(subcommand: Option<&str>) -> Result<(), String> {
	let update_mode = subcommand == Some("update");
	run(update_mode)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn post_process_sets_id_title_description() {
		let mut schema = serde_json::json!({"type": "object"});
		post_process(
			&mut schema,
			"https://example.com/schema.json",
			"test schema",
			"test description",
		);
		let obj = schema.as_object().unwrap();
		assert_eq!(
			obj.get("$id"),
			Some(&serde_json::Value::String(
				"https://example.com/schema.json".to_string()
			))
		);
		assert_eq!(
			obj.get("title"),
			Some(&serde_json::Value::String("test schema".to_string()))
		);
		assert_eq!(
			obj.get("description"),
			Some(&serde_json::Value::String("test description".to_string()))
		);
	}

	#[test]
	fn post_process_release_adds_additional_properties_false() {
		let mut schema = serde_json::json!({
			"type": "object",
			"properties": {
				"schemaVersion": {"type": "string", "default": "0.1"},
				"kind": {"type": "string"}
			}
		});
		post_process_release(
			&mut schema,
			"https://example.com/release.json",
			"release record",
		);
		let obj = schema.as_object().unwrap();
		assert_eq!(
			obj.get("additionalProperties"),
			Some(&serde_json::Value::Bool(false))
		);
		assert_eq!(
			obj.get("$id"),
			Some(&serde_json::Value::String(
				"https://example.com/release.json".to_string()
			))
		);
		assert_eq!(
			obj.get("title"),
			Some(&serde_json::Value::String("release record".to_string()))
		);

		let props = obj.get("properties").unwrap().as_object().unwrap();
		let sv = props.get("schemaVersion").unwrap().as_object().unwrap();
		assert!(!sv.contains_key("default"));
		assert_eq!(
			sv.get("const"),
			Some(&serde_json::Value::String("0.1".to_string()))
		);
	}

	#[test]
	fn post_process_release_non_object() {
		let mut schema = serde_json::Value::Null;
		post_process_release(
			&mut schema,
			"https://example.com/release.json",
			"release record",
		);
		// Should not panic and not modify
		assert!(schema.is_null());
	}

	#[test]
	fn post_process_release_no_properties() {
		let mut schema = serde_json::json!({
			"type": "object",
			"title": "test"
		});
		post_process_release(
			&mut schema,
			"https://example.com/release.json",
			"release record",
		);
		let obj = schema.as_object().unwrap();
		assert_eq!(
			obj.get("additionalProperties"),
			Some(&serde_json::Value::Bool(false))
		);
	}

	#[test]
	fn post_process_release_converts_kind_to_const() {
		let mut schema = serde_json::json!({
			"type": "object",
			"properties": {
				"kind": {"type": "string", "default": "default.kind"}
			}
		});
		post_process_release(
			&mut schema,
			"https://example.com/release.json",
			"release record",
		);
		let props = schema.pointer("/properties").unwrap().as_object().unwrap();
		let kind = props.get("kind").unwrap().as_object().unwrap();
		assert!(!kind.contains_key("default"));
		assert_eq!(
			kind.get("const"),
			Some(&serde_json::Value::String(
				"monochange.releaseRecord".to_string()
			))
		);
	}

	#[test]
	fn post_process_config_renames_defs() {
		let mut schema = serde_json::json!({
			"type": "object",
			"$defs": {
				"RawPackageDefinition": {
					"type": "object",
					"properties": {
						"path": {"type": "string"}
					}
				},
				"BumpSeverity": {
					"type": "string"
				}
			},
			"properties": {
				"package": {"$ref": "#/$defs/RawPackageDefinition"}
			}
		});
		post_process_config(
			&mut schema,
			"https://example.com/config.json",
			"config schema",
		);
		let obj = schema.as_object().unwrap();
		assert_eq!(
			obj.get("$id"),
			Some(&serde_json::Value::String(
				"https://example.com/config.json".to_string()
			))
		);

		// RawPackageDefinition should be renamed to packageDefinition
		let defs = obj.get("$defs").unwrap().as_object().unwrap();
		assert!(!defs.contains_key("RawPackageDefinition"));
		assert!(defs.contains_key("packageDefinition"));

		// packageDefinition should have additionalProperties: false
		let pd = defs.get("packageDefinition").unwrap().as_object().unwrap();
		assert_eq!(
			pd.get("additionalProperties"),
			Some(&serde_json::Value::Bool(false))
		);

		// $ref should be updated
		let props = obj.get("properties").unwrap().as_object().unwrap();
		let package = props.get("package").unwrap().as_object().unwrap();
		assert_eq!(
			package.get("$ref"),
			Some(&serde_json::Value::String(
				"#/$defs/packageDefinition".to_string()
			))
		);
	}

	#[test]
	fn run_cli_round_trip() {
		// Update writes schemas, check validates them
		assert!(run_cli(Some("update")).is_ok());
		assert!(run_cli(None).is_ok());
	}

	#[test]
	fn check_schemas_mismatch() {
		let temp_path = PathBuf::from("/tmp/test-schema-mismatch.json");
		fs::write(&temp_path, "wrong content").unwrap();
		let paths = [(&temp_path, "expected content")];
		let result = check_schemas(&paths);
		fs::remove_file(&temp_path).unwrap();
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("mismatch"));
	}

	#[test]
	fn check_schemas_missing() {
		let temp_path = PathBuf::from("/tmp/test-schema-missing.json");
		let _ = fs::remove_file(&temp_path);
		let paths = [(&temp_path, "expected content")];
		let result = check_schemas(&paths);
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("missing"));
	}
}
