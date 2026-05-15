//! Durable artifact migration registry.

mod release_record_0_0_to_0_1;
mod release_record_0_1_to_0_2;
mod release_record_0_2_to_0_3;

use serde_json::Value;

use crate::SchemaError;
use crate::SchemaVersion;
use crate::object_mut;
use crate::release_record;

pub(crate) struct MigrationEdge {
	from: SchemaVersion,
	to: SchemaVersion,
	apply: fn(&mut Value) -> Result<(), SchemaError>,
}

const RELEASE_RECORD_EDGES: &[MigrationEdge] = &[
	MigrationEdge {
		from: SchemaVersion::new(0, 0),
		to: SchemaVersion::new(0, 1),
		apply: release_record_0_0_to_0_1::apply,
	},
	MigrationEdge {
		from: SchemaVersion::new(0, 1),
		to: SchemaVersion::new(0, 2),
		apply: release_record_0_1_to_0_2::apply,
	},
	MigrationEdge {
		from: SchemaVersion::new(0, 2),
		to: SchemaVersion::new(0, 3),
		apply: release_record_0_2_to_0_3::apply,
	},
];

pub(crate) fn apply_release_record_edges(
	value: &mut Value,
	from: SchemaVersion,
	to: SchemaVersion,
) -> Result<(), SchemaError> {
	let mut cursor = from;
	while cursor != to {
		let Some(edge) = release_record_edge_from(cursor) else {
			return Err(missing_release_record_path(cursor, to));
		};
		if edge.to > to {
			return Err(missing_release_record_path(cursor, to));
		}
		(edge.apply)(value)?;
		cursor = edge.to;
	}
	Ok(())
}

fn release_record_edge_from(version: SchemaVersion) -> Option<&'static MigrationEdge> {
	debug_assert_eq!(
		RELEASE_RECORD_EDGES.len(),
		release_record_edge_versions().len()
	);
	RELEASE_RECORD_EDGES
		.iter()
		.find(|edge| edge.from == version)
}

fn missing_release_record_path(from: SchemaVersion, to: SchemaVersion) -> SchemaError {
	SchemaError::MissingMigrationPath {
		artifact: release_record::KIND,
		from,
		to,
	}
}

pub(crate) fn rename_top_level_field(
	value: &mut Value,
	source: &str,
	destination: &str,
) -> Result<(), SchemaError> {
	let object = object_mut(value)?;
	if let Some(field_value) = object.remove(source) {
		object.insert(destination.to_string(), field_value);
	}
	Ok(())
}

pub(crate) fn remove_top_level_field(value: &mut Value, field: &str) -> Result<(), SchemaError> {
	object_mut(value)?.remove(field);
	Ok(())
}

pub(crate) fn release_record_edge_versions() -> &'static [(SchemaVersion, SchemaVersion)] {
	const VERSIONS: &[(SchemaVersion, SchemaVersion)] = &[
		(SchemaVersion::new(0, 0), SchemaVersion::new(0, 1)),
		(SchemaVersion::new(0, 1), SchemaVersion::new(0, 2)),
		(SchemaVersion::new(0, 2), SchemaVersion::new(0, 3)),
	];
	VERSIONS
}
