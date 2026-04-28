use std::fmt::Display;
use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;

use tempfile::NamedTempFile;

use crate::CommitMessage;
use crate::MonochangeError;
use crate::MonochangeResult;

fn stable_process_path() -> Option<&'static std::ffi::OsStr> {
	static PATH: std::sync::OnceLock<Option<std::ffi::OsString>> = std::sync::OnceLock::new();
	PATH.get_or_init(resolve_stable_process_path).as_deref()
}

fn resolve_stable_process_path() -> Option<std::ffi::OsString> {
	resolve_process_path(
		std::env::var_os("PATH"),
		option_env!("PATH").map(std::ffi::OsString::from),
	)
}

fn resolve_process_path(
	current: Option<std::ffi::OsString>,
	fallback: Option<std::ffi::OsString>,
) -> Option<std::ffi::OsString> {
	if current.as_deref().is_some_and(path_contains_git) {
		return current;
	}

	fallback.filter(|path| path_contains_git(path)).or(current)
}

fn path_contains_git(path: &std::ffi::OsStr) -> bool {
	std::env::split_paths(path).any(|entry| entry.join(git_executable_name()).is_file())
}

#[cfg(windows)]
fn git_executable_name() -> &'static str {
	"git.exe"
}

#[cfg(not(windows))]
fn git_executable_name() -> &'static str {
	"git"
}

/// Build a `git` command scoped to `root` with conflicting git env removed.
#[must_use]
pub fn git_command(root: &Path) -> Command {
	let mut command = Command::new("git");
	command.current_dir(root);
	if let Some(path) = stable_process_path() {
		command.env("PATH", path);
	}

	for variable in [
		"GIT_DIR",
		"GIT_WORK_TREE",
		"GIT_COMMON_DIR",
		"GIT_INDEX_FILE",
		"GIT_OBJECT_DIRECTORY",
		"GIT_ALTERNATE_OBJECT_DIRECTORIES",
	] {
		command.env_remove(variable);
	}

	command
}

/// Build a `git checkout -B` command for `branch`.
#[must_use]
pub fn git_checkout_branch_command(root: &Path, branch: &str) -> Command {
	let mut command = git_command(root);
	command.arg("checkout").arg("-B").arg(branch);
	command
}

/// Build a `git add -A -- ...` command for the provided tracked paths.
#[must_use]
pub fn git_stage_paths_command(root: &Path, tracked_paths: &[PathBuf]) -> Command {
	let mut command = git_command(root);
	command.arg("add").arg("-A").arg("--");
	for path in tracked_paths {
		command.arg(path);
	}
	command
}

/// Render a complete git commit message from the supplied monochange commit message.
#[must_use]
pub fn git_commit_message_text(message: &CommitMessage) -> String {
	match &message.body {
		Some(body) => format!("{}\n\n{}", message.subject, body),
		None => message.subject.clone(),
	}
}

/// Build a `git commit` command for the supplied monochange commit message.
#[must_use]
pub fn git_commit_paths_command(root: &Path, message: &CommitMessage, no_verify: bool) -> Command {
	let mut command = git_command(root);
	command.arg("commit");
	if no_verify {
		command.arg("--no-verify");
	}
	command.arg("--message").arg(&message.subject);
	if let Some(body) = &message.body {
		command.arg("--message").arg(body);
	}
	command
}

/// Build a `git commit` command that reads the full commit message from stdin.
#[must_use]
pub fn git_commit_paths_stdin_command(root: &Path, no_verify: bool) -> Command {
	let mut command = git_command(root);
	command.arg("commit");
	if no_verify {
		command.arg("--no-verify");
	}
	command.arg("--file").arg("-").stdin(Stdio::piped());
	command
}

/// Build a `git commit` command that reads the message from `message_file`.
#[must_use]
pub fn git_commit_file_command(root: &Path, message_file: &Path, no_verify: bool) -> Command {
	let mut command = git_command(root);
	command.arg("commit");
	if no_verify {
		command.arg("--no-verify");
	}
	command.arg("--file").arg(message_file);
	command
}

