use std::fs;
use std::path::Path;
use std::process::Command;

use insta_cmd::get_cargo_bin;
use monochange_core::render_release_record_block;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleaseRecord;
use monochange_core::ReleaseRecordProvider;
use monochange_core::ReleaseRecordTarget;
use monochange_core::SourceProvider;
use monochange_core::VersionFormat;
use serde_json::Value;
use tempfile::tempdir;

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command
}

#[test]
fn release_record_command_reports_record_from_tag_as_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path();
	init_repo(repo);
	let release_record = sample_release_record();
	let release_commit = commit_release_record(repo, &release_record).0;
	git(repo, &["tag", "v1.2.3"]);

	let output = cli()
		.current_dir(repo)
		.arg("release-record")
		.arg("--from")
		.arg("v1.2.3")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release-record json: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let parsed: Value =
		serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("json: {error}"));
	assert_eq!(parsed["inputRef"], "v1.2.3");
	assert_eq!(parsed["resolvedCommit"], release_commit);
	assert_eq!(parsed["recordCommit"], release_commit);
	assert_eq!(parsed["distance"], 0);
	assert_eq!(parsed["record"]["command"], "release-pr");
	assert_eq!(parsed["record"]["releaseTargets"][0]["tagName"], "v1.2.3");
}

#[test]
fn release_record_command_walks_first_parent_ancestry_from_head() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path();
	init_repo(repo);
	let release_record = sample_release_record();
	let release_commit = commit_release_record(repo, &release_record).0;
	git(repo, &["tag", "v1.2.3"]);
	commit_plain(repo, "fix: package release artifacts", "after release\n");
	let head_commit = commit_plain(repo, "fix: format generated files", "after release again\n");

	let output = cli()
		.current_dir(repo)
		.arg("release-record")
		.arg("--from")
		.arg("HEAD")
		.output()
		.unwrap_or_else(|error| panic!("release-record text: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let stdout =
		String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout utf8: {error}"));
	assert!(stdout.contains("input ref: HEAD"));
	assert!(stdout.contains(&format!("resolved commit: {}", short_sha(&head_commit))));
	assert!(stdout.contains(&format!("record commit: {}", short_sha(&release_commit))));
	assert!(stdout.contains("distance: 2"));
	assert!(stdout.contains("- group sdk -> 1.2.3 (tag: v1.2.3)"));
}

#[test]
fn release_record_command_reports_unresolved_refs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path();
	init_repo(repo);

	let output = cli()
		.current_dir(repo)
		.arg("release-record")
		.arg("--from")
		.arg("missing-tag")
		.output()
		.unwrap_or_else(|error| panic!("release-record unresolved ref: {error}"));
	assert!(!output.status.success());
	let stderr =
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"));
	assert!(stderr.contains("could not resolve ref `missing-tag` to a commit"));
}

#[test]
fn release_record_command_reports_missing_record_in_history() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path();
	init_repo(repo);

	let output = cli()
		.current_dir(repo)
		.arg("release-record")
		.arg("--from")
		.arg("HEAD")
		.output()
		.unwrap_or_else(|error| panic!("release-record missing history: {error}"));
	assert!(!output.status.success());
	let stderr =
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"));
	assert!(
		stderr.contains("no monochange release record found in first-parent ancestry from `HEAD`")
	);
}

#[test]
fn release_record_command_fails_loudly_on_malformed_record_in_ancestry() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path();
	init_repo(repo);
	let release_record = sample_release_record();
	commit_release_record(repo, &release_record);
	commit_with_body(
		repo,
		"chore(release): malformed release record",
		"## monochange Release Record\n\n<!-- monochange:release-record:start -->\n```json\n{}\n```");
	commit_plain(repo, "fix: follow-up", "follow-up\n");

	let output = cli()
		.current_dir(repo)
		.arg("release-record")
		.arg("--from")
		.arg("HEAD")
		.output()
		.unwrap_or_else(|error| panic!("release-record malformed ancestry: {error}"));
	assert!(!output.status.success());
	let stderr =
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"));
	assert!(stderr.contains("found a malformed monochange release record in commit"));
}

#[test]
fn release_record_command_reports_unsupported_schema_version() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path();
	init_repo(repo);
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
<!-- monochange:release-record:end -->"#
		.to_string();
	commit_with_body(repo, "chore(release): prepare release", &body);

	let output = cli()
		.current_dir(repo)
		.arg("release-record")
		.arg("--from")
		.arg("HEAD")
		.output()
		.unwrap_or_else(|error| panic!("release-record unsupported schema: {error}"));
	assert!(!output.status.success());
	let stderr =
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"));
	assert!(stderr.contains("uses unsupported schemaVersion 2"));
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
		provider: Some(ReleaseRecordProvider {
			kind: SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
	}
}

fn init_repo(root: &Path) {
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange Tests"]);
	git(root, &["config", "user.email", "monochange@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
	fs::write(root.join("release.txt"), "before\n")
		.unwrap_or_else(|error| panic!("write initial file: {error}"));
	git(root, &["add", "release.txt"]);
	git(root, &["commit", "-m", "initial"]);
}

fn commit_release_record(root: &Path, record: &ReleaseRecord) -> (String, String) {
	let block = render_release_record_block(record)
		.unwrap_or_else(|error| panic!("render release record block: {error}"));
	let body = format!("Prepare release.\n\n{block}");
	let sha = commit_with_body(root, "chore(release): prepare release", &body);
	(sha, body)
}

fn commit_plain(root: &Path, subject: &str, content: &str) -> String {
	fs::write(root.join("release.txt"), content)
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git(root, &["add", "release.txt"]);
	git(root, &["commit", "-m", subject]);
	git_output(root, &["rev-parse", "HEAD"])
}

fn commit_with_body(root: &Path, subject: &str, body: &str) -> String {
	fs::write(root.join("release.txt"), format!("{subject}\n"))
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git(root, &["add", "release.txt"]);
	git(root, &["commit", "--message", subject, "--message", body]);
	git_output(root, &["rev-parse", "HEAD"])
}

fn git(root: &Path, args: &[&str]) {
	let status = Command::new("git")
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn git_output(root: &Path, args: &[&str]) -> String {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(output.status.success(), "git {args:?} failed");
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git output utf8: {error}"))
		.trim()
		.to_string()
}

fn short_sha(sha: &str) -> String {
	sha.chars().take(7).collect()
}
