use std::error::Error;
use std::path::Path;
use std::path::PathBuf;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;

#[test]
fn committed_schema_assets_are_json_and_hosted_copy_is_current() -> Result<(), Box<dyn Error>> {
	let paths = schema_asset_paths()?;

	parse_json(&paths.config_schema)?;
	parse_json(&paths.versioned_config_schema)?;
	parse_json(&paths.hosted_release_schema)?;
	parse_json(&paths.versioned_release_schema)?;
	parse_json(&paths.canonical_release_schema)?;
	parse_json(&paths.migration_changelog)?;

	assert_eq!(
		std::fs::read_to_string(&paths.canonical_release_schema)?,
		std::fs::read_to_string(&paths.hosted_release_schema)?
	);

	Ok(())
}

#[test]
fn release_record_schema_declares_current_artifact_contract() -> Result<(), Box<dyn Error>> {
	let paths = schema_asset_paths()?;
	let schema = parse_json(&paths.canonical_release_schema)?;

	assert_eq!(
		json_str(&schema, "/$id")?,
		"https://monochange.github.io/monochange/schemas/release-record.schema.json"
	);
	assert!(!json_bool(&schema, "/additionalProperties")?);
	assert_eq!(
		json_str(&schema, "/properties/v/const")?,
		monochange_schema::CURRENT_SCHEMA_VERSION_TEXT
	);
	assert_eq!(
		json_str(&schema, "/properties/kind/const")?,
		monochange_schema::release_record::KIND
	);

	let required = json_array(&schema, "/required")?;
	for key in [
		"v",
		"kind",
		"createdAt",
		"command",
		"releaseTargets",
		"releasedPackages",
		"changedFiles",
	] {
		assert!(
			required.iter().any(|value| value.as_str() == Some(key)),
			"release-record schema is missing required key `{key}`"
		);
	}

	Ok(())
}

#[test]
fn versioned_schema_assets_use_stable_ids_without_changing_contracts() -> Result<(), Box<dyn Error>>
{
	let paths = schema_asset_paths()?;
	let release_current = parse_json(&paths.hosted_release_schema)?;
	let release_versioned = parse_json(&paths.versioned_release_schema)?;
	let config_current = parse_json(&paths.config_schema)?;
	let config_versioned = parse_json(&paths.versioned_config_schema)?;

	let release_versioned_id = format!(
		"https://monochange.github.io/monochange/schemas/release-record.v{}.schema.json",
		monochange_schema::CURRENT_SCHEMA_VERSION_TEXT
	);
	let config_versioned_id = format!(
		"https://monochange.github.io/monochange/schemas/monochange.v{}.schema.json",
		monochange_schema::CURRENT_SCHEMA_VERSION_TEXT
	);

	assert_eq!(json_str(&release_versioned, "/$id")?, release_versioned_id);
	assert_eq!(json_str(&config_versioned, "/$id")?, config_versioned_id);
	assert_eq!(
		with_schema_id(release_current, &release_versioned_id)?,
		release_versioned
	);
	assert_eq!(
		with_schema_id(config_current, &config_versioned_id)?,
		config_versioned
	);

	Ok(())
}

#[test]
fn root_monochange_toml_schema_directive_points_to_committed_schema() -> Result<(), Box<dyn Error>>
{
	let paths = schema_asset_paths()?;
	let config_text = std::fs::read_to_string(&paths.root_config)?;
	let Some(first_line) = config_text.lines().next() else {
		return Err(test_error("monochange.toml is empty"));
	};
	let Some(schema_path) = first_line.strip_prefix("#:schema ") else {
		return Err(test_error(
			"monochange.toml must start with a Taplo #:schema directive",
		));
	};

	assert_eq!(schema_path, "./docs/src/schemas/monochange.schema.json");
	assert_eq!(paths.root.join(schema_path), paths.config_schema);
	assert!(paths.config_schema.is_file());

	let parsed_config = toml::from_str::<toml::Value>(&config_text)?;
	let Some(config_table) = parsed_config.as_table() else {
		return Err(test_error("monochange.toml root is not a TOML table"));
	};
	assert!(
		!config_table.contains_key("$schema"),
		"schema hint should stay a comment so strict config parsing does not need to accept `$schema`"
	);

	Ok(())
}

#[test]
fn config_schema_covers_current_root_toml_top_level_keys() -> Result<(), Box<dyn Error>> {
	let paths = schema_asset_paths()?;
	let schema = parse_json(&paths.config_schema)?;
	let config_text = std::fs::read_to_string(&paths.root_config)?;
	let parsed_config = toml::from_str::<toml::Value>(&config_text)?;
	let Some(config_table) = parsed_config.as_table() else {
		return Err(test_error("monochange.toml root is not a TOML table"));
	};
	let schema_properties = json_object(&schema, "/properties")?;

	assert_eq!(
		json_str(&schema, "/$id")?,
		"https://monochange.github.io/monochange/schemas/monochange.schema.json"
	);
	assert!(!json_bool(&schema, "/additionalProperties")?);

	for key in config_table.keys() {
		assert!(
			schema_properties.contains_key(key),
			"config schema is missing root monochange.toml key `{key}`"
		);
	}

	Ok(())
}

