use std::error::Error;
use std::path::Path;
use std::path::PathBuf;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;

const HISTORICAL_RELEASE_RECORD_ARTIFACT: &str = "release-record.v0.1.json";

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
fn release_record_artifact_fixtures_load_through_parser() -> Result<(), Box<dyn Error>> {
	let paths = schema_asset_paths()?;
	let artifact_paths = release_record_artifact_paths(&paths)?;
	let names = artifact_paths
		.iter()
		.map(|path| file_name(path))
		.collect::<Result<Vec<_>, _>>()?;
	let current_versioned_name = format!(
		"release-record.v{}.json",
		monochange_schema::CURRENT_SCHEMA_VERSION_TEXT
	);

	assert!(names.iter().any(|name| name == "release-record.json"));
	assert!(names.iter().any(|name| name == &current_versioned_name));
	assert!(
		names
			.iter()
			.any(|name| name == HISTORICAL_RELEASE_RECORD_ARTIFACT),
		"expected at least one historical release-record artifact fixture"
	);

	for artifact_path in artifact_paths {
		let name = file_name(&artifact_path)?;
		let text = std::fs::read_to_string(&artifact_path)?;
		let raw = serde_json::from_str::<Value>(&text)?;
		let raw_schema_version = json_str(&raw, "/schemaVersion")?;
		let record = monochange_core::parse_release_record_json(&text)?;

		if name == "release-record.json" || name == current_versioned_name {
			assert_eq!(
				raw_schema_version,
				monochange_schema::CURRENT_SCHEMA_VERSION_TEXT
			);
		}
		assert_eq!(
			record.schema_version,
			monochange_schema::CURRENT_SCHEMA_VERSION_TEXT,
			"{name} should migrate to the current schema version"
		);
		assert_eq!(record.kind, monochange_schema::release_record::KIND);
		assert!(
			!record.release_targets.is_empty(),
			"{name} should exercise release targets"
		);
		assert!(
			!record.released_packages.is_empty(),
			"{name} should exercise released packages"
		);
		assert!(
			!record.changed_files.is_empty(),
			"{name} should exercise changed files"
		);
		assert!(
			!record.changesets.is_empty(),
			"{name} should exercise embedded changeset records"
		);
	}

	Ok(())
}

#[test]
fn config_artifact_fixtures_are_valid_json() -> Result<(), Box<dyn Error>> {
	let paths = schema_asset_paths()?;
	let artifact_paths = config_artifact_paths(&paths)?;
	let names = artifact_paths
		.iter()
		.map(|path| file_name(path))
		.collect::<Result<Vec<_>, _>>()?;
	let current_versioned_name = format!(
		"monochange.v{}.json",
		monochange_schema::CURRENT_SCHEMA_VERSION_TEXT
	);

	assert!(names.iter().any(|name| name == "monochange.json"));
	assert!(names.iter().any(|name| name == &current_versioned_name));

	let expected: Value =
		serde_json::from_str(&monochange_schema::config::populated_artifact_json())?;
	for artifact_path in artifact_paths {
		let name = file_name(&artifact_path)?;
		let text = std::fs::read_to_string(&artifact_path)?;
		let raw: Value = serde_json::from_str(&text)?;

		assert_eq!(raw, expected, "{name} should match the generated fixture");
	}

	Ok(())
}