/// Build a force-with-lease push command for `branch`.
#[must_use]
pub fn git_push_branch_command(root: &Path, branch: &str, no_verify: bool) -> Command {
	let mut command = git_command(root);
	command.arg("push");
	if no_verify {
		command.arg("--no-verify");
	}
	command
		.arg("--force-with-lease")
		.arg("origin")
		.arg(format!("HEAD:{branch}"));
	command
}

/// Return the current branch name for the repository at `root`.
#[must_use = "the git branch result must be checked"]
pub fn git_current_branch(root: &Path) -> MonochangeResult<String> {
	let output =
		git_command_output(root, &["symbolic-ref", "--short", "HEAD"]).map_err(|error| {
			MonochangeError::Io(format!("failed to read current git branch: {error}"))
		})?;

	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"failed to read current git branch: {}",
			git_error_detail(&output)
		)));
	}

	Ok(git_stdout_trimmed(&output))
}

/// Return the full HEAD commit SHA for the repository at `root`.
#[must_use = "the HEAD commit result must be checked"]
pub fn git_head_commit(root: &Path) -> MonochangeResult<String> {
	let output = git_command_output(root, &["rev-parse", "HEAD"])
		.map_err(|error| MonochangeError::Io(format!("failed to read HEAD commit: {error}")))?;

	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"failed to read HEAD commit: {}",
			git_error_detail(&output)
		)));
	}

	Ok(git_stdout_trimmed(&output))
}

#[tracing::instrument(skip_all, fields(args = ?args))]
/// Run `git` with the supplied args and capture the raw process output.
#[must_use = "the command output must be checked"]
pub fn git_command_output(root: &Path, args: &[&str]) -> std::io::Result<Output> {
	let mut command = git_command(root);
	command.args(args).output()
}

/// Return stdout as trimmed UTF-8 lossily decoded text.
#[must_use]
pub fn git_stdout_trimmed(output: &Output) -> String {
	String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Return stderr as trimmed UTF-8 lossily decoded text.
#[must_use]
pub fn git_stderr_trimmed(output: &Output) -> String {
	String::from_utf8_lossy(&output.stderr).trim().to_string()
}

/// Prefer stderr over stdout when rendering a git failure detail string.
#[must_use]
pub fn git_error_detail(output: &Output) -> String {
	let stdout = git_stdout_trimmed(output);
	let stderr = git_stderr_trimmed(output);
	if stderr.is_empty() { stdout } else { stderr }
}

/// Detect the standard `nothing to commit` git response.
#[must_use]
pub fn git_reports_nothing_to_commit(output: &Output) -> bool {
	git_stdout_trimmed(output).contains("nothing to commit")
		|| git_stderr_trimmed(output).contains("nothing to commit")
}

#[tracing::instrument(skip_all, fields(action))]
/// Run a prepared command and convert process failures into `MonochangeError`.
#[must_use = "the command result must be checked"]
pub fn run_command(mut command: Command, action: &str) -> MonochangeResult<()> {
	let output = command
		.output()
		.map_err(|error| MonochangeError::Io(format!("failed to {action}: {error}")))?;

	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"failed to {action}: {}",
			git_error_detail(&output)
		)));
	}

	Ok(())
}

/// Run a `git commit` command with a commit message stored in a temporary file.
#[must_use = "the commit command result must be checked"]
pub fn run_git_commit_message(
	root: &Path,
	message: &CommitMessage,
	action: &str,
	no_verify: bool,
) -> MonochangeResult<()> {
	run_git_commit_message_with_io(
		root,
		message,
		action,
		no_verify,
		|message_file, message_text| message_file.write_all(message_text.as_bytes()),
		flush_git_commit_message_file,
	)
}

fn flush_git_commit_message_file(message_file: &mut NamedTempFile) -> std::io::Result<()> {
	message_file.as_file_mut().sync_all()
}

