#![allow(clippy::disallowed_methods)]
use std::process::Command;

use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseSettings;
use monochange_core::SourceProvider;
use tempfile::tempdir;

use super::*;

#[test]
fn write_comma_separated_joins_multiple_values() {
	let mut output = String::new();
	write_comma_separated(&mut output, ["main", "release/*"]);
	assert_eq!(output, "main, release/*");
}

#[test]
fn release_branch_pattern_matches_local_and_remote_branches() {
	let patterns = vec![
		Pattern::new("main").unwrap_or_else(|error| panic!("pattern: {error}")),
		Pattern::new("release/*").unwrap_or_else(|error| panic!("pattern: {error}")),
	];

	assert!(branch_matches(&patterns, "refs/heads/main", "main"));
	assert!(branch_matches(
		&patterns,
		"refs/remotes/origin/main",
		"origin/main"
	));
	assert!(branch_matches(
		&patterns,
		"refs/remotes/origin/release/production",
		"origin/release/production"
	));
	assert!(!branch_matches(
		&patterns,
		"refs/remotes/origin/feature/demo",
		"origin/feature/demo"
	));
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_release_ref_rejects_empty_branch_policy() {
	let repo = init_git_repo();
	let policy = ProviderReleaseSettings {
		branches: Vec::new(),
		..ProviderReleaseSettings::default()
	};
	let error = verify_release_ref(repo.path(), &policy, "HEAD")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected empty branch policy error"));

	assert!(
		error
			.to_string()
			.contains("branches must contain at least one release branch pattern")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_release_ref_rejects_invalid_branch_pattern() {
	let repo = init_git_repo();
	let policy = ProviderReleaseSettings {
		branches: vec!["[".to_string()],
		..ProviderReleaseSettings::default()
	};
	let error = verify_release_ref(repo.path(), &policy, "HEAD")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected invalid branch pattern error"));

	assert!(
		error
			.to_string()
			.contains("invalid [source.releases].branches pattern `[`")
	);
}

#[test]
fn display_branch_name_ignores_remote_head_symbolic_refs() {
	assert_eq!(display_branch_name("refs/remotes/origin/HEAD"), None);
	assert_eq!(display_branch_name("refs/tags/v1.0.0"), None);
	assert_eq!(
		display_branch_name("refs/remotes/origin/main"),
		Some("origin/main".to_string())
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_release_ref_reports_git_branch_listing_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let policy = ProviderReleaseSettings::default();
	let error = verify_release_ref(tempdir.path(), &policy, "HEAD")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected git branch listing error"));

	assert!(!error.to_string().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_release_ref_ignores_remote_head_symbolic_refs() {
	let repo = init_git_repo();
	run_git(
		repo.path(),
		&[
			"symbolic-ref",
			"refs/remotes/origin/HEAD",
			"refs/remotes/origin/main",
		],
	);
	let policy = ProviderReleaseSettings::default();

	verify_release_ref(repo.path(), &policy, "HEAD")
		.await
		.unwrap_or_else(|error| panic!("verify release ref: {error}"));
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_release_ref_reports_when_no_release_branch_refs_match_policy() {
	let repo = init_git_repo();
	let policy = ProviderReleaseSettings {
		branches: vec!["stable".to_string()],
		..ProviderReleaseSettings::default()
	};
	let error = verify_release_ref(repo.path(), &policy, "HEAD")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected release branch policy error"));

	assert!(
		error
			.to_string()
			.contains("matching branch refs: none found")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn enforcement_wrappers_verify_when_enabled() {
	let repo = init_git_repo();
	let mut source = source_configuration();
	source.releases.branches = vec!["release/*".to_string()];
	source.releases.enforce_for_commit = true;

	assert!(
		verify_release_ref_for_tags(repo.path(), Some(&source), "HEAD")
			.await
			.unwrap_or_else(|error| panic!("tag verification: {error}"))
			.is_some()
	);
	assert!(
		verify_release_ref_for_publish(repo.path(), Some(&source), "HEAD")
			.await
			.unwrap_or_else(|error| panic!("publish verification: {error}"))
			.is_some()
	);
	assert!(
		verify_release_ref_for_commit(repo.path(), Some(&source), "HEAD")
			.await
			.unwrap_or_else(|error| panic!("commit verification: {error}"))
			.is_some()
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn enforcement_wrappers_skip_absent_source_or_disabled_policy() {
	let repo = init_git_repo();
	let mut source = source_configuration();
	source.releases.enforce_for_tags = false;
	source.releases.enforce_for_publish = false;

	assert!(
		verify_release_ref_for_tags(repo.path(), None, "HEAD")
			.await
			.unwrap_or_else(|error| panic!("tag verification: {error}"))
			.is_none()
	);
	assert!(
		verify_release_ref_for_tags(repo.path(), Some(&source), "HEAD")
			.await
			.unwrap_or_else(|error| panic!("tag verification: {error}"))
			.is_none()
	);
	assert!(
		verify_release_ref_for_publish(repo.path(), None, "HEAD")
			.await
			.unwrap_or_else(|error| panic!("publish verification: {error}"))
			.is_none()
	);
	assert!(
		verify_release_ref_for_publish(repo.path(), Some(&source), "HEAD")
			.await
			.unwrap_or_else(|error| panic!("publish verification: {error}"))
			.is_none()
	);
	assert!(
		verify_release_ref_for_commit(repo.path(), None, "HEAD")
			.await
			.unwrap_or_else(|error| panic!("commit verification: {error}"))
			.is_none()
	);
	assert!(
		verify_release_ref_for_commit(repo.path(), Some(&source), "HEAD")
			.await
			.unwrap_or_else(|error| panic!("commit verification: {error}"))
			.is_none()
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn commit_wrapper_enforces_policy_when_enabled() {
	let repo = init_git_repo();
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["checkout", "-b", "feature/demo"]);
	write_and_commit(repo.path(), "feature.txt", "feature", "feature commit");
	let mut source = source_configuration();
	source.releases.branches = vec!["release/*".to_string()];
	source.releases.enforce_for_commit = true;
	let error = verify_release_ref_for_commit(repo.path(), Some(&source), "HEAD")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected commit verification error"));

	assert!(
		error
			.to_string()
			.contains("is not reachable from any configured release branch pattern [release/*]")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_release_ref_accepts_commits_reachable_from_globbed_release_branch() {
	let repo = init_git_repo();
	write_and_commit(repo.path(), "release.txt", "release", "release commit");
	run_git(repo.path(), &["tag", "v1.0.0"]);

	let policy = ProviderReleaseSettings {
		branches: vec!["release/*".to_string()],
		..ProviderReleaseSettings::default()
	};

	let report = verify_release_ref(repo.path(), &policy, "v1.0.0")
		.await
		.unwrap_or_else(|error| panic!("verify release ref: {error}"));

	assert_eq!(report.ref_name, "v1.0.0");
	assert_eq!(report.allowed_branches, vec!["release/*"]);
	assert_eq!(report.matched_branch, "release/production");
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_release_ref_rejects_commits_not_reachable_from_release_branch() {
	let repo = init_git_repo();
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["checkout", "-b", "feature/demo"]);
	write_and_commit(repo.path(), "feature.txt", "feature", "feature commit");

	let policy = ProviderReleaseSettings {
		branches: vec!["release/*".to_string()],
		..ProviderReleaseSettings::default()
	};
	let error = verify_release_ref(repo.path(), &policy, "HEAD")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected release branch policy error"));

	assert!(
		error
			.to_string()
			.contains("is not reachable from any configured release branch pattern [release/*]")
	);
}

fn source_configuration() -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::GitHub,
		owner: "monochange".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	}
}

fn init_git_repo() -> tempfile::TempDir {
	let repo = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	run_git(repo.path(), &["init", "-b", "main"]);
	run_git(
		repo.path(),
		&["config", "user.email", "monochange@example.com"],
	);
	run_git(repo.path(), &["config", "user.name", "monochange tests"]);
	run_git(repo.path(), &["config", "commit.gpgsign", "false"]);
	write_and_commit(repo.path(), "README.md", "root", "initial commit");
	run_git(repo.path(), &["checkout", "-b", "release/production"]);
	repo
}

fn write_and_commit(root: &Path, path: &str, contents: &str, message: &str) {
	std::fs::write(root.join(path), contents)
		.unwrap_or_else(|error| panic!("write {path}: {error}"));
	run_git(root, &["add", path]);
	run_git(root, &["commit", "-m", message]);
}

#[test]
#[should_panic(expected = "git")]
fn run_git_reports_stderr_for_failures() {
	let repo = init_git_repo();
	run_git(repo.path(), &["not-a-command"]);
}

fn run_git(root: &Path, args: &[&str]) {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
}
