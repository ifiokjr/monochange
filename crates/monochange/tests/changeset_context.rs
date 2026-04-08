use std::fs;
use std::path::Path;
use std::process::Command;

use insta_cmd::get_cargo_bin;
use serde_json::Value;
use tempfile::tempdir;

mod test_support;
use test_support::{copy_directory, fixture_path};

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command
}

#[test]
fn release_manifest_records_git_changeset_context_and_renders_context_templates() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	let fixture_root = fixture_path("changeset-context/base");
	copy_directory(&fixture_root, root);

	run_git(root, &["init"]);
	run_git(root, &["config", "user.name", "monochange Tests"]);
	run_git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	run_git(root, &["add", "Cargo.toml", "crates", "monochange.toml"]);
	run_git(root, &["commit", "-m", "chore: seed release fixture"]);
	run_git(root, &["add", ".changeset/feature.md"]);
	run_git(root, &["commit", "-m", "feat: add changeset"]);
	let introduced_sha = git_stdout(root, &["rev-parse", "HEAD"]).trim().to_string();

	let updated_fixture = fixture_path("changeset-context/with-updated-changeset");
	copy_directory(
		&updated_fixture.join(".changeset"),
		&root.join(".changeset"),
	);
	run_git(root, &["add", ".changeset/feature.md"]);
	run_git(root, &["commit", "-m", "docs: refine changeset details"]);
	let updated_sha = git_stdout(root, &["rev-parse", "HEAD"]).trim().to_string();

	let output = cli()
		.current_dir(root)
		.arg("release-manifest")
		.arg("--dry-run")
		.output()
		.unwrap_or_else(|error| panic!("release-manifest: {error}"));
	assert!(
		output.status.success(),
		"release-manifest failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let manifest_path = root.join(".monochange/release-manifest.json");
	let manifest = fs_read_to_string(&manifest_path);
	let parsed: Value =
		serde_json::from_str(&manifest).unwrap_or_else(|error| panic!("manifest json: {error}"));

	assert_eq!(
		parsed["changesets"][0]["path"].as_str(),
		Some(".changeset/feature.md")
	);
	assert_eq!(
		parsed["changesets"][0]["context"]["provider"].as_str(),
		Some("generic_git")
	);
	assert_eq!(
		parsed["changesets"][0]["context"]["introduced"]["commit"]["shortSha"].as_str(),
		Some(&introduced_sha[..7])
	);
	assert_eq!(
		parsed["changesets"][0]["context"]["lastUpdated"]["commit"]["shortSha"].as_str(),
		Some(&updated_sha[..7])
	);
	let rendered = parsed["changelogs"][0]["rendered"]
		.as_str()
		.unwrap_or_else(|| panic!("expected rendered changelog"));
	assert!(!rendered.contains("> _Changeset:_ `.changeset/feature.md`"));
	assert!(rendered.contains(&introduced_sha[..7]));
	assert!(rendered.contains(&updated_sha[..7]));
}

#[test]
fn diagnostics_command_reports_changeset_introduction_and_last_updated() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	let fixture_root = fixture_path("changeset-context/base");
	copy_directory(&fixture_root, root);

	run_git(root, &["init"]);
	run_git(root, &["config", "user.name", "monochange Tests"]);
	run_git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	run_git(root, &["add", "Cargo.toml", "crates", "monochange.toml"]);
	run_git(root, &["commit", "-m", "chore: seed release fixture"]);
	run_git(root, &["add", ".changeset/feature.md"]);
	run_git(root, &["commit", "-m", "feat: add changeset"]);
	let introduced_sha = git_stdout(root, &["rev-parse", "HEAD"]).trim().to_string();

	let updated_fixture = fixture_path("changeset-context/with-updated-changeset");
	copy_directory(
		&updated_fixture.join(".changeset"),
		&root.join(".changeset"),
	);
	run_git(root, &["add", ".changeset/feature.md"]);
	run_git(root, &["commit", "-m", "docs: refine changeset details"]);
	let updated_sha = git_stdout(root, &["rev-parse", "HEAD"]).trim().to_string();

	let output = cli()
		.current_dir(root)
		.arg("diagnostics")
		.args(["--changeset", ".changeset/feature.md", "--format", "json"])
		.output()
		.unwrap_or_else(|error| panic!("diagnostics: {error}"));
	assert!(
		output.status.success(),
		"diagnostics command failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	let stdout = String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("diagnostics output utf8: {error}"));
	let parsed: Value =
		serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("diagnostics json: {error}"));

	assert_eq!(
		parsed["requestedChangesets"][0].as_str(),
		Some(".changeset/feature.md")
	);
	assert_eq!(
		parsed["changesets"][0]["context"]["introduced"]["commit"]["shortSha"].as_str(),
		Some(&introduced_sha[..7])
	);
	assert_eq!(
		parsed["changesets"][0]["context"]["lastUpdated"]["commit"]["shortSha"].as_str(),
		Some(&updated_sha[..7])
	);
}

#[test]
fn diagnostics_command_reports_all_changesets_and_deduplicates_explicit_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	let fixture_root = fixture_path("changeset-context/base");
	copy_directory(&fixture_root, root);

	let output = cli()
		.current_dir(root)
		.arg("diagnostics")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("diagnostics: {error}"));
	assert!(
		output.status.success(),
		"diagnostics command failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	let parsed: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("diagnostics json: {error}"));
	let requested = parsed["requestedChangesets"]
		.as_array()
		.unwrap_or_else(|| panic!("requested changesets"));
	assert_eq!(requested.len(), 2);
	assert_eq!(requested[0].as_str(), Some(".changeset/feature.md"));
	assert_eq!(requested[1].as_str(), Some(".changeset/performance.md"));

	let duplicate_output = cli()
		.current_dir(root)
		.arg("diagnostics")
		.arg("--changeset")
		.arg(".changeset/feature.md")
		.arg("--changeset")
		.arg(".changeset/feature.md")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("diagnostics: {error}"));
	assert!(
		duplicate_output.status.success(),
		"diagnostics command failed: {}",
		String::from_utf8_lossy(&duplicate_output.stderr)
	);
	let duplicate_parsed: Value = serde_json::from_slice(&duplicate_output.stdout)
		.unwrap_or_else(|error| panic!("diagnostics json: {error}"));
	let duplicate_requested = duplicate_parsed["requestedChangesets"]
		.as_array()
		.unwrap_or_else(|| panic!("requested changesets"));
	assert_eq!(duplicate_requested.len(), 1);
	assert_eq!(
		duplicate_requested[0].as_str(),
		Some(".changeset/feature.md")
	);
}

fn run_git(root: &Path, args: &[&str]) {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(output.status.success(), "git {args:?} failed");
	String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("utf8: {error}"))
}

fn fs_read_to_string(path: &Path) -> String {
	fs::read_to_string(path)
		.unwrap_or_else(|error| panic!("read manifest {}: {error}", path.display()))
}