#[test]
fn config_schema_preserves_dynamic_tables_while_closing_known_shapes() -> Result<(), Box<dyn Error>>
{
	let paths = schema_asset_paths()?;
	let schema = parse_json(&paths.config_schema)?;

	for pointer in [
		"/properties/package",
		"/properties/group",
		"/properties/cli",
	] {
		let section = json_object(&schema, pointer)?;
		assert!(
			section.contains_key("additionalProperties"),
			"{pointer} should allow user-defined table names"
		);
	}

	for pointer in [
		"/$defs/packageDefinition/additionalProperties",
		"/$defs/groupDefinition/additionalProperties",
		"/$defs/cliCommand/additionalProperties",
		"/$defs/ecosystemSettings/additionalProperties",
		"/$defs/source/additionalProperties",
		"/$defs/defaults/additionalProperties",
	] {
		assert!(!json_bool(&schema, pointer)?, "{pointer} should be closed");
	}

	assert!(
		json_object(&schema, "/$defs/lints/properties/rules")?.contains_key("additionalProperties"),
		"lint rule names must remain dynamic"
	);

	Ok(())
}

#[test]
fn schema_asset_inventory_matches_snapshot() -> Result<(), Box<dyn Error>> {
	let paths = schema_asset_paths()?;
	let release_schema = parse_json(&paths.canonical_release_schema)?;
	let config_schema = parse_json(&paths.config_schema)?;
	let changelog = parse_json(&paths.migration_changelog)?;

	let inventory = json!({
		"currentSchemaVersion": monochange_schema::CURRENT_SCHEMA_VERSION_TEXT,
		"schemaCrateVersion": schema_crate_version(&paths)?,
		"releaseRecord": {
			"kind": monochange_schema::release_record::KIND,
			"schemaId": json_str(&release_schema, "/$id")?,
			"required": json_array(&release_schema, "/required")?,
			"additionalProperties": json_bool(&release_schema, "/additionalProperties")?,
		},
		"configuration": {
			"schemaId": json_str(&config_schema, "/$id")?,
			"dynamicTables": ["package", "group", "cli"],
			"additionalProperties": json_bool(&config_schema, "/additionalProperties")?,
		},
		"migrationChangelog": changelog,
	});

	assert_json_snapshot!(inventory);

	Ok(())
}

#[test]
fn release_record_schema_multiline_fields_are_snapshot_individually() -> Result<(), Box<dyn Error>>
{
	let paths = schema_asset_paths()?;
	let release_schema = parse_json(&paths.canonical_release_schema)?;
	let config_schema = parse_json(&paths.config_schema)?;
	let changelog = parse_json(&paths.migration_changelog)?;

	assert_snapshot!(
		"release_record_schema_description",
		json_str(&release_schema, "/description")?
	);
	assert_json_snapshot!(
		"release_record_required_fields",
		json_array(&release_schema, "/required")?
	);
	assert_snapshot!(
		"config_schema_description",
		json_str(&config_schema, "/description")?
	);
	assert_json_snapshot!("migration_changelog_entries", changelog);

	Ok(())
}

#[test]
fn schema_crate_version_stays_decoupled_from_public_schema_version() -> Result<(), Box<dyn Error>> {
	let paths = schema_asset_paths()?;

	assert_eq!(schema_crate_version(&paths)?, "0.0.0");
	assert_eq!(monochange_schema::CURRENT_SCHEMA_VERSION_TEXT, "0.0");
	assert_eq!(
		monochange_schema::current_schema_version()?,
		monochange_schema::SchemaVersion::new(0, 0)
	);

	Ok(())
}

#[test]
fn release_record_migration_outcomes_match_snapshot() {
	let mut missing_version = sample_release_record();
	if let Some(object) = missing_version.as_object_mut() {
		object.remove("v");
	}

	let mut missing_kind = sample_release_record();
	if let Some(object) = missing_kind.as_object_mut() {
		object.remove("kind");
	}

	let mut pre_public_shape = sample_release_record();
	if let Some(object) = pre_public_shape.as_object_mut() {
		object.remove("v");
		object.insert("schemaVersion".to_string(), json!(1));
	}

	let scenarios = vec![
		("current", sample_release_record()),
		("not_object", json!(["not", "an", "object"])),
		("missing_kind", missing_kind),
		(
			"wrong_kind",
			sample_release_record_with("0.1", "monochange.otherRecord"),
		),
		("missing_version", missing_version),
		("pre_public_schema_version_field", pre_public_shape),
		(
			"non_string_version",
			sample_release_record_with_value(
				json!(1),
				json!(monochange_schema::release_record::KIND),
			),
		),
		(
			"invalid_version_text",
			sample_release_record_with("0.1.0", monochange_schema::release_record::KIND),
		),
		(
			"old_version_without_migration_edge",
			sample_release_record_with("0.1", monochange_schema::release_record::KIND),
		),
		(
			"future_version",
			sample_release_record_with("0.2", monochange_schema::release_record::KIND),
		),
	];
	let outcomes = scenarios
		.into_iter()
		.map(|(scenario, value)| {
			match monochange_schema::release_record::migrate_value(value) {
				Ok(value) => {
					json!({
						"scenario": scenario,
						"status": "ok",
						"v": value.get("v"),
					})
				}
				Err(error) => {
					json!({
						"scenario": scenario,
						"status": "error",
						"error": error.to_string(),
					})
				}
			}
		})
		.collect::<Vec<_>>();

	assert_json_snapshot!(outcomes);
}

