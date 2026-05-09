use serde_json::Value;
use serde_json::json;

use crate::CURRENT_SCHEMA_VERSION_TEXT;
use crate::SchemaError;
use crate::SchemaVersion;
use crate::SchemaVersionParseError;
use crate::current_schema_version;
use crate::extract_major_minor;
use crate::migration_changelog;
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
fn extract_major_minor_strips_patch_at_runtime() {
	assert_eq!(extract_major_minor("1.42.3"), "1.42");
	assert_eq!(extract_major_minor("1.42"), "1.42");
}

#[test]
fn current_schema_version_strips_patch_from_package_version() {
	let pkg_version = env!("CARGO_PKG_VERSION");

	let (expected_major, expected_minor) = {
		let parts: Vec<&str> = pkg_version.split('.').take(2).collect();
		(
			parts.first().unwrap_or(&"0").parse::<u64>().unwrap_or(0),
			parts.get(1).unwrap_or(&"0").parse::<u64>().unwrap_or(0),
		)
	};
	let expected_text = format!("{expected_major}.{expected_minor}");

	assert_eq!(CURRENT_SCHEMA_VERSION_TEXT, expected_text);
	let current = current_schema_version()
		.unwrap_or_else(|error| panic!("parse current schema version: {error}"));
	assert_eq!(current, SchemaVersion::new(expected_major, expected_minor));
	let from_package = SchemaVersion::from_package_version(pkg_version)
		.unwrap_or_else(|error| panic!("parse package version: {error}"));
	assert_eq!(
		from_package,
		SchemaVersion::new(expected_major, expected_minor)
	);
	let serialized = serde_json::to_value(current)
		.unwrap_or_else(|error| panic!("serialize schema version: {error}"));
	assert_eq!(serialized, json!(expected_text));
}

#[test]
fn release_record_accepts_current_schema_version() {
	let migrated = release_record::migrate_value(json!({
		"v": CURRENT_SCHEMA_VERSION_TEXT,
		"kind": release_record::KIND,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.unwrap_or_else(|error| panic!("validate release record: {error}"));

	assert_eq!(migrated.get("v"), Some(&json!(CURRENT_SCHEMA_VERSION_TEXT)));
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

	assert_eq!(rendered.get("v"), Some(&json!(CURRENT_SCHEMA_VERSION_TEXT)));
	assert!(
		rendered.get("schemaVersion").is_none(),
		"internal schemaVersion must not leak into durable records"
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
		"v": 1,
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
		"v": "0.1.0",
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
fn release_record_rejects_old_version_without_migration_edge() {
	let error = release_record::migrate_value(json!({
		"v": "9.0",
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
fn release_record_rejects_unsupported_kind() {
	let error = release_record::migrate_value(json!({
		"v": "0.1",
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
		"v": "9.0",
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
fn migration_changelog_is_machine_readable_json() {
	let json = migration_changelog::to_json_pretty()
		.unwrap_or_else(|error| panic!("migration changelog json: {error}"));
	assert_eq!(json, "[]");
	assert!(migration_changelog::entries_for_artifact(release_record::KIND).is_empty());
}

#[test]
fn committed_release_record_schema_tracks_current_wire_constants() {
	let release_record_schema = include_str!("../../schemas/release-record.schema.json");
	let schema = serde_json::from_str::<Value>(release_record_schema)
		.unwrap_or_else(|error| panic!("release record schema json: {error}"));

	assert_eq!(
		schema
			.pointer("/properties/v/const")
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
	let changelog = include_str!("../../schemas/migration-changelog.json");

	serde_json::from_str::<Value>(release_record_schema)
		.unwrap_or_else(|error| panic!("release record schema json: {error}"));
	serde_json::from_str::<Value>(changelog)
		.unwrap_or_else(|error| panic!("migration changelog json: {error}"));
}

#[test]
fn committed_migration_changelog_is_current() {
	let generated = migration_changelog::to_json_pretty()
		.unwrap_or_else(|error| panic!("migration changelog json: {error}"));
	let committed_path =
		std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas/migration-changelog.json");
	let committed_result = std::fs::read_to_string(&committed_path);
	assert!(
		committed_result.is_ok(),
		"read committed migration changelog"
	);
	let committed = committed_result.unwrap_or_default();
	assert_eq!(committed, format!("{generated}\n"));
}
