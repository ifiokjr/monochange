//! Durable JSON schema versions and migration metadata for monochange artifacts.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;
use serde::Serializer;
use serde_json::Value;
use thiserror::Error;

include!(concat!(env!("OUT_DIR"), "/schema_version.rs"));

/// A durable schema version written as `major.minor`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SchemaVersion {
	major: u64,
	minor: u64,
}

impl SchemaVersion {
	/// Create a schema version from major and minor components.
	#[must_use]
	pub const fn new(major: u64, minor: u64) -> Self {
		Self { major, minor }
	}

	/// Major component.
	#[must_use]
	pub const fn major(self) -> u64 {
		self.major
	}

	/// Minor component.
	#[must_use]
	pub const fn minor(self) -> u64 {
		self.minor
	}

	/// Derive a schema version from a semantic package version string.
	pub fn from_package_version(package_version: &str) -> Result<Self, SchemaVersionParseError> {
		let (major, remainder) = package_version
			.split_once('.')
			.ok_or(SchemaVersionParseError::MissingMinor)?;
		let (minor, patch) = remainder
			.split_once('.')
			.ok_or(SchemaVersionParseError::MissingPatch)?;
		if patch.is_empty()
			|| patch.contains('.')
			|| !patch.chars().all(|character| character.is_ascii_digit())
		{
			return Err(SchemaVersionParseError::InvalidPatch(patch.to_string()));
		}
		let major = parse_component(major, SchemaVersionParseError::InvalidMajor)?;
		let minor = parse_component(minor, SchemaVersionParseError::InvalidMinor)?;
		Ok(Self { major, minor })
	}
}

impl fmt::Display for SchemaVersion {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(formatter, "{}.{}", self.major, self.minor)
	}
}

impl FromStr for SchemaVersion {
	type Err = SchemaVersionParseError;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		let (major, minor) = value
			.split_once('.')
			.ok_or(SchemaVersionParseError::MissingSeparator)?;
		if minor.contains('.') {
			return Err(SchemaVersionParseError::TooManyComponents);
		}
		let major = parse_component(major, SchemaVersionParseError::InvalidMajor)?;
		let minor = parse_component(minor, SchemaVersionParseError::InvalidMinor)?;
		Ok(Self { major, minor })
	}
}

impl Serialize for SchemaVersion {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_str(&self.to_string())
	}
}

fn parse_component(
	component: &str,
	make_error: fn(String) -> SchemaVersionParseError,
) -> Result<u64, SchemaVersionParseError> {
	if component.is_empty()
		|| !component
			.chars()
			.all(|character| character.is_ascii_digit())
	{
		return Err(make_error(component.to_string()));
	}
	component
		.parse::<u64>()
		.map_err(|_| make_error(component.to_string()))
}

/// Return the current durable schema version.
pub fn current_schema_version() -> Result<SchemaVersion, SchemaVersionParseError> {
	SchemaVersion::from_str(CURRENT_SCHEMA_VERSION_TEXT)
}

fn current_schema_version_for_error() -> SchemaVersion {
	current_schema_version().unwrap_or_else(|_| SchemaVersion::new(0, 0))
}

/// Errors while parsing `major.minor` schema versions.
#[derive(Debug, Clone, Eq, Error, PartialEq)]
pub enum SchemaVersionParseError {
	/// Version text did not contain a `.` separator.
	#[error("missing `.` separator")]
	MissingSeparator,
	/// Package version text did not contain a major component.
	#[error("missing major component")]
	MissingMajor,
	/// Package version text did not contain a minor component.
	#[error("missing minor component")]
	MissingMinor,
	/// Package version text did not contain a patch component.
	#[error("missing patch component")]
	MissingPatch,
	/// Version text had more than major/minor components.
	#[error("expected exactly major.minor")]
	TooManyComponents,
	/// Major component was not a non-negative integer.
	#[error("invalid major component `{0}`")]
	InvalidMajor(String),
	/// Minor component was not a non-negative integer.
	#[error("invalid minor component `{0}`")]
	InvalidMinor(String),
	/// Patch component was not a non-negative integer.
	#[error("invalid patch component `{0}`")]
	InvalidPatch(String),
}

