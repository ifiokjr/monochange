use serde_json::Value;
use serde_json::json;

use crate::CURRENT_SCHEMA_VERSION_TEXT;
use crate::SchemaError;
use crate::SchemaVersion;
use crate::SchemaVersionParseError;
use crate::config;
use crate::current_schema_version;
use crate::release_record;

#[test]
fn schema_version_parses_major_minor_only() {
	let version: SchemaVersion = "8.2"
		.parse()
		.unwrap_or_else(|error| panic!("parse schema version: {error}"));
	assert_eq!(version.major(), 8);
	assert_eq!(version.minor(), 2);
	assert_eq!(version.to_string(), "8.2");
	assert!("8.2.1".parse::<SchemaVersion>().is_err());
	assert!("8".parse::<SchemaVersion>().is_err());
	assert!("8.x".parse::<SchemaVersion>().is_err());
}

#[test]
fn package_version_parser_reports_component_errors() {
	assert!(matches!(
		SchemaVersion::from_package_version(""),
		Err(SchemaVersionParseError::MissingMinor)
	));
	assert!(matches!(
		SchemaVersion::from_package_version("1"),
		Err(SchemaVersionParseError::MissingMinor)
	));
	assert!(matches!(
		SchemaVersion::from_package_version("x.2.3"),
		Err(SchemaVersionParseError::InvalidMajor(major)) if major == "x"
	));
	assert!(matches!(
		SchemaVersion::from_package_version(".2.3"),
		Err(SchemaVersionParseError::InvalidMajor(major)) if major.is_empty()
	));
	assert!(matches!(
		SchemaVersion::from_package_version("1.x.3"),
		Err(SchemaVersionParseError::InvalidMinor(minor)) if minor == "x"
	));
	assert!(matches!(
		SchemaVersion::from_package_version("1.2.x"),
		Err(SchemaVersionParseError::InvalidPatch(patch)) if patch == "x"
	));
	assert!(matches!(
		SchemaVersion::from_package_version("1."),
		Err(SchemaVersionParseError::MissingPatch)
	));
}

#[test]
fn current_schema_version_is_not_behind_package_version() {
	let package_version = env!("CARGO_PKG_VERSION");
	let current = current_schema_version()
		.unwrap_or_else(|error| panic!("parse current schema version: {error}"));
	let package = SchemaVersion::from_package_version(package_version)
		.unwrap_or_else(|error| panic!("parse package version: {error}"));
	assert!(
		current >= package,
		"current durable schema {current} must not lag package-derived schema {package}"
	);
	let serialized = serde_json::to_value(current)
		.unwrap_or_else(|error| panic!("serialize schema version: {error}"));
	assert_eq!(serialized, json!(CURRENT_SCHEMA_VERSION_TEXT));
	assert_eq!(
		serde_json::to_value(package).unwrap(),
		json!(package.to_string())
	);
}

#[test]
fn populated_release_record_artifact_uses_current_schema_version() {
	let version = CURRENT_SCHEMA_VERSION_TEXT;
	let json = release_record::current_populated_artifact_json();
	let value: Value = serde_json::from_str(&json)
		.unwrap_or_else(|error| panic!("parse populated release record artifact: {error}"));

	assert_eq!(value["schemaVersion"], version);
	assert_eq!(value["kind"], release_record::KIND);
	assert_eq!(value["releaseTargets"].as_array().unwrap().len(), 2);
	assert_eq!(value["changesets"].as_array().unwrap().len(), 1);
	assert!(
		value["changedFiles"]
			.as_array()
			.unwrap()
			.iter()
			.any(|entry| {
				entry
					== &json!(
						"crates/monochange_schema/schemas/artifacts/current/release-record/01.json"
					)
			})
	);
	assert!(
		!value["changedFiles"]
			.as_array()
			.unwrap()
			.iter()
			.any(|entry| {
				entry
					.as_str()
					.is_some_and(|entry| entry.contains("release-record.v"))
			})
	);
}

#[test]
fn populated_config_artifact_is_deterministic() {
	let first = config::populated_artifact_json();
	let second = config::populated_artifact_json();
	assert_eq!(first, second);
	let value: Value = serde_json::from_str(&first)
		.unwrap_or_else(|error| panic!("parse populated config artifact: {error}"));
	assert_eq!(value["source"]["owner"], "monochange");
	assert_eq!(value["source"]["repo"], "monochange");
}