#[test]
fn current_artifact_fixtures_are_colocated() -> Result<(), Box<dyn Error>> {
	let paths = schema_asset_paths()?;
	let mut names = Vec::new();
	for entry in std::fs::read_dir(&paths.current_artifacts_dir)? {
		let path = entry?.path();
		if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
			names.push(file_name(&path)?);
		}
	}
	names.sort();

	assert_eq!(names, vec!["monochange.json", "release-record.json"]);
	assert!(!paths.artifacts_dir.join("monochange.current.json").exists());
	assert!(
		!paths
			.artifacts_dir
			.join("release-record.current.json")
			.exists()
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
		json_str(&schema, "/properties/schemaVersion/default")?,
		monochange_schema::CURRENT_SCHEMA_VERSION_TEXT
	);
	assert_eq!(
		json_str(&schema, "/properties/kind/const")?,
		monochange_schema::release_record::KIND
	);

	let required = json_array(&schema, "/required")?;
	for key in [
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

	let lints_schema = json_object(&schema, "/properties/lints")?;
	for keyword in ["additionalProperties", "properties", "type"] {
		assert!(
			!lints_schema.contains_key(keyword),
			"lints schema should not restrict lint rule shapes with `{keyword}`"
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

	assert_json_snapshot!(inventory, {
		".schemaCrateVersion" => "[schema crate version]",
		".currentSchemaVersion" => "[schema version]"
	});

	Ok(())
}

#[test]
fn release_record_schema_multiline_fields_are_snapshot_individually() -> Result<(), Box<dyn Error>>
{
	let paths = schema_asset_paths()?;
	let release_schema = parse_json(&paths.canonical_release_schema)?;
	let config_schema = parse_json(&paths.config_schema)?;
	let changelog = parse_json(&paths.migration_changelog)?;

	let description = json_str(&release_schema, "/description")?;
	// The committed schema description can lag or lead the crate's current schema
	// version while release PRs regenerate assets. Redact any artifact-version
	// number here so this snapshot protects the wording instead of coupling CI to
	// the transient release bump order.
	let redacted_description = redact_schema_description_version(description);
	assert_snapshot!("release_record_schema_description", redacted_description);
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

	// Read the actual crate version from the Cargo.toml so the test doesn't break on every release bump.
	let manifest = std::fs::read_to_string(&paths.schema_crate_manifest)?;
	let parsed = toml::from_str::<toml::Value>(&manifest)?;
	let expected_version = parsed
		.get("package")
		.and_then(|package| package.get("version"))
		.and_then(toml::Value::as_str)
		.unwrap_or_default();

	assert_eq!(schema_crate_version(&paths)?, expected_version);
	let manifest_schema = monochange_schema::SchemaVersion::from_package_version(expected_version)
		.map_err(|error| test_error(format!("invalid manifest version: {error}")))?;
	let current_schema = monochange_schema::current_schema_version()?;
	assert!(
		current_schema >= manifest_schema,
		"current durable schema {current_schema} must not lag manifest-derived schema {manifest_schema}"
	);
	assert_eq!(
		monochange_schema::CURRENT_SCHEMA_VERSION_TEXT,
		current_schema.to_string()
	);

	Ok(())
}

#[test]
fn release_record_migration_outcomes_match_snapshot() {
	let mut missing_version = sample_release_record();
	if let Some(object) = missing_version.as_object_mut() {
		object.remove("schemaVersion");
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

	let current_version = monochange_schema::CURRENT_SCHEMA_VERSION_TEXT;
	let invalid_version_text = format!("{current_version}.0");
	let future_version = unsupported_schema_version();
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
			sample_release_record_with(
				&invalid_version_text,
				monochange_schema::release_record::KIND,
			),
		),
		(
			"old_version_without_migration_edge",
			sample_release_record(),
		),
		(
			"future_version",
			sample_release_record_with(&future_version, monochange_schema::release_record::KIND),
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
						"schemaVersion": value.get("schemaVersion"),
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

	let redacted_outcomes: Vec<Value> = outcomes
		.into_iter()
		.map(|mut outcome| {
			if let Value::Object(ref mut map) = outcome {
				if let Some(v) = map.get_mut("schemaVersion")
					&& let Value::String(_) = v
				{
					*v = Value::String("[schema version]".to_string());
				}
				if let Some(v) = map.get_mut("v")
					&& let Value::String(_) = v
				{
					*v = Value::String("[schema version]".to_string());
				}
				if let Some(v) = map.get_mut("error")
					&& let Value::String(s) = v
				{
					*v = Value::String(redact_schema_versions(
						s,
						&[current_version, &future_version],
					));
				}
			}
			outcome
		})
		.collect();

	assert_json_snapshot!(redacted_outcomes);
}

// Snapshot redaction is intentional: schema versions are derived from release
// package versions, so release PRs must prove behavior without baking the next
// version number into expected output.
fn redact_schema_description_version(text: &str) -> String {
	let marker = "artifact version ";
	let Some(marker_start) = text.find(marker) else {
		return text.replace(
			monochange_schema::CURRENT_SCHEMA_VERSION_TEXT,
			"[schema version]",
		);
	};
	let version_start = marker_start + marker.len();
	let Some(version_len) = schema_version_prefix_len(&text[version_start..]) else {
		return text.to_string();
	};
	let version_end = version_start + version_len;
	format!(
		"{}[schema version]{}",
		&text[..version_start],
		&text[version_end..]
	)
}

fn schema_version_prefix_len(text: &str) -> Option<usize> {
	let mut seen_separator = false;
	let mut seen_minor_digit = false;
	let mut end = 0;
	for (index, character) in text.char_indices() {
		if character.is_ascii_digit() {
			if seen_separator {
				seen_minor_digit = true;
			}
			end = index + character.len_utf8();
			continue;
		}
		if character == '.' && !seen_separator {
			seen_separator = true;
			end = index + character.len_utf8();
			continue;
		}
		break;
	}
	seen_minor_digit.then_some(end)
}

fn redact_schema_versions(text: &str, versions: &[&str]) -> String {
	versions.iter().fold(text.to_string(), |redacted, version| {
		redacted.replace(version, "[schema version]")
	})
}

fn unsupported_schema_version() -> String {
	let current = monochange_schema::current_schema_version()
		.unwrap_or_else(|error| panic!("parse current schema version: {error}"));
	monochange_schema::SchemaVersion::new(current.major(), current.minor() + 1).to_string()
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
	artifacts_dir: PathBuf,
	current_artifacts_dir: PathBuf,
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
		artifacts_dir: root.join("crates/monochange_schema/schemas/artifacts"),
		current_artifacts_dir: root.join("crates/monochange_schema/schemas/artifacts/current"),
		root,
	})
}

fn release_record_artifact_paths(paths: &SchemaAssetPaths) -> Result<Vec<PathBuf>, Box<dyn Error>> {
	let mut artifact_paths = vec![paths.current_artifacts_dir.join("release-record.json")];
	for entry in std::fs::read_dir(&paths.artifacts_dir)? {
		let path = entry?.path();
		let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
			continue;
		};
		if name.starts_with("release-record.v") && name.ends_with(".json") {
			artifact_paths.push(path);
		}
	}
	artifact_paths.sort();
	if artifact_paths.is_empty() {
		return Err(test_error("no release-record artifact fixtures were found"));
	}
	for artifact_path in &artifact_paths {
		if !artifact_path.exists() {
			return Err(test_error(format!(
				"missing release-record artifact fixture: {}",
				artifact_path.display()
			)));
		}
	}
	Ok(artifact_paths)
}

fn config_artifact_paths(paths: &SchemaAssetPaths) -> Result<Vec<PathBuf>, Box<dyn Error>> {
	let mut artifact_paths = vec![paths.current_artifacts_dir.join("monochange.json")];
	for entry in std::fs::read_dir(&paths.artifacts_dir)? {
		let path = entry?.path();
		let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
			continue;
		};
		if name.starts_with("monochange.v") && name.ends_with(".json") {
			artifact_paths.push(path);
		}
	}
	artifact_paths.sort();
	if artifact_paths.is_empty() {
		return Err(test_error("no config artifact fixtures were found"));
	}
	for artifact_path in &artifact_paths {
		if !artifact_path.exists() {
			return Err(test_error(format!(
				"missing config artifact fixture: {}",
				artifact_path.display()
			)));
		}
	}
	Ok(artifact_paths)
}

fn file_name(path: &Path) -> Result<String, Box<dyn Error>> {
	let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
		return Err(test_error(format!(
			"path has no UTF-8 file name: {}",
			path.display()
		)));
	};
	Ok(name.to_string())
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
		"schemaVersion": version,
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
