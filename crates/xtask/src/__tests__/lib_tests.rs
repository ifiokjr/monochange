use std::fs;
use std::path::PathBuf;

use crate::check_schemas;
use crate::command_literals_from_cli_source;
use crate::expected_schema_version;
use crate::post_process;
use crate::post_process_config;
use crate::post_process_release;
use crate::render_commands_inventory;
use crate::replace_commands_inventory;
use crate::run_with_paths;
use crate::schema_version_file_contents;

#[test]
fn command_literals_from_cli_source_collects_clap_commands() {
	let source = r#"
		.subcommand(Command::new("init"))
		.subcommand(Command::new("lint").subcommand(Command::new("list")))
	"#;
	let commands = command_literals_from_cli_source(source);

	assert!(commands.contains("init"));
	assert!(commands.contains("lint"));
	assert!(commands.contains("list"));
}

#[test]
fn replace_commands_inventory_replaces_marker_block() {
	let mut built_in = std::collections::BTreeSet::new();
	built_in.insert("init".to_string());
	let configured = std::collections::BTreeSet::new();
	let mut step_commands = std::collections::BTreeSet::new();
	step_commands.insert("step:validate".to_string());
	let expected = render_commands_inventory(&built_in, &configured, &step_commands);
	let current =
		"before\n<!-- xtask:commands:start -->\nstale\n<!-- xtask:commands:end -->\nafter";
	let updated = replace_commands_inventory(current, &expected).unwrap();

	assert!(updated.contains("before"));
	assert!(updated.contains("- `init`"));
	assert!(updated.contains("- `step:validate`"));
	assert!(updated.contains("after"));
	assert!(!updated.contains("stale"));
}

#[test]
fn replace_commands_inventory_requires_markers() {
	let error = replace_commands_inventory("missing", "expected").unwrap_err();

	assert!(error.contains("Missing command inventory start marker"));
}

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
		"9.9",
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
	let kind = props.get("kind").unwrap().as_object().unwrap();

	// schemaVersion keeps an overridable default so stale compiled constants do not hide drift.
	assert_eq!(sv.get("default"), Some(&serde_json::json!("9.9")));
	assert!(!sv.contains_key("const"));

	// kind is converted to const (fixed discriminator)
	assert!(!kind.contains_key("default"));
	assert_eq!(
		kind.get("const"),
		Some(&serde_json::Value::String(
			"monochange.releaseRecord".to_string()
		))
	);
}

