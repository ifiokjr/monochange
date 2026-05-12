#![allow(
	clippy::indexing_slicing,
	clippy::single_char_pattern,
	clippy::used_underscore_binding
)]

use std::fs;
use std::path::Path;

use insta::assert_snapshot;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleaseRecord;
use monochange_core::ReleaseRecordProvider;
use monochange_core::ReleaseRecordTarget;
use monochange_core::SourceProvider;
use monochange_core::VersionFormat;
use monochange_test_helpers::git;
use monochange_test_helpers::git_output_trimmed;
use serde_json::Value;
use tempfile::TempDir;

mod test_support;
use test_support::assert_readable_json_snapshot;
use test_support::current_test_name;
use test_support::fixture_path;
use test_support::monochange_command;
use test_support::setup_fixture;
use test_support::snapshot_settings;

#[etest::etest]
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
	assert_readable_json_snapshot!(parsed);
}

#[etest::etest]
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

#[etest::etest]
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

#[etest::etest]
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

#[etest::etest]
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
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(String::from_utf8_lossy(&output.stdout).contains("sdk"));
}

#[etest::etest]
fn release_record_command_skips_malformed_file_based_record_in_ancestry() {
	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	let release_record = sample_release_record();
	let (record_commit, _) = commit_release_record(repo, &release_record);
	let malformed_dir = repo.join(".monochange/releases/malformed-record");
	fs::create_dir_all(&malformed_dir)
		.unwrap_or_else(|error| panic!("create malformed record dir: {error}"));
	fs::write(malformed_dir.join("release.json"), "{}")
		.unwrap_or_else(|error| panic!("write malformed record: {error}"));
	git(
		repo,
		&["add", ".monochange/releases/malformed-record/release.json"],
	);
	git(
		repo,
		&["commit", "-m", "chore: add malformed release record"],
	);
	commit_plain(repo, "fix: follow-up", "release-record/follow-up");

	let output = release_record_output(repo, &["--from", "HEAD", "--format", "json"]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!("json: {error}\n{}", String::from_utf8_lossy(&output.stdout))
	});
	assert_eq!(
		parsed["recordCommit"].as_str(),
		Some(record_commit.as_str()),
		"expected malformed file-based release record to be skipped"
	);
}

#[etest::etest]
fn release_record_command_reports_unsupported_schema_version() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	let schema_version = unsupported_release_record_schema_version();
	let json_text = format!(
		r#"{{
  "v": "{schema_version}",
  "kind": "monochange.releaseRecord",
  "createdAt": "2026-04-07T08:00:00Z",
  "command": "release-pr",
  "releaseTargets": [],
  "releasedPackages": [],
  "changedFiles": []
}}"#
	);
	commit_file_based_record_raw(repo, &json_text, "release-record/commit-body");

	let output = release_record_output(repo, &["--from", "HEAD"]);
	assert!(!output.status.success());
	assert_snapshot!(
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"))
	);
}

// Keep this test ahead of the current durable schema version. Release PRs bump
// `monochange_schema`, so a hard-coded "future" version eventually becomes
// current and turns this error-path test into a release-CI failure.
fn unsupported_release_record_schema_version() -> String {
	let (major, minor) = monochange_core::RELEASE_RECORD_SCHEMA_VERSION
		.split_once('.')
		.unwrap_or_else(|| panic!("schema version should be major.minor"));
	let major = major
		.parse::<u64>()
		.unwrap_or_else(|error| panic!("parse schema major: {error}"));
	let minor = minor
		.parse::<u64>()
		.unwrap_or_else(|error| panic!("parse schema minor: {error}"));
	format!("{major}.{}", minor + 1)
}

#[etest::etest]
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
	assert_readable_json_snapshot!(parsed);
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

#[etest::etest]
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

#[etest::etest]
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

