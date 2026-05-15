use serde_json::Value;

use crate::SchemaError;
use crate::object_mut;

/// Migrate release-record artifacts from schema version 0.2 to 0.3.
///
/// This is a no-op migration: the release-record shape did not change between
/// 0.2 and 0.3, but the config schema gained new backward-compatible fields
/// (`ChangelogStyle`, `ChangelogSectionDef`, etc.) requiring a version bump.
pub(crate) fn apply(value: &mut Value) -> Result<(), SchemaError> {
	object_mut(value)?;
	Ok(())
}