/// Durable artifact migration error.
#[derive(Debug, Error)]
pub enum SchemaError {
	/// Artifact root was not a JSON object.
	#[error("artifact is not a JSON object")]
	NotObject,
	/// Artifact lacked a kind discriminator.
	#[error("artifact is missing required `kind`")]
	MissingKind,
	/// Artifact kind did not match the expected durable artifact.
	#[error("artifact uses unsupported kind `{actual}`; expected `{expected}`")]
	UnsupportedKind {
		/// Actual kind in the payload.
		actual: String,
		/// Expected artifact kind.
		expected: &'static str,
	},
	/// Artifact lacked the current version field.
	#[error("artifact is missing required schema version field `schemaVersion`")]
	MissingVersion,
	/// Current `schemaVersion` field was not a string.
	#[error("artifact schema version field `schemaVersion` must be a string")]
	NonStringVersion,
	/// Current `v` field could not be parsed.
	#[error("artifact uses invalid schema version `{version}`: {source}")]
	InvalidVersion {
		/// Invalid version text.
		version: String,
		/// Parse failure.
		source: SchemaVersionParseError,
	},
	/// Configured current schema version could not be parsed.
	#[error("current schema version `{version}` is invalid: {source}")]
	InvalidCurrentVersion {
		/// Invalid current schema version text.
		version: &'static str,
		/// Parse failure.
		source: SchemaVersionParseError,
	},
	/// Artifact used a schema version newer than this binary can read.
	#[error(
		"artifact uses unsupported schema version `{actual}`; current supported version is `{current}`"
	)]
	UnsupportedVersion {
		/// Version found in the payload.
		actual: String,
		/// Current supported version.
		current: SchemaVersion,
	},
	/// Artifact has no explicit migration path to the current schema version.
	#[error("artifact `{artifact}` has no migration path from schema version `{from}` to `{to}`")]
	MissingMigrationPath {
		/// Durable artifact kind.
		artifact: &'static str,
		/// Version found in the payload.
		from: SchemaVersion,
		/// Current supported version.
		to: SchemaVersion,
	},
	/// JSON conversion failure.
	#[error("artifact json error: {0}")]
	Json(#[from] serde_json::Error),
}

fn object_mut(value: &mut Value) -> Result<&mut serde_json::Map<String, Value>, SchemaError> {
	value.as_object_mut().ok_or(SchemaError::NotObject)
}

fn validate_kind(
	object: &serde_json::Map<String, Value>,
	expected: &'static str,
) -> Result<(), SchemaError> {
	let actual = object
		.get("kind")
		.and_then(Value::as_str)
		.ok_or(SchemaError::MissingKind)?;
	if actual != expected {
		return Err(SchemaError::UnsupportedKind {
			actual: actual.to_string(),
			expected,
		});
	}
	Ok(())
}

fn parse_current_version(value: &Value) -> Result<SchemaVersion, SchemaError> {
	let version = value.as_str().ok_or(SchemaError::NonStringVersion)?;
	SchemaVersion::from_str(version).map_err(|source| {
		SchemaError::InvalidVersion {
			version: version.to_string(),
			source,
		}
	})
}

/// Release-record durable artifact support.
pub mod release_record {
	use serde_json::Value;

	use crate::CURRENT_SCHEMA_VERSION_TEXT;
	use crate::SchemaError;
	use crate::SchemaVersion;
	use crate::current_schema_version_for_error;
	use crate::migration_changelog;
	use crate::object_mut;
	use crate::parse_current_version;
	use crate::validate_kind;

	/// Durable artifact kind for commit-embedded release records.
	pub const KIND: &str = "monochange.releaseRecord";
	const SCHEMA_VERSION_FIELD: &str = "schemaVersion";
	const LEGACY_VERSION_FIELD: &str = "v";

	/// Return the current release-record schema version.
	pub fn current_version() -> Result<SchemaVersion, SchemaError> {
		Ok(current_schema_version_for_error())
	}

	/// Render the current deterministic populated release-record artifact.
	#[must_use]
	pub fn current_populated_artifact_json() -> String {
		populated_artifact_json(CURRENT_SCHEMA_VERSION_TEXT)
	}

