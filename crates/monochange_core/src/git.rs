use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use crate::CommitMessage;
use crate::MonochangeError;
use crate::MonochangeResult;

/// Build a `git` command scoped to `root` with conflicting git env removed.
#[must_use]
pub fn git_command(root: &Path) -> Command {
	let mut command = Command::new("git");
	command.current_dir(root);

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

/// Build a `git commit` command for the supplied monochange commit message.
#[must_use]
pub fn git_commit_paths_command(root: &Path, message: &CommitMessage) -> Command {
	let mut command = git_command(root);
	command.arg("commit").arg("--message").arg(&message.subject);
	if let Some(body) = &message.body {
		command.arg("--message").arg(body);
	}
	command
}

/// Build a force-with-lease push command for `branch`.
#[must_use]
pub fn git_push_branch_command(root: &Path, branch: &str) -> Command {
	let mut command = git_command(root);
	command
		.arg("push")
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
