use std::fs;
use std::io;

use super::*;

#[test]
fn git_command_reuses_stable_path_for_child_processes() {
	let command = git_command(Path::new("."));
	let path = command
		.get_envs()
		.find_map(|(key, value)| (key == "PATH").then_some(value))
		.flatten()
		.unwrap_or_else(|| panic!("expected PATH override"));

	assert!(path_contains_git(path));
}

#[test]
fn resolve_process_path_prefers_current_path_when_it_contains_git() {
	let current = temp_path_with_git();
	let fallback = tempdir("create fallback dir");

	let path = resolve_process_path(
		Some(current.path().as_os_str().to_owned()),
		Some(fallback.path().as_os_str().to_owned()),
	)
	.unwrap_or_else(|| panic!("expected current PATH"));

	assert_eq!(path, current.path().as_os_str());
}

#[test]
fn resolve_process_path_uses_fallback_when_current_path_lacks_git() {
	let current = tempdir("create current dir");
	let fallback = temp_path_with_git();

	let path = resolve_process_path(
		Some(current.path().as_os_str().to_owned()),
		Some(fallback.path().as_os_str().to_owned()),
	)
	.unwrap_or_else(|| panic!("expected fallback PATH"));

	assert_eq!(path, fallback.path().as_os_str());
}

#[test]
fn resolve_process_path_keeps_current_path_when_no_path_contains_git() {
	let current = tempdir("create current dir");
	let fallback = tempdir("create fallback dir");

	let path = resolve_process_path(
		Some(current.path().as_os_str().to_owned()),
		Some(fallback.path().as_os_str().to_owned()),
	)
	.unwrap_or_else(|| panic!("expected current PATH"));

	assert_eq!(path, current.path().as_os_str());
}

fn temp_path_with_git() -> tempfile::TempDir {
	let directory = tempdir("create PATH dir");
	fs::write(directory.path().join(git_executable_name()), "")
		.unwrap_or_else(|error| panic!("write git shim: {error}"));
	directory
}

fn tempdir(context: &str) -> tempfile::TempDir {
	tempfile::tempdir().unwrap_or_else(|error| panic!("{context}: {error}"))
}

fn git(root: &Path, args: &[&str]) {
	let status = git_command(root)
		.args(["-c", "commit.gpgsign=false"])
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed with {status}");
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
	let output = git_command(root)
		.args(["-c", "commit.gpgsign=false"])
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed with {output:?}"
	);
	String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn git_commit_message_text_renders_subject_and_body() {
	let message = CommitMessage {
		subject: "chore(release): prepare release".to_string(),
		body: Some("Prepare release.\n\n<!-- release record -->".to_string()),
	};

	assert_eq!(
		git_commit_message_text(&message),
		"chore(release): prepare release\n\nPrepare release.\n\n<!-- release record -->"
	);
}

#[test]
fn git_commit_message_text_renders_subject_without_body() {
	let message = CommitMessage {
		subject: "chore(release): prepare release".to_string(),
		body: None,
	};

	assert_eq!(
		git_commit_message_text(&message),
		"chore(release): prepare release"
	);
}

#[test]
fn git_commit_paths_stdin_command_reads_message_from_stdin() {
	let command = git_commit_paths_stdin_command(Path::new("."), true);
	let args = command
		.get_args()
		.map(|arg| arg.to_string_lossy().into_owned())
		.collect::<Vec<_>>();

	assert_eq!(args, vec!["commit", "--no-verify", "--file", "-"]);
}

#[test]
fn git_commit_file_command_reads_message_from_file() {
	let command = git_commit_file_command(Path::new("."), Path::new("message.txt"), true);
	let args = command
		.get_args()
		.map(|arg| arg.to_string_lossy().into_owned())
		.collect::<Vec<_>>();

	assert_eq!(args, vec!["commit", "--no-verify", "--file", "message.txt"]);
}

#[test]
fn git_commit_temp_file_error_helpers_include_phase_diagnostics() {
	let message = CommitMessage {
		subject: "chore(release): prepare release".to_string(),
		body: None,
	};
	let message_text = git_commit_message_text(&message);
	let message_file = Path::new("message.txt");

	let create_error = git_commit_create_temp_file_error(
		Path::new("repo"),
		&message,
		&message_text,
		"commit release pull request changes",
		true,
		"disk unavailable",
	)
	.to_string();
	let write_error = git_commit_write_temp_file_error(
		Path::new("repo"),
		&message,
		&message_text,
		"commit release pull request changes",
		true,
		message_file,
		"write failed",
	)
	.to_string();
	let flush_error = git_commit_flush_temp_file_error(
		Path::new("repo"),
		&message,
		&message_text,
		"commit release pull request changes",
		true,
		message_file,
		"flush failed",
	)
	.to_string();

	assert!(create_error.contains("phase: creating temporary commit message file"));
	assert!(create_error.contains("temporary message file: <not yet created>"));
	assert!(create_error.contains("body preview: <none>"));
	assert!(create_error.contains("no_verify: true"));
	assert!(write_error.contains("phase: writing temporary commit message file"));
	assert!(write_error.contains("temporary message file: message.txt"));
	assert!(flush_error.contains("phase: flushing temporary commit message file"));
	assert!(flush_error.contains("temporary message file: message.txt"));
}

