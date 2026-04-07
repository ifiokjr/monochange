use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

pub fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

pub fn copy_directory(source: &Path, destination: &Path) {
	fs::create_dir_all(destination)
		.unwrap_or_else(|error| panic!("create destination {}: {error}", destination.display()));
	for entry in fs::read_dir(source)
		.unwrap_or_else(|error| panic!("read dir {}: {error}", source.display()))
	{
		let entry = entry.unwrap_or_else(|error| panic!("dir entry: {error}"));
		let source_path = entry.path();
		let destination_path = destination.join(entry.file_name());
		let metadata = fs::metadata(&source_path)
			.unwrap_or_else(|error| panic!("metadata {}: {error}", source_path.display()));
		if metadata.is_dir() {
			copy_directory(&source_path, &destination_path);
		} else if metadata.is_file() {
			if let Some(parent) = destination_path.parent() {
				fs::create_dir_all(parent)
					.unwrap_or_else(|error| panic!("create parent {}: {error}", parent.display()));
			}
			fs::copy(&source_path, &destination_path).unwrap_or_else(|error| {
				panic!(
					"copy {} -> {}: {error}",
					source_path.display(),
					destination_path.display()
				)
			});
		}
	}
}

#[allow(dead_code)]
pub fn snapshot_settings() -> insta::Settings {
	let mut settings = insta::Settings::clone_current();
	settings.add_filter(r"/private/var/folders/[^\s]+?/T/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/var/folders/[^\s]+?/T/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/private/tmp/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/tmp/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/home/runner/work/_temp/[^/\s]+", "[ROOT]");
	settings.add_filter(r"\b[A-Z]:\\[^\s]+?\\Temp\\[^\\\s]+", "[ROOT]");
	settings.add_filter(r"SourceOffset\(\d+\)", "SourceOffset([OFFSET])");
	settings.add_filter(r"length: \d+", "length: [LEN]");
	settings.add_filter(r"@ bytes \d+\.\.\d+", "@ bytes [OFFSET]..[END]");
	settings.add_filter(r"\b[0-9a-f]{7,40}\b", "[SHA]");
	settings.add_filter(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}", "[DATETIME]");
	settings.add_filter(r"\d{4}-\d{2}-\d{2}", "[DATE]");
	settings
}

#[allow(dead_code)]
pub fn json_subset(value: &Value, fields: &[(&str, &str)]) -> Value {
	let mut subset = Map::new();
	for (key, pointer) in fields {
		subset.insert(
			(*key).to_string(),
			value.pointer(pointer).cloned().unwrap_or(Value::Null),
		);
	}
	Value::Object(subset)
}
