use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;

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

#[cfg(test)]
mod tests {
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
		std::fs::write(directory.path().join(git_executable_name()), "")
			.unwrap_or_else(|error| panic!("write git shim: {error}"));
		directory
	}

	fn tempdir(context: &str) -> tempfile::TempDir {
		tempfile::tempdir().unwrap_or_else(|error| panic!("{context}: {error}"))
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
}