#[test]
fn post_process_release_non_object() {
	let mut schema = serde_json::Value::Null;
	post_process_release(
		&mut schema,
		"https://example.com/release.json",
		"release record",
		"9.9",
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
		"9.9",
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
		"9.9",
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

	// packageDefinition should have additionalProperties: false
	let defs = obj.get("$defs").unwrap().as_object().unwrap();
	let pd = defs
		.get("RawPackageDefinition")
		.unwrap()
		.as_object()
		.unwrap();
	assert_eq!(
		pd.get("additionalProperties"),
		Some(&serde_json::Value::Bool(false))
	);

	// BumpSeverity should NOT have additionalProperties: false (no properties key)
	let bs = defs.get("BumpSeverity").unwrap().as_object().unwrap();
	assert!(!bs.contains_key("additionalProperties"));
}

#[test]
fn run_cli_round_trip() {
	let temp = std::env::temp_dir();
	let schemas = temp.join("xtask-test-schemas");
	let docs = temp.join("xtask-test-docs");
	let schema_version_path = temp.join("xtask-test-schema-version");
	let version = "7.8";
	let _ = fs::remove_dir_all(&schemas);
	let _ = fs::remove_dir_all(&docs);
	let _ = fs::remove_file(&schema_version_path);

	fs::create_dir_all(&schemas).unwrap();
	fs::create_dir_all(&docs).unwrap();

	assert!(run_with_paths(true, &schemas, &docs, &schema_version_path, version).is_ok());
	assert!(run_with_paths(false, &schemas, &docs, &schema_version_path, version).is_ok());
	assert_eq!(
		fs::read_to_string(&schema_version_path).unwrap(),
		schema_version_file_contents(version)
	);

	let artifacts = schemas.join("artifacts");
	assert!(artifacts.join("release-record.current.json").exists());
	assert!(
		artifacts
			.join(format!("release-record.v{version}.json"))
			.exists()
	);
	assert!(
		docs.join(format!("release-record.v{version}.schema.json"))
			.exists()
	);

	let _ = fs::remove_dir_all(&schemas);
	let _ = fs::remove_dir_all(&docs);
	let _ = fs::remove_file(&schema_version_path);
}

#[test]
fn expected_schema_version_applies_pre_1_major_bumps() {
	let workspace =
		std::env::temp_dir().join(format!("xtask-schema-version-{}", std::process::id()));
	let _ = fs::remove_dir_all(&workspace);
	fs::create_dir_all(workspace.join("crates/monochange_schema")).unwrap();
	fs::create_dir_all(workspace.join(".changeset")).unwrap();
	fs::write(
		workspace.join("crates/monochange_schema/Cargo.toml"),
		"[package]\nname = \"monochange_schema\"\nversion = \"0.1.1\"\n",
	)
	.unwrap();
	fs::write(
		workspace.join(".changeset/schema-major.md"),
		"---\nmonochange_schema: major\n---\n\nSchema migration.\n",
	)
	.unwrap();

	assert_eq!(expected_schema_version(&workspace).unwrap(), "0.2");
	let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn expected_schema_version_uses_largest_schema_changeset_bump() {
	let workspace = std::env::temp_dir().join(format!(
		"xtask-schema-version-largest-{}",
		std::process::id()
	));
	let _ = fs::remove_dir_all(&workspace);
	fs::create_dir_all(workspace.join("crates/monochange_schema")).unwrap();
	fs::create_dir_all(workspace.join(".changeset")).unwrap();
	fs::write(
		workspace.join("crates/monochange_schema/Cargo.toml"),
		"[package]\nname = \"monochange_schema\"\nversion = \"1.2.3\"\n",
	)
	.unwrap();
	fs::write(
		workspace.join(".changeset/schema-patch.md"),
		"---\nmonochange_schema:\n  bump: patch\n---\n\nPatch.\n",
	)
	.unwrap();
	fs::write(
		workspace.join(".changeset/schema-minor.md"),
		"---\nmonochange_schema: { bump: minor }\n---\n\nMinor.\n",
	)
	.unwrap();

	assert_eq!(expected_schema_version(&workspace).unwrap(), "1.3");
	let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn check_schemas_mismatch() {
	let temp_path = PathBuf::from("/tmp/test-schema-mismatch.json");
	fs::write(&temp_path, r#"{"a": 1}"#).unwrap();
	let paths = [(&temp_path, r#"{"a": 2}"#)];
	let result = check_schemas(&paths);
	fs::remove_file(&temp_path).unwrap();
	assert!(result.is_err());
	assert!(result.unwrap_err().contains("mismatch"));
}

#[test]
fn check_schemas_invalid_json() {
	let temp_path = PathBuf::from("/tmp/test-schema-invalid.json");
	fs::write(&temp_path, "not json").unwrap();
	let paths = [(&temp_path, r#"{}"#)];
	let result = check_schemas(&paths);
	fs::remove_file(&temp_path).unwrap();
	assert!(result.is_err());
	let msg = result.unwrap_err();
	assert!(msg.contains("mismatch"));
	assert!(msg.contains("invalid JSON"));
}

#[test]
fn check_schemas_formatting_difference_ok() {
	let temp_path = PathBuf::from("/tmp/test-schema-formatting.json");
	fs::write(&temp_path, "{\"a\":1}").unwrap();
	let paths = [(&temp_path, r#"{"a": 1}"#)];
	let result = check_schemas(&paths);
	fs::remove_file(&temp_path).unwrap();
	assert!(result.is_ok());
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

#[test]
fn check_schemas_expected_invalid_json() {
	let temp_path = PathBuf::from("/tmp/test-schema-expected-invalid.json");
	fs::write(&temp_path, r#"{"a": 1}"#).unwrap();
	let paths = [(&temp_path, "not json")];
	let result = check_schemas(&paths);
	fs::remove_file(&temp_path).unwrap();
	assert!(result.is_err());
	let msg = result.unwrap_err();
	assert!(msg.contains("invalid JSON"));
	assert!(msg.contains("Generated"));
}
