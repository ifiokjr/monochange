use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use tempfile::TempDir;

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

pub fn fixture_path_from(manifest_dir: &str, relative: &str) -> PathBuf {
	Path::new(manifest_dir)
		.join("../../fixtures/tests")
		.join(relative)
}

pub fn copy_directory(source: &Path, destination: &Path) {
	copy_directory_filtered(source, destination, &|_| false);
}

pub fn setup_fixture_from(manifest_dir: &str, relative: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path_from(manifest_dir, relative), tempdir.path());
	tempdir
}

pub fn setup_scenario_workspace_from(manifest_dir: &str, scenario_relative: &str) -> TempDir {
	let scenario_root = fixture_path_from(manifest_dir, scenario_relative);
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
