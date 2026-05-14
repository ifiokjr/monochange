use serde_json::Value;

use crate::SchemaError;
use crate::release_record::LEGACY_VERSION_FIELD;
use crate::release_record::SCHEMA_VERSION_FIELD;

pub(crate) fn apply(value: &mut Value) -> Result<(), SchemaError> {
	super::rename_top_level_field(value, LEGACY_VERSION_FIELD, SCHEMA_VERSION_FIELD)
}
