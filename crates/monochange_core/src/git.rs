use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use crate::CommitMessage;
use crate::MonochangeError;
use crate::MonochangeResult;

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

#[must_use]
pub fn git_checkout_branch_command(root: &Path, branch: &str) -> Command {
	let mut command = git_command(root);
	command.arg("checkout").arg("-B").arg(branch);
	command
}

#[must_use]
pub fn git_stage_paths_command(root: &Path, tracked_paths: &[PathBuf]) -> Command {
	let mut command = git_command(root);
	command.arg("add").arg("-A").arg("--");
	for path in tracked_paths {
		command.arg(path);
	}
	command
}

#[must_use]
pub fn git_commit_paths_command(root: &Path, message: &CommitMessage) -> Command {
	let mut command = git_command(root);
	command.arg("commit").arg("--message").arg(&message.subject);
	if let Some(body) = &message.body {
		command.arg("--message").arg(body);
	}
	command
}

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

#[tracing::instrument(skip_all, fields(args = ?args))]
pub fn git_command_output(root: &Path, args: &[&str]) -> std::io::Result<Output> {
	let mut command = git_command(root);
	command.args(args).output()
}

#[must_use]
pub fn git_stdout_trimmed(output: &Output) -> String {
	String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[must_use]
pub fn git_stderr_trimmed(output: &Output) -> String {
	String::from_utf8_lossy(&output.stderr).trim().to_string()
}

#[must_use]
pub fn git_error_detail(output: &Output) -> String {
	let stdout = git_stdout_trimmed(output);
	let stderr = git_stderr_trimmed(output);
	if stderr.is_empty() {
		stdout
	} else {
		stderr
	}
}

#[must_use]
pub fn git_reports_nothing_to_commit(output: &Output) -> bool {
	git_stdout_trimmed(output).contains("nothing to commit")
		|| git_stderr_trimmed(output).contains("nothing to commit")
}

#[tracing::instrument(skip_all, fields(action))]
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