fn run_git_commit_message_with_io<WriteMessage, FlushMessage>(
	root: &Path,
	message: &CommitMessage,
	action: &str,
	no_verify: bool,
	write_message: WriteMessage,
	flush_message: FlushMessage,
) -> MonochangeResult<()>
where
	WriteMessage: FnOnce(&mut NamedTempFile, &str) -> std::io::Result<()>,
	FlushMessage: FnOnce(&mut NamedTempFile) -> std::io::Result<()>,
{
	let message_text = git_commit_message_text(message);
	let mut message_file =
		NamedTempFile::new_in(git_commit_message_temp_dir(root)).map_err(|error| {
			git_commit_create_temp_file_error(
				root,
				message,
				&message_text,
				action,
				no_verify,
				error,
			)
		})?;
	write_message(&mut message_file, &message_text).map_err(|error| {
		git_commit_write_temp_file_error(
			root,
			message,
			&message_text,
			action,
			no_verify,
			message_file.path(),
			error,
		)
	})?;
	flush_message(&mut message_file).map_err(|error| {
		git_commit_flush_temp_file_error(
			root,
			message,
			&message_text,
			action,
			no_verify,
			message_file.path(),
			error,
		)
	})?;

	let mut command = git_commit_file_command(root, message_file.path(), no_verify);
	let output = command.output().map_err(|error| {
		MonochangeError::Io(format!(
			"failed to {action}: {error}\n{}",
			git_commit_message_diagnostics(
				root,
				message,
				&message_text,
				action,
				"spawning git commit with --file",
				no_verify,
				Some(message_file.path()),
			)
		))
	})?;

	if output.status.success() || git_reports_nothing_to_commit(&output) {
		return Ok(());
	}

	Err(MonochangeError::Config(format!(
		"failed to {action}: {}\n{}",
		git_error_detail(&output),
		git_commit_message_diagnostics(
			root,
			message,
			&message_text,
			action,
			"executing git commit with --file",
			no_verify,
			Some(message_file.path()),
		)
	)))
}

/// Run a commit command and treat `nothing to commit` as success.
#[must_use = "the commit command result must be checked"]
pub fn run_commit_command_allow_nothing_to_commit(
	mut command: Command,
	action: &str,
) -> MonochangeResult<()> {
	let output = command
		.output()
		.map_err(|error| MonochangeError::Io(format!("failed to {action}: {error}")))?;

	if output.status.success() || git_reports_nothing_to_commit(&output) {
		return Ok(());
	}

	Err(MonochangeError::Config(format!(
		"failed to {action}: {}",
		git_error_detail(&output)
	)))
}

fn git_commit_message_temp_dir(root: &Path) -> &Path {
	root.parent().unwrap_or(root)
}

fn git_commit_create_temp_file_error(
	root: &Path,
	message: &CommitMessage,
	message_text: &str,
	action: &str,
	no_verify: bool,
	error: impl Display,
) -> MonochangeError {
	MonochangeError::Io(format!(
		"failed to {action}: could not create temporary git commit message file: {error}\n{}",
		git_commit_message_diagnostics(
			root,
			message,
			message_text,
			action,
			"creating temporary commit message file",
			no_verify,
			None,
		)
	))
}

fn git_commit_write_temp_file_error(
	root: &Path,
	message: &CommitMessage,
	message_text: &str,
	action: &str,
	no_verify: bool,
	message_file: &Path,
	error: impl Display,
) -> MonochangeError {
	MonochangeError::Io(format!(
		"failed to {action}: could not write temporary git commit message file {}: {error}\n{}",
		message_file.display(),
		git_commit_message_diagnostics(
			root,
			message,
			message_text,
			action,
			"writing temporary commit message file",
			no_verify,
			Some(message_file),
		)
	))
}

fn git_commit_flush_temp_file_error(
	root: &Path,
	message: &CommitMessage,
	message_text: &str,
	action: &str,
	no_verify: bool,
	message_file: &Path,
	error: impl Display,
) -> MonochangeError {
	MonochangeError::Io(format!(
		"failed to {action}: could not flush temporary git commit message file {}: {error}\n{}",
		message_file.display(),
		git_commit_message_diagnostics(
			root,
			message,
			message_text,
			action,
			"flushing temporary commit message file",
			no_verify,
			Some(message_file),
		)
	))
}

