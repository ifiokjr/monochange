#![allow(clippy::disallowed_methods)]
use std::fs;
use std::process::Stdio;

use tempfile::tempdir;

use super::*;

fn git(root: &Path, args: &[&str]) {
	let output = monochange_core::git::git_command(root)
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

fn git_output(root: &Path, args: &[&str]) -> String {
	let output = monochange_core::git::git_command(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git stdout utf8: {error}"))
		.trim()
		.to_string()
}

fn init_git_repo(root: &Path) {
	git(root, &["init", "-b", "main"]);
	git(root, &["config", "user.name", "monochange tests"]);
	git(root, &["config", "user.email", "monochange@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn git_commit_paths_supports_large_commit_message_bodies() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git(root, &["add", "release.txt"]);
	git(root, &["commit", "-m", "initial"]);

	fs::write(root.join("release.txt"), "release\nupdated\n")
		.unwrap_or_else(|error| panic!("update release file: {error}"));
	git(root, &["add", "release.txt"]);
	let body = "release record entry\n".repeat(16_384);
	git_commit_paths(
		root,
		&CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: Some(body.clone()),
		},
		false,
	)
	.await
	.unwrap_or_else(|error| panic!("git commit paths: {error}"));

	let commit_body = git_output(root, &["log", "-1", "--format=%B"]);
	assert!(commit_body.contains("chore(release): prepare release"));
	assert!(commit_body.contains(body.trim_end()));
}

#[tokio::test(flavor = "multi_thread")]
async fn git_stage_paths_returns_ok_when_all_paths_are_non_stageable() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(root.join(".gitignore"), ".monochange/local/\n")
		.unwrap_or_else(|error| panic!("write .gitignore: {error}"));
	fs::write(root.join("tracked.txt"), "tracked\n")
		.unwrap_or_else(|error| panic!("write tracked file: {error}"));
	init_git_repo(root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
	fs::create_dir_all(root.join(".monochange/local"))
		.unwrap_or_else(|error| panic!("create .monochange: {error}"));
	fs::write(root.join(".monochange/local/release-manifest.json"), "{}\n")
		.unwrap_or_else(|error| panic!("write release manifest: {error}"));

	git_stage_paths(
		root,
		&[
			PathBuf::from(".monochange/local/release-manifest.json"),
			PathBuf::from(".changeset/missing.md"),
		],
	)
	.await
	.unwrap_or_else(|error| panic!("git stage paths: {error}"));

	assert_eq!(git_output(root, &["diff", "--cached", "--name-only"]), "");
}

#[tokio::test(flavor = "multi_thread")]
async fn git_path_is_tracked_reports_command_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path().to_path_buf();
	drop(tempdir);

	let error = git_path_is_tracked(&root, Path::new("release.txt"))
		.await
		.err()
		.unwrap_or_else(|| panic!("expected tracked inspection failure"));
	assert!(
		error
			.to_string()
			.contains("failed to inspect tracked git path release.txt")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn git_path_is_ignored_reports_false_for_unignored_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));

	assert!(
		!git_path_is_ignored(root, Path::new("release.txt"))
			.await
			.unwrap_or_else(|error| panic!("git path ignored: {error}"))
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn git_path_is_ignored_reports_inspection_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(root.join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));

	let error = git_path_is_ignored(root, Path::new("release.txt"))
		.await
		.err()
		.unwrap_or_else(|| panic!("expected ignored inspection failure"));
	assert!(
		error
			.to_string()
			.contains("failed to inspect ignored git path release.txt")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn git_path_is_ignored_reports_command_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path().to_path_buf();
	drop(tempdir);

	let error = git_path_is_ignored(&root, Path::new("release.txt"))
		.await
		.err()
		.unwrap_or_else(|| panic!("expected ignored command failure"));
	assert!(
		error
			.to_string()
			.contains("failed to inspect ignored git path release.txt")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn run_git_capture_includes_stderr_for_failed_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);

	let error = run_git_capture(root, &["show", "missing-commit"], "capture failure")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected failed git capture"));
	assert!(error.to_string().contains("capture failure"));
	assert!(error.to_string().contains("missing-commit"));
}

#[tokio::test(flavor = "multi_thread")]
async fn run_git_process_reports_nonzero_exit_status_details() {
	let mut command = ProcessCommand::new("git");
	command.arg("definitely-not-a-real-git-command");

	let error = run_git_process(command, "process failure")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected failed git process"));
	assert!(error.to_string().contains("process failure"));
	assert!(
		error
			.to_string()
			.contains("definitely-not-a-real-git-command")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn run_git_process_with_stdin_allows_commands_without_piped_stdin() {
	let mut command = ProcessCommand::new("git");
	command.arg("--version");

	run_git_process_with_stdin(command, b"message", "stdin process")
		.await
		.unwrap_or_else(|error| panic!("stdin process should succeed: {error}"));
}

#[tokio::test(flavor = "multi_thread")]
async fn run_git_process_with_stdin_reports_spawn_failures() {
	let command = ProcessCommand::new("definitely-not-a-real-monochange-test-command");

	let error = run_git_process_with_stdin(command, b"message", "stdin process failure")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected failed stdin git process"));
	assert!(error.to_string().contains("stdin process failure"));
}

#[tokio::test(flavor = "multi_thread")]
async fn run_git_process_with_stdin_reports_nonzero_exit_status_details() {
	let mut command = ProcessCommand::new("git");
	command
		.arg("definitely-not-a-real-git-command")
		.stdin(Stdio::piped());

	let error = run_git_process_with_stdin(command, b"message", "stdin process failure")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected failed stdin git process"));
	assert!(error.to_string().contains("stdin process failure"));
	assert!(
		error
			.to_string()
			.contains("definitely-not-a-real-git-command")
	);
}
