use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

const SCHEMA_VERSION_FILE: &str = "SCHEMA_VERSION";

fn main() {
	let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|error| {
		panic!("read CARGO_MANIFEST_DIR: {error}");
	}));
	let schema_version_path = manifest_dir.join(SCHEMA_VERSION_FILE);
	println!("cargo:rerun-if-changed={}", schema_version_path.display());

	let schema_version = schema_version_from_file(&schema_version_path);

	let out_dir = env::var("OUT_DIR").unwrap_or_else(|error| {
		panic!("read OUT_DIR: {error}");
	});
	let dest_path = Path::new(&out_dir).join("schema_version.rs");
	fs::write(
		&dest_path,
		format!(
			"/// Current durable public schema version text.\n\
			///\n\
			/// Generated from `SCHEMA_VERSION`, which is updated by `schema:update`\n\
			/// and verified by `schema:check`.\n\
			pub const CURRENT_SCHEMA_VERSION_TEXT: &str = \"{schema_version}\";\n"
		),
	)
	.unwrap_or_else(|error| panic!("write {}: {error}", dest_path.display()));
}

fn schema_version_from_file(path: &Path) -> String {
	let contents =
		fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
	let schema_version = contents.trim();
	assert!(
		!schema_version.is_empty(),
		"{} must not be empty",
		path.display()
	);
	validate_schema_version(schema_version)
		.unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()));
	schema_version.to_string()
}

fn validate_schema_version(schema_version: &str) -> Result<(), String> {
	let Some((major, minor)) = schema_version.split_once('.') else {
		return Err("expected major.minor".to_string());
	};
	if minor.contains('.') {
		return Err("expected exactly major.minor".to_string());
	}
	validate_component("major", major)?;
	validate_component("minor", minor)
}

fn validate_component(name: &str, component: &str) -> Result<(), String> {
	if component.is_empty()
		|| !component
			.chars()
			.all(|character| character.is_ascii_digit())
	{
		return Err(format!("invalid {name} component `{component}`"));
	}
	Ok(())
}