#[etest::etest]
fn tag_release_command_json_snapshots_entire_report() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	configure_origin_remote(repo);
	let mut release_record = sample_release_record();
	release_record.release_targets.push(ReleaseRecordTarget {
		id: "cli".to_string(),
		kind: ReleaseOwnerKind::Package,
		version: "2.0.0".to_string(),
		version_format: VersionFormat::Namespaced,
		tag: true,
		release: true,
		tag_name: "cli/v2.0.0".to_string(),
		members: Vec::new(),
	});
	commit_release_record(repo, &release_record);
	git(repo, &["push", "-u", "origin", "HEAD:main"]);

	let output = tag_release_output(
		repo,
		&[
			"--from",
			"HEAD",
			"--dry-run",
			"--push=false",
			"--format",
			"json",
		],
	);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!("json: {error}\n{}", String::from_utf8_lossy(&output.stdout))
	});
	assert_readable_json_snapshot!(parsed);
}

#[etest::etest]
fn tag_release_command_dry_run_reports_planned_tags_without_creating_them() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	configure_origin_remote(repo);
	let release_record = sample_release_record();
	commit_release_record(repo, &release_record);
	git(repo, &["push", "-u", "origin", "HEAD:main"]);

	let output = tag_release_output(repo, &["--from", "HEAD", "--dry-run", "--push=false"]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_snapshot!(
		String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout utf8: {error}"))
	);
	assert!(
		git_command(repo, &["rev-parse", "refs/tags/v1.2.3^{commit}"]).is_none(),
		"expected dry run to avoid creating a local tag"
	);
	assert!(
		git_command(repo, &["ls-remote", "--tags", "origin", "v1.2.3"]).is_none(),
		"expected dry run to avoid pushing a remote tag"
	);
}

#[etest::etest]
fn tag_release_command_reports_when_no_tags_are_declared() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	configure_origin_remote(repo);
	let mut release_record = sample_release_record();
	release_record.release_targets[0].tag = false;
	let (commit, _) = commit_release_record(repo, &release_record);
	git(repo, &["push", "-u", "origin", "HEAD:main"]);

	let output = tag_release_output(repo, &["--from", "HEAD", "--push=false"]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_snapshot!(
		String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout utf8: {error}"))
	);
	assert_eq!(git_output_trimmed(repo, &["rev-parse", "HEAD"]), commit);
	assert!(
		git_command(repo, &["ls-remote", "--tags", "origin", "v1.2.3"]).is_none(),
		"expected no declared tags to skip remote pushes"
	);
}

