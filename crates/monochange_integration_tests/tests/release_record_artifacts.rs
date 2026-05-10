use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use insta_cmd::get_cargo_bin;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_release_fixture() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path("release-pr/ungrouped"), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
	tempdir
}

fn prepare_release(root: &Path) -> Value {
	let output = Command::new(get_cargo_bin("mc"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-07")
		.arg("step:prepare-release")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run prepare-release: {error}"));
	assert!(
		output.status.success(),
		"prepare-release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse prepare-release json: {error}"))
}

fn release_record_paths(root: &Path) -> Vec<PathBuf> {
	let releases_dir = root.join(".monochange/releases");
	let mut paths = std::fs::read_dir(&releases_dir)
		.unwrap_or_else(|error| panic!("read {}: {error}", releases_dir.display()))
		.map(|entry| {
			entry
				.unwrap_or_else(|error| panic!("read release entry: {error}"))
				.path()
				.join("release.json")
		})
		.collect::<Vec<_>>();
	paths.sort();
	paths
}

#[test]
fn prepare_release_persists_one_record_and_reuses_it_on_later_runs() {
	let tempdir = setup_release_fixture();
	let root = tempdir.path();

	let first = prepare_release(root);
	let first_paths = release_record_paths(root);
	assert_eq!(first_paths.len(), 1);
	let first_record = std::fs::read_to_string(&first_paths[0])
		.unwrap_or_else(|error| panic!("read {}: {error}", first_paths[0].display()));
	let index_path = root.join(".monochange/local/release-index.jsonl");
	let first_index = std::fs::read_to_string(&index_path)
		.unwrap_or_else(|error| panic!("read {}: {error}", index_path.display()));

	let second = prepare_release(root);
	let second_paths = release_record_paths(root);
	assert_eq!(second_paths, first_paths);
	let second_record = std::fs::read_to_string(&second_paths[0])
		.unwrap_or_else(|error| panic!("read {}: {error}", second_paths[0].display()));
	let second_index = std::fs::read_to_string(&index_path)
		.unwrap_or_else(|error| panic!("read {}: {error}", index_path.display()));

	assert_eq!(second_record, first_record);
	assert_eq!(second_index, first_index);
	assert!(first_index.contains("1b9c77930352f342"));
	assert_eq!(
		first["releaseTargets"], second["releaseTargets"],
		"repeat prepare-release should compute the same release target identities"
	);
	let index_entries = first_index
		.lines()
		.map(|line| {
			serde_json::from_str::<Value>(line)
				.unwrap_or_else(|error| panic!("parse index line `{line}`: {error}"))
		})
		.collect::<Vec<_>>();
	assert_json_snapshot!(serde_json::json!({
		"releaseRecordPath": first_paths[0]
			.strip_prefix(root)
			.unwrap_or_else(|error| panic!("strip temp root: {error}"))
			.to_string_lossy(),
		"index": index_entries,
		"releaseTargets": first["releaseTargets"],
	}));
}
