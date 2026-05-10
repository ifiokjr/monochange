use std::fs;
use std::path::PathBuf;

fn post_process(schema: &mut serde_json::Value, id: &str, title: &str, description: &str) {
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

fn post_process_release(schema: &mut serde_json::Value, id: &str, title: &str) {
	post_process(
		schema,
		id,
		title,
		"Durable commit-embedded release record schema for monochange artifact version 0.1.",
	);
	if let Some(obj) = schema.as_object_mut() {
		obj.insert(
			"additionalProperties".to_string(),
			serde_json::Value::Bool(false),
		);
		// Override schemaVersion and kind to use const (single allowed value)
		if let Some(props) = schema
			.pointer_mut("/properties")
			.and_then(|v| v.as_object_mut())
		{
			if let Some(sv) = props.get_mut("schemaVersion") {
				if let Some(sv_obj) = sv.as_object_mut() {
					sv_obj.remove("default");
					sv_obj.insert(
						"const".to_string(),
						serde_json::Value::String("0.1".to_string()),
					);
				}
			}
			if let Some(kind) = props.get_mut("kind") {
				if let Some(kind_obj) = kind.as_object_mut() {
					kind_obj.remove("default");
					kind_obj.insert(
						"const".to_string(),
						serde_json::Value::String("monochange.releaseRecord".to_string()),
					);
				}
			}
		}
	}
}

fn post_process_config(schema: &mut serde_json::Value, id: &str, title: &str) {
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
					if k == "$ref" {
						if let serde_json::Value::String(s) = v {
							for (old, new) in map {
								*s =
									s.replace(&format!("#/$defs/{old}"), &format!("#/$defs/{new}"));
							}
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

	// Rename $defs keys and add additionalProperties: false
	if let Some(defs) = schema.pointer_mut("/$defs").and_then(|v| v.as_object_mut()) {
		let keys_to_rename: Vec<(String, String, serde_json::Value)> = defs
			.iter_mut()
			.filter_map(|(k, v)| {
				replacements.get(k).map(|new| {
					if let Some(obj) = v.as_object_mut() {
						if obj.contains_key("properties") {
							obj.insert(
								"additionalProperties".to_string(),
								serde_json::Value::Bool(false),
							);
						}
					}
					(k.clone(), new.clone(), v.clone())
				})
			})
			.collect();
		for (old, new, value) in keys_to_rename {
			defs.remove(&old);
			defs.insert(new, value);
		}
	}
}

fn main() {
	let args: Vec<String> = std::env::args().collect();
	let update_mode = args.get(1).map(|s| s.as_str()) == Some("update");

	let schemas_dir = PathBuf::from("crates/monochange_schema/schemas");
	let docs_schemas_dir = PathBuf::from("docs/src/schemas");
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
	} else {
		// Check mode: compare existing files
		let check_file = |path: &PathBuf, content: &str| {
			if path.exists() {
				let existing = fs::read_to_string(path).unwrap();
				if existing != content {
					eprintln!("Schema mismatch: {}", path.display());
					std::process::exit(1);
				}
			} else {
				eprintln!("Schema file missing: {}", path.display());
				std::process::exit(1);
			}
		};

		check_file(&release_path, &release_json);
		check_file(&config_path, &config_json);

		println!("Schemas are up to date.");
	}
}