#[test]
fn preview_text_truncates_long_multiline_messages() {
	let preview = preview_text(&format!("{}\nsecond line", "a".repeat(260)));

	assert!(preview.ends_with('…'));
	assert!(!preview.contains('\n'));
}

#[test]
fn run_git_commit_message_reports_create_temp_file_diagnostics() {
	let tempdir = tempdir("create parent");
	let missing_nested_root = tempdir.path().join("missing-parent").join("repo");
	let error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(run_git_commit_message(
			&missing_nested_root,
			&CommitMessage {
				subject: "chore(release): prepare release".to_string(),
				body: Some("release body".to_string()),
			},
			"commit release pull request changes",
			false,
		))
		.err()
		.unwrap_or_else(|| panic!("expected commit failure"))
		.to_string();

	assert!(error.contains("could not create temporary git commit message file"));
	assert!(error.contains("phase: creating temporary commit message file"));
	assert!(error.contains("temporary message file: <not yet created>"));
}

#[test]
fn run_git_commit_message_reports_write_and_flush_diagnostics() {
	let tempdir = tempdir("create repo");
	let root = tempdir.path();
	let message = CommitMessage {
		subject: "chore(release): prepare release".to_string(),
		body: Some("release body".to_string()),
	};

	let write_error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(run_git_commit_message_with_io(
			root,
			&message,
			"commit release pull request changes",
			false,
			|_, _| Err(io::Error::other("write failed")),
			flush_git_commit_message_file,
		))
		.err()
		.unwrap_or_else(|| panic!("expected write failure"))
		.to_string();
	let flush_error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(run_git_commit_message_with_io(
			root,
			&message,
			"commit release pull request changes",
			false,
			|_, _| Ok(()),
			|_| Err(io::Error::other("flush failed")),
		))
		.err()
		.unwrap_or_else(|| panic!("expected flush failure"))
		.to_string();

	assert!(write_error.contains("could not write temporary git commit message file"));
	assert!(write_error.contains("phase: writing temporary commit message file"));
	assert!(write_error.contains("temporary message file:"));
	assert!(flush_error.contains("could not flush temporary git commit message file"));
	assert!(flush_error.contains("phase: flushing temporary commit message file"));
	assert!(flush_error.contains("temporary message file:"));
}

#[test]
fn run_git_commit_message_commits_large_message_from_file() {
	let tempdir = tempdir("create repo");
	let root = tempdir.path();
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange Tests"]);
	git(root, &["config", "user.email", "monochange@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
	fs::write(root.join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git(root, &["add", "release.txt"]);

	let body = "release target metadata\n".repeat(30_000);
	tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(run_git_commit_message(
			root,
			&CommitMessage {
				subject: "chore(release): prepare release".to_string(),
				body: Some(body.clone()),
			},
			"commit release pull request changes",
			false,
		))
		.unwrap_or_else(|error| panic!("commit: {error}"));

	let message = git_stdout(root, &["log", "-1", "--pretty=%B"]);
	assert!(message.starts_with("chore(release): prepare release\n\n"));
	assert!(message.contains("release target metadata"));
	assert!(message.len() >= body.len());
}

#[test]
fn run_git_commit_message_reports_message_diagnostics_when_git_cannot_start() {
	let tempdir = tempdir("create parent");
	let missing = tempdir.path().join("missing");
	let error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(run_git_commit_message(
			&missing,
			&CommitMessage {
				subject: "chore(release): prepare release".to_string(),
				body: Some("release body".to_string()),
			},
			"commit release pull request changes",
			false,
		))
		.err()
		.unwrap_or_else(|| panic!("expected commit failure"));
	let error = error.to_string();

	assert!(error.contains("failed to commit release pull request changes"));
	assert!(error.contains("phase: spawning git commit with --file"));
	assert!(error.contains("command: git commit --file <temporary-message-file>"));
	assert!(error.contains("subject: chore(release): prepare release"));
	assert!(error.contains("body bytes: 12"));
	assert!(error.contains("full message bytes: 45"));
	assert!(error.contains("write the commit message to a file"));
	assert!(error.contains("avoid passing very large release records"));
}

#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::disallowed_methods)]
async fn run_commit_command_allow_nothing_to_commit_executes_git_commit() {
	let repo = tempdir("create git repo");
	git(repo.path(), &["init"]);
	git(repo.path(), &["config", "user.email", "test@example.com"]);
	git(repo.path(), &["config", "user.name", "Test User"]);
	git(repo.path(), &["config", "commit.gpgsign", "false"]);
	fs::write(repo.path().join("README.md"), "hello\n")
		.unwrap_or_else(|error| panic!("write README: {error}"));
	git(repo.path(), &["add", "README.md"]);

	let mut command = git_command(repo.path());
	command.args(["commit", "-m", "initial"]);
	run_commit_command_allow_nothing_to_commit(command, "commit changes")
		.await
		.unwrap_or_else(|error| panic!("commit changes: {error}"));

	let subject = git_stdout(repo.path(), &["log", "-1", "--pretty=%s"]);
	assert_eq!(subject.trim(), "initial");
}
