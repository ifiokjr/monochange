use std::env;
use std::fs;
use std::path::Path;

fn main() {
	let version = env::var("CARGO_PKG_VERSION").unwrap();
	let mut parts = version.split('.');
	let major = parts.next().unwrap();
	let minor = parts.next().unwrap();
	let schema_version = format!("{major}.{minor}");

	let out_dir = env::var("OUT_DIR").unwrap();
	let dest_path = Path::new(&out_dir).join("schema_version.rs");
	fs::write(
		&dest_path,
		format!(r#"pub const CURRENT_SCHEMA_VERSION_TEXT: &str = "{schema_version}";"#),
	)
	.unwrap();
}
