use std::fs;
use std::path::Path;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleaseRecord;
use monochange_core::ReleaseRecordProvider;
use monochange_core::ReleaseRecordTarget;
use monochange_core::SourceProvider;
use monochange_core::VersionFormat;
use monochange_core::render_release_record_block;
use monochange_test_helpers::git;
use monochange_test_helpers::git_output_trimmed;
use serde_json::Value;
use tempfile::TempDir;

mod test_support;
use test_support::current_test_name;
use test_support::fixture_path;
use test_support::monochange_command;
use test_support::setup_fixture;
use test_support::snapshot_settings;

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_command_reports_record_from_tag_as_json() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	let release_record = sample_release_record();
	commit_release_record(repo, &release_record);
	git(repo, &["tag", "v1.2.3"]);

	let output = release_record_output(repo, &["--from", "v1.2.3", "--format", "json"]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!("json: {error}\n{}", String::from_utf8_lossy(&output.stdout))
	});
	assert_json_snapshot!(parsed);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_command_walks_first_parent_ancestry_from_head() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	let release_record = sample_release_record();
	commit_release_record(repo, &release_record);
	git(repo, &["tag", "v1.2.3"]);
	commit_plain(
		repo,
		"fix: package release artifacts",
		"release-record/after-release",
	);
	commit_plain(
		repo,
		"fix: format generated files",
		"release-record/after-release-again",
	);

	let output = release_record_output(repo, &["--from", "HEAD"]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_snapshot!(
		String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout utf8: {error}"))
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_command_reports_unresolved_refs() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();

	let output = release_record_output(repo, &["--from", "missing-tag"]);
	assert!(!output.status.success());
	assert_snapshot!(
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"))
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_command_reports_missing_record_in_history() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();

	let output = release_record_output(repo, &["--from", "HEAD"]);
	assert!(!output.status.success());
	assert_snapshot!(
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"))
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_command_fails_loudly_on_malformed_record_in_ancestry() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	let release_record = sample_release_record();
	commit_release_record(repo, &release_record);
	commit_with_body(
		repo,
		"chore(release): malformed release record",
		"## monochange Release Record\n\n<!-- monochange:release-record:start -->\n```json\n{}\n```",
		"release-record/commit-body-alt",
	);
	commit_plain(repo, "fix: follow-up", "release-record/follow-up");

	let output = release_record_output(repo, &["--from", "HEAD"]);
	assert!(!output.status.success());
	assert_snapshot!(
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"))
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_command_reports_unsupported_schema_version() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	let body = r#"Prepare release.

## monochange Release Record

<!-- monochange:release-record:start -->
```json
{
  "schemaVersion": 2,
  "kind": "monochange.releaseRecord",
  "createdAt": "2026-04-07T08:00:00Z",
  "command": "release-pr",
  "releaseTargets": [],
  "releasedPackages": [],
  "changedFiles": []
}
```
<!-- monochange:release-record:end -->"#;
	commit_with_body(
		repo,
		"chore(release): prepare release",
		body,
		"release-record/commit-body",
	);

	let output = release_record_output(repo, &["--from", "HEAD"]);
	assert!(!output.status.success());
	assert_snapshot!(
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"))
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn tag_release_command_creates_and_pushes_declared_tags() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	configure_origin_remote(repo);
	let release_record = sample_release_record();
	let (commit, _) = commit_release_record(repo, &release_record);
	git(repo, &["push", "-u", "origin", "HEAD:main"]);

	let output = tag_release_output(repo, &["--from", "HEAD", "--format", "json"]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!("json: {error}\n{}", String::from_utf8_lossy(&output.stdout))
	});
	assert_json_snapshot!(parsed);
	assert_eq!(
		git_output_trimmed(repo, &["rev-parse", "refs/tags/v1.2.3^{commit}"]),
		commit
	);
	assert_eq!(
		git_output_trimmed(repo, &["ls-remote", "--tags", "origin", "v1.2.3"])
			.split_whitespace()
			.next()
			.unwrap_or_else(|| panic!("expected remote tag sha")),
		commit
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn tag_release_command_is_idempotent_when_tags_already_exist() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	configure_origin_remote(repo);
	let release_record = sample_release_record();
	commit_release_record(repo, &release_record);
	git(repo, &["push", "-u", "origin", "HEAD:main"]);

	let first = tag_release_output(repo, &["--from", "HEAD"]);
	assert!(
		first.status.success(),
		"{}",
		String::from_utf8_lossy(&first.stderr)
	);

	let second = tag_release_output(repo, &["--from", "HEAD"]);
	assert!(
		second.status.success(),
		"{}",
		String::from_utf8_lossy(&second.stderr)
	);
	assert_snapshot!(
		String::from_utf8(second.stdout).unwrap_or_else(|error| panic!("stdout utf8: {error}"))
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn tag_release_command_rejects_descendant_refs_that_are_not_release_commits() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	configure_origin_remote(repo);
	let release_record = sample_release_record();
	commit_release_record(repo, &release_record);
	commit_plain(repo, "fix: follow-up", "release-record/follow-up");

	let output = tag_release_output(repo, &["--from", "HEAD"]);
	assert!(!output.status.success());
	assert_snapshot!(
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"))
	);
}

fn setup_release_repo() -> TempDir {
	let tempdir = setup_fixture("release-record/base-repo");
	let repo = tempdir.path();
	git(repo, &["init"]);
	git(repo, &["config", "user.name", "monochange Tests"]);
	git(repo, &["config", "user.email", "monochange@example.com"]);
	git(repo, &["config", "commit.gpgsign", "false"]);
	git(repo, &["add", "release.txt"]);
	git(repo, &["commit", "-m", "initial"]);
	tempdir
}

fn release_record_output(root: &Path, args: &[&str]) -> std::process::Output {
	monochange_command(None)
		.current_dir(root)
		.arg("release-record")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("release-record output: {error}"))
}

fn tag_release_output(root: &Path, args: &[&str]) -> std::process::Output {
	monochange_command(None)
		.current_dir(root)
		.arg("tag-release")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("tag-release output: {error}"))
}

fn configure_origin_remote(root: &Path) {
	let remote_root = root.join("origin.git");
	git(
		root,
		&["init", "--bare", remote_root.to_string_lossy().as_ref()],
	);
	git(
		root,
		&[
			"remote",
			"add",
			"origin",
			remote_root.to_string_lossy().as_ref(),
		],
	);
}

fn sample_release_record() -> ReleaseRecord {
	ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-07T08:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![ReleaseRecordTarget {
			id: "sdk".to_string(),
			kind: ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.2.3".to_string(),
			members: vec!["monochange".to_string(), "monochange_core".to_string()],
		}],
		released_packages: vec!["monochange".to_string(), "monochange_core".to_string()],
		changed_files: vec![Path::new("Cargo.lock").to_path_buf()],
		updated_changelogs: vec![Path::new("crates/monochange/CHANGELOG.md").to_path_buf()],
		deleted_changesets: vec![Path::new(".changeset/feature.md").to_path_buf()],
		package_publications: Vec::new(),
		provider: Some(ReleaseRecordProvider {
			kind: SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
	}
}

fn commit_release_record(root: &Path, record: &ReleaseRecord) -> (String, String) {
	let block = render_release_record_block(record)
		.unwrap_or_else(|error| panic!("render release record block: {error}"));
	let body = format!("Prepare release.\n\n{block}");
	let sha = commit_with_body(
		root,
		"chore(release): prepare release",
		&body,
		"release-record/commit-body",
	);
	(sha, body)
}

fn commit_plain(root: &Path, subject: &str, fixture_relative: &str) -> String {
	write_release_file_from_fixture(root, fixture_relative);
	git(root, &["add", "release.txt"]);
	git(root, &["commit", "-m", subject]);
	git_output_trimmed(root, &["rev-parse", "HEAD"])
}

fn commit_with_body(root: &Path, subject: &str, body: &str, fixture_relative: &str) -> String {
	write_release_file_from_fixture(root, fixture_relative);
	git(root, &["add", "release.txt"]);
	git(root, &["commit", "--message", subject, "--message", body]);
	git_output_trimmed(root, &["rev-parse", "HEAD"])
}

fn write_release_file_from_fixture(root: &Path, fixture_relative: &str) {
	let source = fixture_path(fixture_relative).join("release.txt");
	fs::copy(&source, root.join("release.txt"))
		.unwrap_or_else(|error| panic!("copy {} into repo: {error}", source.display()));
}
