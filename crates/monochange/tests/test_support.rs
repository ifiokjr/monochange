use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

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
pub fn current_test_name() -> String {
	let current = thread::current();
	let name = current
		.name()
		.unwrap_or("unknown")
		.split("::")
		.last()
		.unwrap_or("unknown");
	if let Some(rest) = name.strip_prefix("case_") {
		if let Some((index, suffix)) = rest.split_once('_') {
			if index.chars().all(|ch| ch.is_ascii_digit()) && !suffix.is_empty() {
				return suffix.to_string();
			}
		}
	}
	name.to_string()
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