#[test]
fn release_record_accepts_current_schema_version() {
	let migrated = release_record::migrate_value(json!({
		"schemaVersion": CURRENT_SCHEMA_VERSION_TEXT,
		"kind": release_record::KIND,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.unwrap_or_else(|error| panic!("validate release record: {error}"));

	assert_eq!(
		migrated.get("schemaVersion"),
		Some(&json!(CURRENT_SCHEMA_VERSION_TEXT))
	);
}

#[test]
fn release_record_migrates_older_schema_versions() {
	let migrated = release_record::migrate_value(json!({
		"schemaVersion": "0.1",
		"kind": release_record::KIND,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.unwrap_or_else(|error| panic!("migrate old release record: {error}"));

	assert_eq!(
		migrated.get("schemaVersion"),
		Some(&json!(CURRENT_SCHEMA_VERSION_TEXT))
	);
}

#[test]
fn release_record_migrates_legacy_v_only_schema_version() {
	let migrated = release_record::migrate_value(json!({
		"v": "0.0",
		"kind": release_record::KIND,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.unwrap_or_else(|error| panic!("migrate legacy release record: {error}"));

	assert_eq!(
		migrated.get("schemaVersion"),
		Some(&json!(CURRENT_SCHEMA_VERSION_TEXT))
	);
	assert!(migrated.get("v").is_none());
}

#[test]
fn release_record_rust_migration_helpers_apply_supported_changes() {
	let mut value = json!({
		"oldName": "kept",
		"removed": true,
		"other": "stable"
	});

	release_record::rename_top_level_field(&mut value, "oldName", "newName")
		.unwrap_or_else(|error| panic!("rename field: {error}"));
	release_record::remove_top_level_field(&mut value, "removed")
		.unwrap_or_else(|error| panic!("remove field: {error}"));

	assert_eq!(value.get("newName"), Some(&json!("kept")));
	assert!(value.get("oldName").is_none());
	assert!(value.get("removed").is_none());
	assert_eq!(value.get("other"), Some(&json!("stable")));
}

#[test]
fn release_record_rust_migration_edges_are_explicit_and_ordered() {
	assert_eq!(
		release_record::migration_edge_versions(),
		&[
			(SchemaVersion::new(0, 0), SchemaVersion::new(0, 1)),
			(SchemaVersion::new(0, 1), SchemaVersion::new(0, 2)),
		]
	);
}

#[test]
fn release_record_rust_migration_helpers_reject_non_object_values() {
	let mut value = json!(null);
	let error = release_record::rename_top_level_field(&mut value, "oldName", "newName")
		.err()
		.unwrap_or_else(|| panic!("expected non-object rename error"));
	assert!(matches!(error, SchemaError::NotObject));

	let error = release_record::remove_top_level_field(&mut value, "removed")
		.err()
		.unwrap_or_else(|| panic!("expected non-object remove error"));
	assert!(matches!(error, SchemaError::NotObject));
}

#[test]
fn release_record_render_current_value_writes_public_version_only() {
	let rendered = release_record::render_current_value(json!({
		"schemaVersion": 1,
		"kind": release_record::KIND,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.unwrap_or_else(|error| panic!("render current release record: {error}"));

	assert_eq!(
		rendered.get("schemaVersion"),
		Some(&json!(CURRENT_SCHEMA_VERSION_TEXT))
	);
	assert!(
		rendered.get("v").is_none(),
		"legacy `v` must not leak into durable records"
	);
}

#[test]
fn release_record_render_current_value_rejects_non_object_or_missing_kind() {
	let not_object = release_record::render_current_value(json!([]))
		.err()
		.unwrap_or_else(|| panic!("expected non-object error"));
	assert!(matches!(not_object, SchemaError::NotObject));

	let missing_kind = release_record::render_current_value(json!({
		"schemaVersion": 1,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected missing-kind error"));
	assert!(matches!(missing_kind, SchemaError::MissingKind));
}

#[test]
fn release_record_rejects_missing_version() {
	let error = release_record::migrate_value(json!({
		"kind": release_record::KIND,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected missing version error"));
	assert!(matches!(error, SchemaError::MissingVersion));
}

#[test]
fn release_record_rejects_non_string_version() {
	let error = release_record::migrate_value(json!({
		"schemaVersion": 1,
		"kind": release_record::KIND,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected non-string version error"));
	assert!(matches!(error, SchemaError::NonStringVersion));
}

#[test]
fn release_record_rejects_invalid_version_text() {
	let error = release_record::migrate_value(json!({
		"schemaVersion": "0.1.0",
		"kind": release_record::KIND,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected invalid version error"));
	assert!(matches!(
		error,
		SchemaError::InvalidVersion { version, .. } if version == "0.1.0"
	));
}

#[test]
fn release_record_rejects_unsupported_kind() {
	let error = release_record::migrate_value(json!({
		"schemaVersion": "0.1",
		"kind": "monochange.otherRecord",
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected unsupported kind error"));
	assert!(matches!(
		error,
		SchemaError::UnsupportedKind { actual, expected }
			if actual == "monochange.otherRecord" && expected == release_record::KIND
	));
}

#[test]
fn release_record_rejects_future_version() {
	let error = release_record::migrate_value(json!({
		"schemaVersion": "9.0",
		"kind": release_record::KIND,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected unsupported version error"));
	assert!(matches!(
		error,
		SchemaError::UnsupportedVersion { actual, .. } if actual == "9.0"
	));
}

#[test]
fn committed_release_record_schema_tracks_current_wire_constants() {
	let release_record_schema = include_str!("../../schemas/release-record.schema.json");
	let schema = serde_json::from_str::<Value>(release_record_schema)
		.unwrap_or_else(|error| panic!("release record schema json: {error}"));

	assert_eq!(
		schema
			.pointer("/properties/schemaVersion/default")
			.and_then(Value::as_str),
		Some(CURRENT_SCHEMA_VERSION_TEXT)
	);
	assert_eq!(
		schema
			.pointer("/properties/kind/const")
			.and_then(Value::as_str),
		Some(release_record::KIND)
	);
	assert_eq!(
		schema
			.pointer("/additionalProperties")
			.and_then(Value::as_bool),
		Some(false)
	);
}

#[test]
fn committed_json_schema_files_parse() {
	let release_record_schema = include_str!("../../schemas/release-record.schema.json");
	serde_json::from_str::<Value>(release_record_schema)
		.unwrap_or_else(|error| panic!("release record schema json: {error}"));
}
