use std::fmt::Display;
use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;
use std::path::Path;
use std::path::PathBuf;
use tokio::process::Command;
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
	command.arg("add").arg("-A").arg("-f").arg("--");
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
pub async fn git_current_branch(root: &Path) -> MonochangeResult<String> {
	let output =
		git_command_output(root, &["symbolic-ref", "--short", "HEAD"]).await.map_err(|error| {
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
pub async fn git_head_commit(root: &Path) -> MonochangeResult<String> {
	let output = git_command_output(root, &["rev-parse", "HEAD"])
		.await
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
pub async fn git_command_output(root: &Path, args: &[&str]) -> std::io::Result<Output> {
	let mut command = git_command(root);
	command.args(args).output().await
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
pub async fn run_command(mut command: Command, action: &str) -> MonochangeResult<()> {
	let output = command
		.output()
		.await
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
pub async fn run_git_commit_message(
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
	.await
}

fn flush_git_commit_message_file(message_file: &mut NamedTempFile) -> std::io::Result<()> {
	message_file.as_file_mut().sync_all()
}

async fn run_git_commit_message_with_io<WriteMessage, FlushMessage>(
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
	let output = command.output().await.map_err(|error| {
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
pub async fn run_commit_command_allow_nothing_to_commit(
	mut command: Command,
	action: &str,
) -> MonochangeResult<()> {
	let output = command
		.output().await
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
#[path = "__tests__/git_tests.rs"]
mod tests;
