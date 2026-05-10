use std::fs;
use std::path::PathBuf;

use crate::check_schemas;
use crate::post_process;
use crate::post_process_config;
use crate::post_process_release;
use crate::run_with_paths;

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
	let kind = props.get("kind").unwrap().as_object().unwrap();

	// schemaVersion keeps default (evolves: 0.0 → 0.1 → 0.2)
	assert!(sv.contains_key("default"));
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
	let _ = fs::remove_dir_all(&schemas);
	let _ = fs::remove_dir_all(&docs);

	fs::create_dir_all(&schemas).unwrap();
	fs::create_dir_all(&docs).unwrap();

	assert!(run_with_paths(true, &schemas, &docs).is_ok());
	assert!(run_with_paths(false, &schemas, &docs).is_ok());

	let _ = fs::remove_dir_all(&schemas);
	let _ = fs::remove_dir_all(&docs);
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