	/// Render a deterministic populated release-record artifact for a schema version.
	#[must_use]
	pub fn populated_artifact_json(version: &str) -> String {
		let release_version = format!("{version}.0");
		let tag_name = format!("v{release_version}");
		let schema_tag_name = format!("monochange_schema/v{release_version}");
		let versioned_artifact =
			format!("crates/monochange_schema/schemas/artifacts/release-record.v{version}.json");
		let artifact = serde_json::json!({
			"schemaVersion": version,
			"kind": KIND,
			"createdAt": "2026-01-01T00:00:00Z",
			"command": "mc release --commit",
			"version": release_version.as_str(),
			"versions": {
				"main": release_version.as_str(),
				"monochange_schema": release_version.as_str()
			},
			"releaseTargets": [
				{
					"id": "main",
					"kind": "group",
					"version": release_version.as_str(),
					"versionFormat": "primary",
					"tag": true,
					"release": true,
					"tagName": tag_name.as_str(),
					"members": ["monochange", "monochange_core"]
				},
				{
					"id": "monochange_schema",
					"kind": "package",
					"version": release_version.as_str(),
					"versionFormat": "namespaced",
					"tag": false,
					"release": false,
					"tagName": schema_tag_name.as_str(),
					"members": []
				}
			],
			"releasedPackages": ["monochange", "monochange_core", "monochange_schema"],
			"changedFiles": [
				"Cargo.toml",
				"crates/monochange_schema/Cargo.toml",
				"crates/monochange_schema/schemas/artifacts/current/release-record.json",
				versioned_artifact
			],
			"updatedChangelogs": ["changelog.md"],
			"deletedChangesets": [".changeset/release-record-schema-compat.md"],
			"changesets": [
				{
					"path": ".changeset/release-record-schema-compat.md",
					"summary": "Keep release record schema compatibility checks stable",
					"details": "Generated release-record artifact fixtures are parsed through the same migration path as commit-embedded records.",
					"targets": [
						{
							"id": "monochange_schema",
							"kind": "package",
							"bump": "major",
							"origin": "frontmatter",
							"evidenceRefs": ["crates/monochange_schema/src/lib.rs"],
							"changeType": "fix",
							"causedBy": ["release-record-schema-compat"]
						}
					]
				}
			],
			"provider": {
				"kind": "github",
				"owner": "monochange",
				"repo": "monochange",
				"host": "github.com"
			}
		});
		serde_json::to_string_pretty(&artifact)
			.unwrap_or_else(|error| panic!("serialize release-record artifact fixture: {error}"))
	}

	/// Convert a release-record JSON value into the current durable wire shape.
	///
	/// This is intended for rendering new artifacts from existing in-memory domain
	/// structs. It writes `schemaVersion` and removes legacy `v`.
	pub fn render_current_value(mut value: Value) -> Result<Value, SchemaError> {
		let object = object_mut(&mut value)?;
		validate_kind(object, KIND)?;
		object.remove(LEGACY_VERSION_FIELD);
		object.insert(
			SCHEMA_VERSION_FIELD.to_string(),
			Value::String(CURRENT_SCHEMA_VERSION_TEXT.to_string()),
		);
		Ok(value)
	}

	/// Validate a release-record JSON value against the current durable wire shape.
	///
	/// Release records are embedded in git commits and must remain readable by
	/// newer monochange binaries after schema upgrades. Older `schemaVersion`
	/// values must traverse explicit migration edges; missing edges fail instead
	/// of silently accepting older records. Future versions still fail so older
	/// binaries do not silently misread newer data. Legacy `v` fields are accepted
	/// as a compatibility bridge.
	pub fn migrate_value(mut value: Value) -> Result<Value, SchemaError> {
		let object = object_mut(&mut value)?;
		validate_kind(object, KIND)?;
		let version_value = object
			.get(SCHEMA_VERSION_FIELD)
			.or_else(|| object.get(LEGACY_VERSION_FIELD))
			.ok_or(SchemaError::MissingVersion)?;
		let version = parse_current_version(version_value)?;
		let current = current_version()?;
		if version > current {
			return Err(SchemaError::UnsupportedVersion {
				actual: version.to_string(),
				current,
			});
		}
		migrate_edges(&mut value, version, current)?;

		let object = object_mut(&mut value)?;
		object.remove(LEGACY_VERSION_FIELD);
		object.insert(
			SCHEMA_VERSION_FIELD.to_string(),
			Value::String(CURRENT_SCHEMA_VERSION_TEXT.to_string()),
		);
		Ok(value)
	}

	fn migrate_edges(
		value: &mut Value,
		from: SchemaVersion,
		to: SchemaVersion,
	) -> Result<(), SchemaError> {
		let mut cursor = from;
		while cursor != to {
			let Some(edge) = migration_edge_from(cursor) else {
				return Err(SchemaError::MissingMigrationPath {
					artifact: KIND,
					from: cursor,
					to,
				});
			};
			if edge.to > to {
				return Err(SchemaError::MissingMigrationPath {
					artifact: KIND,
					from: cursor,
					to,
				});
			}
			apply_migration_edge(value, edge)?;
			cursor = edge.to;
		}
		Ok(())
	}