struct SchemaAssetPaths {
	root: PathBuf,
	root_config: PathBuf,
	config_schema: PathBuf,
	versioned_config_schema: PathBuf,
	hosted_release_schema: PathBuf,
	versioned_release_schema: PathBuf,
	canonical_release_schema: PathBuf,
	migration_changelog: PathBuf,
	schema_crate_manifest: PathBuf,
}

fn schema_asset_paths() -> Result<SchemaAssetPaths, Box<dyn Error>> {
	let root = workspace_root()?;
	Ok(SchemaAssetPaths {
		root_config: root.join("monochange.toml"),
		config_schema: root.join("docs/src/schemas/monochange.schema.json"),
		versioned_config_schema: root.join(format!(
			"docs/src/schemas/monochange.v{}.schema.json",
			monochange_schema::CURRENT_SCHEMA_VERSION_TEXT
		)),
		hosted_release_schema: root.join("docs/src/schemas/release-record.schema.json"),
		versioned_release_schema: root.join(format!(
			"docs/src/schemas/release-record.v{}.schema.json",
			monochange_schema::CURRENT_SCHEMA_VERSION_TEXT
		)),
		canonical_release_schema: root
			.join("crates/monochange_schema/schemas/release-record.schema.json"),
		migration_changelog: root.join("crates/monochange_schema/schemas/migration-changelog.json"),
		schema_crate_manifest: root.join("crates/monochange_schema/Cargo.toml"),
		root,
	})
}

fn workspace_root() -> Result<PathBuf, Box<dyn Error>> {
	let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
	let Some(crates_dir) = manifest_dir.parent() else {
		return Err(test_error("integration test crate has no parent directory"));
	};
	let Some(root) = crates_dir.parent() else {
		return Err(test_error("crates directory has no parent directory"));
	};
	Ok(root.to_path_buf())
}

fn parse_json(path: &Path) -> Result<Value, Box<dyn Error>> {
	let contents = std::fs::read_to_string(path)?;
	let value = serde_json::from_str(&contents)?;
	Ok(value)
}

fn schema_crate_version(paths: &SchemaAssetPaths) -> Result<String, Box<dyn Error>> {
	let manifest = std::fs::read_to_string(&paths.schema_crate_manifest)?;
	let parsed = toml::from_str::<toml::Value>(&manifest)?;
	let Some(version) = parsed
		.get("package")
		.and_then(|package| package.get("version"))
		.and_then(toml::Value::as_str)
	else {
		return Err(test_error(
			"monochange_schema manifest is missing package.version",
		));
	};
	Ok(version.to_string())
}

fn with_schema_id(mut value: Value, schema_id: &str) -> Result<Value, Box<dyn Error>> {
	let Some(object) = value.as_object_mut() else {
		return Err(test_error("schema root is not a JSON object"));
	};
	object.insert("$id".to_string(), Value::String(schema_id.to_string()));
	Ok(value)
}

fn sample_release_record() -> Value {
	sample_release_record_with(
		monochange_schema::CURRENT_SCHEMA_VERSION_TEXT,
		monochange_schema::release_record::KIND,
	)
}

fn sample_release_record_with(version: &str, kind: &str) -> Value {
	sample_release_record_with_value(json!(version), json!(kind))
}

fn sample_release_record_with_value(version: Value, kind: Value) -> Value {
	json!({
		"v": version,
		"kind": kind,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	})
}

fn json_object<'a>(
	value: &'a Value,
	pointer: &str,
) -> Result<&'a Map<String, Value>, Box<dyn Error>> {
	let Some(object) = value.pointer(pointer).and_then(Value::as_object) else {
		return Err(test_error(format!("expected JSON object at `{pointer}`")));
	};
	Ok(object)
}

fn json_array<'a>(value: &'a Value, pointer: &str) -> Result<&'a [Value], Box<dyn Error>> {
	let Some(array) = value.pointer(pointer).and_then(Value::as_array) else {
		return Err(test_error(format!("expected JSON array at `{pointer}`")));
	};
	Ok(array)
}

fn json_str<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, Box<dyn Error>> {
	let Some(text) = value.pointer(pointer).and_then(Value::as_str) else {
		return Err(test_error(format!("expected JSON string at `{pointer}`")));
	};
	Ok(text)
}

fn json_bool(value: &Value, pointer: &str) -> Result<bool, Box<dyn Error>> {
	let Some(boolean) = value.pointer(pointer).and_then(Value::as_bool) else {
		return Err(test_error(format!("expected JSON boolean at `{pointer}`")));
	};
	Ok(boolean)
}

fn test_error(message: impl Into<String>) -> Box<dyn Error> {
	std::io::Error::other(message.into()).into()
}