fn git_commit_message_diagnostics(
	root: &Path,
	message: &CommitMessage,
	message_text: &str,
	action: &str,
	phase: &str,
	no_verify: bool,
	message_file: Option<&Path>,
) -> String {
	let body = message.body.as_deref().unwrap_or("");
	let message_file = message_file.map_or_else(
		|| "<not yet created>".to_string(),
		|path| path.display().to_string(),
	);
	let mut diagnostics = String::new();
	let _ = writeln!(diagnostics, "git commit diagnostics:");
	let _ = writeln!(diagnostics, "  action: {action}");
	let _ = writeln!(diagnostics, "  phase: {phase}");
	let _ = writeln!(diagnostics, "  repository: {}", root.display());
	let _ = writeln!(
		diagnostics,
		"  command: git commit --file <temporary-message-file>"
	);
	let _ = writeln!(diagnostics, "  temporary message file: {message_file}");
	let _ = writeln!(diagnostics, "  no_verify: {no_verify}");
	let _ = writeln!(diagnostics, "  subject bytes: {}", message.subject.len());
	let _ = writeln!(diagnostics, "  body bytes: {}", body.len());
	let _ = writeln!(diagnostics, "  full message bytes: {}", message_text.len());
	let _ = writeln!(diagnostics, "  subject: {}", message.subject);
	if message.body.is_some() {
		let _ = writeln!(diagnostics, "  body preview: {}", preview_text(body));
	} else {
		let _ = writeln!(diagnostics, "  body preview: <none>");
	}
	let _ = writeln!(
		diagnostics,
		"  full message preview: {}",
		preview_text(message_text)
	);
	let _ = writeln!(diagnostics, "suggested manual commit approaches:");
	let _ = writeln!(
		diagnostics,
		"  - write the commit message to a file and run `git commit --file <message-file>` from the repository root"
	);
	let _ = writeln!(
		diagnostics,
		"  - if a hook is failing, rerun the same command locally and inspect the hook output"
	);
	let _ = writeln!(
		diagnostics,
		"  - avoid passing very large release records with repeated `--message`/`-m` arguments because OS argv limits can reject them before git starts"
	);
	diagnostics
}

fn preview_text(text: &str) -> String {
	const PREVIEW_CHARS: usize = 240;
	let mut chars = text.chars();
	let preview = chars.by_ref().take(PREVIEW_CHARS).collect::<String>();
	if chars.next().is_some() {
		format!("{}…", preview.replace('\n', "\\n"))
	} else {
		preview.replace('\n', "\\n")
	}
}

#[cfg(test)]
mod tests {
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
			.args(args)
			.status()
			.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
		assert!(status.success(), "git {args:?} failed with {status}");
	}

	fn git_stdout(root: &Path, args: &[&str]) -> String {
		let output = git_command(root)
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
		let error = run_git_commit_message(
			&missing_nested_root,
			&CommitMessage {
				subject: "chore(release): prepare release".to_string(),
				body: Some("release body".to_string()),
			},
			"commit release pull request changes",
			false,
		)
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

		let write_error = run_git_commit_message_with_io(
			root,
			&message,
			"commit release pull request changes",
			false,
			|_, _| Err(io::Error::other("write failed")),
			flush_git_commit_message_file,
		)
		.err()
		.unwrap_or_else(|| panic!("expected write failure"))
		.to_string();
		let flush_error = run_git_commit_message_with_io(
			root,
			&message,
			"commit release pull request changes",
			false,
			|_, _| Ok(()),
			|_| Err(io::Error::other("flush failed")),
		)
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
		fs::write(root.join("release.txt"), "release\n")
			.unwrap_or_else(|error| panic!("write release file: {error}"));
		git(root, &["add", "release.txt"]);

		let body = "release target metadata\n".repeat(30_000);
		run_git_commit_message(
			root,
			&CommitMessage {
				subject: "chore(release): prepare release".to_string(),
				body: Some(body.clone()),
			},
			"commit release pull request changes",
			false,
		)
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
		let error = run_git_commit_message(
			&missing,
			&CommitMessage {
				subject: "chore(release): prepare release".to_string(),
				body: Some("release body".to_string()),
			},
			"commit release pull request changes",
			false,
		)
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
}