#[etest::etest]
fn tag_release_command_rejects_existing_tags_that_point_elsewhere() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	let initial_commit = git_output_trimmed(repo, &["rev-parse", "HEAD"]);
	let release_record = sample_release_record();
	commit_release_record(repo, &release_record);
	git(repo, &["tag", "v1.2.3", &initial_commit]);

	let output = tag_release_output(repo, &["--from", "HEAD"]);
	assert!(!output.status.success());
	assert_snapshot!(
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"))
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_dedupes_overlapping_records_on_second_commit() {
	let tempdir = setup_release_repo();
	let repo = tempdir.path();

	let mut first = sample_release_record();
	first.release_targets[0].version = "0.5.0".to_string();
	first.release_targets[0].id = "sdk".to_string();
	first.changesets = Vec::new();
	commit_release_record(repo, &first);

	// Make a change so the next commit has something to stage
	fs::write(
		repo.join("release.txt"),
		"second commit change
",
	)
	.unwrap_or_else(|error| panic!("write release file: {error}"));

	let mut second = sample_release_record();
	second.release_targets[0].version = "0.5.0".to_string();
	second.release_targets[0].id = "sdk".to_string();
	second.changesets = Vec::new();
	second.changelogs = vec![monochange_core::ReleaseManifestChangelog {
		owner_id: "sdk".to_string(),
		owner_kind: ReleaseOwnerKind::Group,
		path: std::path::PathBuf::from("CHANGELOG.md"),
		format: monochange_core::ChangelogFormat::Monochange,
		notes: monochange_core::ReleaseNotesDocument {
			title: "0.5.0".to_string(),
			summary: vec!["change".to_string()],
			sections: Vec::new(),
		},
		rendered: "## 0.5.0".to_string(),
	}];
	commit_release_record(repo, &second);

	let releases_dir = repo.join(".monochange/releases");
	let count = fs::read_dir(&releases_dir)
		.unwrap_or_else(|error| panic!("read releases dir: {error}"))
		.filter_map(Result::ok)
		.filter(|e| e.path().join("release.json").exists())
		.count();
	assert_eq!(count, 1, "expected exactly one release record after dedupe");
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_command_detects_file_based_record_in_merge_commit() {
	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	let release_record = sample_release_record();
	let (record_commit, _) = commit_release_record(repo, &release_record);

	// Create a feature branch with a plain commit after the release record
	git(repo, &["checkout", "-b", "feature"]);
	commit_plain(repo, "fix: some feature work", "release-record/follow-up");

	// Merge the feature branch into main (creating a merge commit)
	git(repo, &["checkout", "main"]);
	git(
		repo,
		&["merge", "feature", "--no-ff", "-m", "Merge feature branch"],
	);

	// Verify the merge commit is a merge commit (has 2 parents)
	let parent_count = git_output_trimmed(repo, &["cat-file", "-p", "HEAD"]);
	let parent_lines = parent_count
		.lines()
		.filter(|line| line.starts_with("parent "))
		.count();
	assert_eq!(
		parent_lines, 2,
		"expected HEAD to be a merge commit with 2 parents"
	);

	// mc release-record --from HEAD should find the record via the merge commit
	let output = release_record_output(repo, &["--from", "HEAD", "--format", "json"]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!("json: {error}\n{}", String::from_utf8_lossy(&output.stdout))
	});
	assert_eq!(
		parsed["recordCommit"].as_str(),
		Some(record_commit.as_str()),
		"expected merge commit to resolve to the release record commit"
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn git_diff_tree_m_flag_detects_release_record_in_merge_commit() {
	let tempdir = setup_release_repo();
	let repo = tempdir.path();

	// Create a feature branch and commit the release record there
	git(repo, &["checkout", "-b", "feature"]);
	let release_record = sample_release_record();
	commit_release_record(repo, &release_record);

	// Merge the feature branch into main, bringing in the release record
	git(repo, &["checkout", "main"]);
	git(
		repo,
		&["merge", "feature", "--no-ff", "-m", "Merge feature branch"],
	);

	// Without -m, diff-tree produces combined diff which omits clean merges
	let without_m = std::process::Command::new("git")
		.current_dir(repo)
		.args(["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"])
		.output()
		.unwrap_or_else(|error| panic!("git diff-tree without -m: {error}"));
	let without_m_output = String::from_utf8_lossy(&without_m.stdout);
	let _has_record_without_m = without_m_output
		.lines()
		.any(|line| line.starts_with(".monochange/releases/") && line.ends_with("/release.json"));

	// With -m, diff-tree compares against each parent and finds the file
	let with_m = std::process::Command::new("git")
		.current_dir(repo)
		.args([
			"diff-tree",
			"-m",
			"--no-commit-id",
			"--name-only",
			"-r",
			"HEAD",
		])
		.output()
		.unwrap_or_else(|error| panic!("git diff-tree with -m: {error}"));
	let with_m_output = String::from_utf8_lossy(&with_m.stdout);
	let has_record_with_m = with_m_output
		.lines()
		.any(|line| line.starts_with(".monochange/releases/") && line.ends_with("/release.json"));

	// The release record was on the feature branch, not on main. With -m,
	// diff-tree compares HEAD against each parent individually, so it will
	// show the file when compared to the first parent (main) which lacked it.
	assert!(
		has_record_with_m,
		"expected git diff-tree -m to detect .monochange/releases/*/release.json in merge commit; got:\n{with_m_output}"
	);
}
#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_command_walks_back_many_non_release_commits() {
	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	let release_record = sample_release_record();
	let (record_commit, _) = commit_release_record(repo, &release_record);

	// Create 5 plain follow-up commits after the release
	for index in 0..5 {
		let content = format!("follow-up content {index}\n");
		fs::write(repo.join(format!("follow-up-{index}.txt")), content)
			.unwrap_or_else(|error| panic!("write follow-up file: {error}"));
		git(repo, &["add", "."]);
		git(repo, &["commit", "-m", &format!("fix: follow-up {index}")]);
	}

	let output = release_record_output(repo, &["--from", "HEAD", "--format", "json"]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!("json: {error}\n{}", String::from_utf8_lossy(&output.stdout))
	});
	assert_eq!(
		parsed["recordCommit"].as_str(),
		Some(record_commit.as_str()),
		"expected to find the release record commit through 5 non-release commits"
	);
	assert_eq!(
		parsed["distance"].as_u64(),
		Some(5),
		"expected distance of 5 commits back to the release record"
	);
	let head_commit = git_output_trimmed(repo, &["rev-parse", "HEAD"]);
	assert_eq!(
		parsed["resolvedCommit"].as_str(),
		Some(head_commit.as_str()),
		"expected resolvedCommit to be HEAD itself"
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_command_finds_most_recent_among_multiple_releases() {
	let tempdir = setup_release_repo();
	let repo = tempdir.path();

	// First release at v0.5.0
	let mut first = sample_release_record();
	first.release_targets[0].version = "0.5.0".to_string();
	first.release_targets[0].id = "sdk".to_string();
	first.changesets = Vec::new();
	let (first_commit, _) = commit_release_record(repo, &first);

	// Two plain commits between releases
	let _content_1 = "between-release 1\n";
	fs::write(repo.join("between-release-1.txt"), _content_1)
		.unwrap_or_else(|error| panic!("write between-release file: {error}"));
	git(repo, &["add", "."]);
	git(repo, &["commit", "-m", "fix: post-release fix 1"]);
	let _content_2 = "between-release 2\n";
	fs::write(repo.join("between-release-2.txt"), _content_2)
		.unwrap_or_else(|error| panic!("write between-release file: {error}"));
	git(repo, &["add", "."]);
	git(repo, &["commit", "-m", "chore: update dependencies"]);

	// Second release at v0.6.0
	let mut second = sample_release_record();
	second.release_targets[0].version = "0.6.0".to_string();
	second.release_targets[0].id = "sdk".to_string();
	second.changesets = Vec::new();
	let (second_commit, _) = commit_release_record(repo, &second);

	// Two more plain commits after the second release
	let _content_3 = "between-release 3\n";
	fs::write(repo.join("between-release-3.txt"), _content_3)
		.unwrap_or_else(|error| panic!("write between-release file: {error}"));
	git(repo, &["add", "."]);
	git(repo, &["commit", "-m", "fix: another fix"]);
	let _content_4 = "between-release 4\n";
	fs::write(repo.join("between-release-4.txt"), _content_4)
		.unwrap_or_else(|error| panic!("write between-release file: {error}"));
	git(repo, &["add", "."]);
	git(repo, &["commit", "-m", "docs: update readme"]);

	let output = release_record_output(repo, &["--from", "HEAD", "--format", "json"]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!("json: {error}\n{}", String::from_utf8_lossy(&output.stdout))
	});

	// Should resolve to the SECOND (most recent) release, not the first
	assert_eq!(
		parsed["recordCommit"].as_str(),
		Some(second_commit.as_str()),
		"expected most recent release (v0.6.0), not the older one (v0.5.0)"
	);
	assert_eq!(
		parsed["distance"].as_u64(),
		Some(2),
		"expected distance of 2 from HEAD to the second release"
	);

	// Verify the first release is still accessible from its own commit
	let first_output = release_record_output(repo, &["--from", &first_commit, "--format", "json"]);
	assert!(first_output.status.success());
	let first_parsed: Value = serde_json::from_slice(&first_output.stdout)
		.unwrap_or_else(|error| panic!("first json: {error}"));
	assert_eq!(
		first_parsed["recordCommit"].as_str(),
		Some(first_commit.as_str()),
		"expected record at first_commit to be the first release"
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn release_record_command_sha_flag_outputs_only_commit_hash() {
	let tempdir = setup_release_repo();
	let repo = tempdir.path();
	let release_record = sample_release_record();
	let (record_commit, _) = commit_release_record(repo, &release_record);

	// Two plain commits after the release
	let content_1 = "sha-test 1\n";
	fs::write(repo.join("sha-test-1.txt"), content_1)
		.unwrap_or_else(|error| panic!("write sha-test file: {error}"));
	git(repo, &["add", "."]);
	git(repo, &["commit", "-m", "fix: post-release fix"]);
	let content_2 = "sha-test 2\n";
	fs::write(repo.join("sha-test-2.txt"), content_2)
		.unwrap_or_else(|error| panic!("write sha-test file: {error}"));
	git(repo, &["add", "."]);
	git(repo, &["commit", "-m", "chore: update readme"]);

	let output = release_record_output(repo, &["--from", "HEAD", "--sha"]);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let stdout =
		String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout utf8: {error}"));
	let trimmed = stdout.trim();
	assert_eq!(
		trimmed, record_commit,
		"expected --sha to output the record commit hash"
	);
	assert!(
		!trimmed.contains("{"),
		"expected --sha to output plain text, not JSON"
	);
}

fn setup_release_repo() -> TempDir {
	let tempdir = setup_fixture("release-record/base-repo");
	let repo = tempdir.path();
	git(repo, &["init"]);
	git(repo, &["branch", "-M", "main"]);
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

fn git_command(root: &Path, args: &[&str]) -> Option<String> {
	let output = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	if !output.status.success() {
		return None;
	}

	let stdout = String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git stdout utf8: {error}"))
		.trim()
		.to_string();
	if stdout.is_empty() {
		None
	} else {
		Some(stdout)
	}
}

fn sample_release_record() -> ReleaseRecord {
	ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-07T08:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.2.3".to_string()),
		versions: std::collections::BTreeMap::from([("sdk".to_string(), "1.2.3".to_string())]),
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
		changesets: Vec::new(),
		changelogs: Vec::new(),
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
	let json = serde_json::to_string_pretty(record)
		.unwrap_or_else(|error| panic!("serialize release record: {error}"));
	let hash = {
		use std::collections::hash_map::DefaultHasher;
		use std::hash::Hasher;
		let mut hasher = DefaultHasher::new();
		for target in &record.release_targets {
			hasher.write(target.id.as_bytes());
			hasher.write(target.version.as_bytes());
		}
		format!("{:016x}", hasher.finish())
	};
	let dir = root.join(".monochange/releases").join(&hash);
	fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create release record dir: {error}"));
	let record_path = dir.join("release.json");
	fs::write(&record_path, &json).unwrap_or_else(|error| panic!("write release record: {error}"));
	write_release_file_from_fixture(root, "release-record/commit-body");
	git(root, &["add", "."]);
	git(
		root,
		&[
			"commit",
			"-m",
			"chore(release): prepare release",
			"-m",
			"Prepare release.",
		],
	);
	let sha = git_output_trimmed(root, &["rev-parse", "HEAD"]);
	let body = "Prepare release.".to_string();
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

fn commit_file_based_record_raw(root: &Path, json_text: &str, fixture_relative: &str) {
	let dir = root.join(".monochange/releases/0000000000000000");
	fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create release record dir: {error}"));
	let record_path = dir.join("release.json");
	fs::write(&record_path, json_text)
		.unwrap_or_else(|error| panic!("write release record: {error}"));
	write_release_file_from_fixture(root, fixture_relative);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "chore(release): prepare release"]);
}

fn write_release_file_from_fixture(root: &Path, fixture_relative: &str) {
	let source = fixture_path(fixture_relative).join("release.txt");
	fs::copy(&source, root.join("release.txt"))
		.unwrap_or_else(|error| panic!("copy {} into repo: {error}", source.display()));
}
