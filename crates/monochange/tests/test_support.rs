use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use insta_cmd::get_cargo_bin;
use serde_json::{Map, Value};
use tempfile::TempDir;

pub fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

#[allow(dead_code)]
pub fn copy_directory(source: &Path, destination: &Path) {
	copy_directory_filtered(source, destination, &|_| false);
}

#[allow(dead_code)]
pub fn setup_scenario_workspace(scenario_relative: &str) -> TempDir {
	let scenario_root = fixture_path(scenario_relative);
	let workspace_root = scenario_root.join("workspace");
	let source_root = if workspace_root.is_dir() {
		workspace_root
	} else {
		scenario_root
	};
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory_filtered(&source_root, tempdir.path(), &|path| {
		path.file_name().is_some_and(|name| name == "expected")
	});
	tempdir
}

#[allow(dead_code)]
pub fn expected_fixture_path(scenario_relative: &str, relative: &str) -> PathBuf {
	fixture_path(scenario_relative)
		.join("expected")
		.join(relative)
}

#[allow(dead_code)]
pub fn monochange_command(release_date: Option<&str>) -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	if let Some(release_date) = release_date {
		command.env("MONOCHANGE_RELEASE_DATE", release_date);
	}
	command
}

#[allow(dead_code)]
pub fn run_json_command(root: &Path, command: &str, release_date: Option<&str>) -> Value {
	let output = monochange_command(release_date)
		.current_dir(root)
		.arg(command)
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse command json: {error}"))
}

#[allow(dead_code)]
pub fn assert_json_fixture(actual: &Value, expected_path: &Path) {
	let expected = fs::read_to_string(expected_path).unwrap_or_else(|error| {
		panic!("read expected fixture {}: {error}", expected_path.display())
	});
	let expected_json: Value = serde_json::from_str(&expected).unwrap_or_else(|error| {
		panic!(
			"parse expected fixture {}: {error}",
			expected_path.display()
		)
	});
	let normalized_expected = normalize_json_value(&expected_json);
	let normalized_actual = normalize_json_value(actual);
	if normalized_expected != normalized_actual {
		similar_asserts::assert_eq!(
			serde_json::to_string_pretty(&normalized_expected)
				.unwrap_or_else(|error| panic!("serialize expected fixture: {error}")),
			serde_json::to_string_pretty(&normalized_actual)
				.unwrap_or_else(|error| panic!("serialize actual json: {error}")),
			"json fixture mismatch: {}",
			expected_path.display()
		);
	}
}

fn normalize_json_value(value: &Value) -> Value {
	match value {
		Value::Array(items) => Value::Array(items.iter().map(normalize_json_value).collect()),
		Value::Object(entries) => Value::Object(
			entries
				.iter()
				.map(|(key, value)| (key.clone(), normalize_json_value(value)))
				.collect(),
		),
		Value::String(text) => Value::String(normalize_temp_paths(text)),
		_ => value.clone(),
	}
}

fn normalize_temp_paths(value: &str) -> String {
	let mut normalized = value.to_string();
	for root in temp_path_roots() {
		normalized = replace_temp_root_instances(&normalized, &root);
	}
	normalized
}

fn temp_path_roots() -> Vec<String> {
	let mut roots = Vec::new();
	let temp_dir = std::env::temp_dir();
	let temp_root = temp_dir.to_string_lossy().trim_end_matches('/').to_string();
	if !temp_root.is_empty() {
		roots.push(temp_root.clone());
		if let Some(stripped) = temp_root.strip_prefix("/private") {
			roots.push(stripped.to_string());
		} else if temp_root.starts_with("/var/") {
			roots.push(format!("/private{temp_root}"));
		}
	}
	for fallback in ["/private/tmp", "/tmp"] {
		if !roots.iter().any(|root| root == fallback) {
			roots.push(fallback.to_string());
		}
	}
	roots.sort();
	roots.dedup();
	roots
}

fn replace_temp_root_instances(value: &str, root: &str) -> String {
	let mut output = String::new();
	let mut rest = value;
	while let Some(index) = rest.find(root) {
		output.push_str(&rest[..index]);
		let mut consumed = index + root.len();
		let tail = &rest[consumed..];
		let mut chars = tail.chars();
		if chars.next().is_some_and(|ch| ch == '/' || ch == '\\') {
			consumed += 1;
		}
		let suffix = &rest[consumed..];
		let component_end = suffix
			.find(['/', '\\', ' ', '\n', '\t', ':', ')', ']', ',', '"', '\''])
			.unwrap_or(suffix.len());
		if component_end == 0 {
			output.push_str(root);
			rest = &rest[index + root.len()..];
			continue;
		}
		consumed += component_end;
		output.push_str("[ROOT]");
		rest = &rest[consumed..];
	}
	output.push_str(rest);
	output
}

fn copy_directory_filtered(source: &Path, destination: &Path, skipped: &dyn Fn(&Path) -> bool) {
	fs::create_dir_all(destination)
		.unwrap_or_else(|error| panic!("create destination {}: {error}", destination.display()));
	for entry in fs::read_dir(source)
		.unwrap_or_else(|error| panic!("read dir {}: {error}", source.display()))
	{
		let entry = entry.unwrap_or_else(|error| panic!("dir entry: {error}"));
		let source_path = entry.path();
		if skipped(&source_path) {
			continue;
		}
		let destination_path = destination.join(entry.file_name());
		let metadata = fs::metadata(&source_path)
			.unwrap_or_else(|error| panic!("metadata {}: {error}", source_path.display()));
		if metadata.is_dir() {
			copy_directory_filtered(&source_path, &destination_path, skipped);
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
