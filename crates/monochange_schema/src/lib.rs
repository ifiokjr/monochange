//! Durable JSON schema versions and migration metadata for monochange artifacts.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;
use serde::Serializer;
use serde_json::Value;
use thiserror::Error;

/// Current durable public schema version text.
///
/// This derives from the Cargo package version by stripping the patch component
/// at compile time.
pub const CURRENT_SCHEMA_VERSION_TEXT: &str = extract_major_minor(env!("CARGO_PKG_VERSION"));

const fn extract_major_minor(version: &str) -> &str {
	let bytes = version.as_bytes();
	let mut remaining = bytes;
	let mut index = 0;
	let mut dots = 0;

	while let Some((byte, rest)) = remaining.split_first() {
		if *byte == b'.' {
			dots += 1;
			if dots == 2 {
				break;
			}
		}
		remaining = rest;
		index += 1;
	}

	version.split_at(index).0
}

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
	#[error("artifact is missing required schema version field `v`")]
	MissingVersion,
	/// Current `v` field was not a string.
	#[error("artifact schema version field `v` must be a string")]
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
	/// Artifact used a non-current schema version.
	#[error(
		"artifact uses unsupported schema version `{actual}`; current supported version is `{current}`"
	)]
	UnsupportedVersion {
		/// Version found in the payload.
		actual: String,
		/// Current supported version.
		current: SchemaVersion,
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
	use crate::object_mut;
	use crate::parse_current_version;
	use crate::validate_kind;

	/// Durable artifact kind for commit-embedded release records.
	pub const KIND: &str = "monochange.releaseRecord";
	const INTERNAL_SCHEMA_VERSION_FIELD: &str = "schemaVersion";

	/// Return the current release-record schema version.
	pub fn current_version() -> Result<SchemaVersion, SchemaError> {
		Ok(current_schema_version_for_error())
	}

	/// Convert a release-record JSON value into the current durable wire shape.
	///
	/// This is intended for rendering new artifacts from existing in-memory domain
	/// structs. It writes `v` and removes internal-only `schemaVersion`.
	pub fn render_current_value(mut value: Value) -> Result<Value, SchemaError> {
		let object = object_mut(&mut value)?;
		validate_kind(object, KIND)?;
		object.remove(INTERNAL_SCHEMA_VERSION_FIELD);
		object.insert(
			"v".to_string(),
			Value::String(CURRENT_SCHEMA_VERSION_TEXT.to_string()),
		);
		Ok(value)
	}

	/// Validate a release-record JSON value against the current durable wire shape.
	///
	/// `0.0` is the first supported public schema version. Values without `v` or
	/// with any non-current `v` fail instead of taking a migration path.
	pub fn migrate_value(mut value: Value) -> Result<Value, SchemaError> {
		let object = object_mut(&mut value)?;
		validate_kind(object, KIND)?;
		let version_value = object.get("v").ok_or(SchemaError::MissingVersion)?;
		let version = parse_current_version(version_value)?;
		let current = current_version()?;
		if version != current {
			return Err(SchemaError::UnsupportedVersion {
				actual: version.to_string(),
				current,
			});
		}
		Ok(value)
	}
}

/// Machine-readable migration changelog entries.
pub mod migration_changelog {
	use serde::Serialize;

	use crate::SchemaVersion;

	/// All known durable migration changelog entries.
	///
	/// `0.0` is the first public schema version, so the initial changelog is
	/// intentionally empty. Future breaking changes add explicit edges here.
	pub const ENTRIES: &[MigrationChangelogEntry] = &[];

	/// A structured migration changelog entry.
	#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
	#[serde(rename_all = "camelCase")]
	pub struct MigrationChangelogEntry {
		/// Artifact kind this migration applies to.
		pub artifact: &'static str,
		/// Source version for the migration edge.
		pub from: MigrationSource,
		/// Destination `v` after migration.
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
			/// Source `v` value.
			v: SchemaVersion,
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
