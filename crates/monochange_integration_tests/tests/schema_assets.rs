use std::error::Error;
use std::path::Path;
use std::path::PathBuf;

use serde_json::Map;
use serde_json::Value;

#[test]
fn committed_schema_assets_are_json_and_hosted_copy_is_current() -> Result<(), Box<dyn Error>> {
	let paths = schema_asset_paths()?;

	parse_json(&paths.config_schema)?;
	parse_json(&paths.hosted_release_schema)?;
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

struct SchemaAssetPaths {
	root: PathBuf,
	root_config: PathBuf,
	config_schema: PathBuf,
	hosted_release_schema: PathBuf,
	canonical_release_schema: PathBuf,
	migration_changelog: PathBuf,
}

fn schema_asset_paths() -> Result<SchemaAssetPaths, Box<dyn Error>> {
	let root = workspace_root()?;
	Ok(SchemaAssetPaths {
		root_config: root.join("monochange.toml"),
		config_schema: root.join("docs/src/schemas/monochange.schema.json"),
		hosted_release_schema: root.join("docs/src/schemas/release-record.schema.json"),
		canonical_release_schema: root
			.join("crates/monochange_schema/schemas/release-record.schema.json"),
		migration_changelog: root.join("crates/monochange_schema/schemas/migration-changelog.json"),
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
