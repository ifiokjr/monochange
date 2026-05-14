use serde_json::Value;

use crate::SchemaError;
use crate::object_mut;

pub(crate) fn apply(value: &mut Value) -> Result<(), SchemaError> {
	object_mut(value)?;
	Ok(())
}