	fn migration_edge_from(
		version: SchemaVersion,
	) -> Option<&'static migration_changelog::MigrationChangelogEntry> {
		migration_changelog::entries_for_artifact(KIND)
			.into_iter()
			.find(|entry| migration_source_version(entry.from) == version)
	}

	fn migration_source_version(source: migration_changelog::MigrationSource) -> SchemaVersion {
		match source {
			migration_changelog::MigrationSource::Version { schema_version } => schema_version,
		}
	}

	fn apply_migration_edge(
		_value: &mut Value,
		edge: &migration_changelog::MigrationChangelogEntry,
	) -> Result<(), SchemaError> {
		if edge.operation == migration_changelog::MigrationOperation::Noop && edge.noop {
			return Ok(());
		}
		Err(SchemaError::MissingMigrationPath {
			artifact: KIND,
			from: migration_source_version(edge.from),
			to: edge.to,
		})
	}
}

/// Configuration schema artifact support.
pub mod config {
	/// Render a deterministic populated workspace-configuration artifact.
	#[must_use]
	pub fn populated_artifact_json() -> String {
		let artifact = serde_json::json!({
			"source": {
				"owner": "monochange",
				"repo": "monochange",
				"provider": "github"
			}
		});
		serde_json::to_string_pretty(&artifact)
			.unwrap_or_else(|error| panic!("serialize config artifact fixture: {error}"))
	}
}

/// Machine-readable migration changelog entries.
pub mod migration_changelog {
	use serde::Serialize;

	use crate::SchemaVersion;

	/// All known durable migration changelog entries.
	pub const ENTRIES: &[MigrationChangelogEntry] = &[MigrationChangelogEntry {
		artifact: crate::release_record::KIND,
		from: MigrationSource::Version {
			schema_version: SchemaVersion::new(0, 1),
		},
		to: SchemaVersion::new(0, 2),
		operation: MigrationOperation::Noop,
		changes: &[],
		noop: true,
		reason: Some(
			"Release-record schema 0.2 preserves the 0.1 wire shape; the explicit edge keeps older artifact readability covered by CI.",
		),
	}];

	/// A structured migration changelog entry.
	#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
	#[serde(rename_all = "camelCase")]
	pub struct MigrationChangelogEntry {
		/// Artifact kind this migration applies to.
		pub artifact: &'static str,
		/// Source version for the migration edge.
		pub from: MigrationSource,
		/// Destination `schemaVersion` after migration.
		pub to: SchemaVersion,
		/// Summary operation for the edge.
		pub operation: MigrationOperation,
		/// Machine-readable field changes performed by this edge.
		pub changes: &'static [MigrationChange],
		/// Whether this edge intentionally leaves the payload unchanged.
		pub noop: bool,
		/// Human-readable reason for this edge.
		pub reason: Option<&'static str>,
	}

	/// A source schema version.
	#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
	#[serde(tag = "type", rename_all = "camelCase")]
	pub enum MigrationSource {
		/// Current string schema version field.
		Version {
			/// Source `schemaVersion` value.
			#[serde(rename = "schemaVersion")]
			schema_version: SchemaVersion,
		},
	}

	/// Machine-readable migration operation names.
	#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
	#[serde(rename_all = "snake_case")]
	pub enum MigrationOperation {
		/// Rename a field.
		RenameField,
		/// Add a field.
		AddField,
		/// Remove a field.
		RemoveField,
		/// Explicit no-op edge.
		Noop,
	}

	/// A single field-level migration change.
	#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
	#[serde(rename_all = "camelCase")]
	pub struct MigrationChange {
		/// Operation performed on this path.
		pub operation: MigrationOperation,
		/// JSON Pointer-like path affected by this change.
		pub path: &'static str,
		/// Replacement path/value, if applicable.
		pub replacement: Option<&'static str>,
		/// Explanation for this change.
		pub reason: Option<&'static str>,
	}

	/// Return migration entries for an artifact kind.
	#[must_use]
	pub fn entries_for_artifact(artifact: &str) -> Vec<&'static MigrationChangelogEntry> {
		ENTRIES
			.iter()
			.filter(|entry| entry.artifact == artifact)
			.collect()
	}

	/// Render the migration changelog as deterministic pretty JSON.
	pub fn to_json_pretty() -> Result<String, serde_json::Error> {
		serde_json::to_string_pretty(ENTRIES)
	}
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
