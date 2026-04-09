use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use monochange_core::git::git_command_output;
use monochange_core::git::git_commit_paths_command;
use monochange_core::git::git_error_detail;
use monochange_core::git::git_stage_paths_command;
use monochange_core::git::git_stderr_trimmed;
use monochange_core::git::git_stdout_trimmed;
use monochange_core::CommitMessage;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

pub(crate) fn resolve_git_tag_commit(root: &Path, tag_name: &str) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&[
			"rev-parse",
			"--verify",
			&format!("refs/tags/{tag_name}^{{commit}}"),
		],
		&format!("release tag {tag_name} could not be found"),
	)
}

pub(crate) fn git_is_ancestor(
	root: &Path,
	ancestor: &str,
	descendant: &str,
) -> MonochangeResult<bool> {
	let output = git_command_output(root, &["merge-base", "--is-ancestor", ancestor, descendant])
		.map_err(|error| {
		MonochangeError::Discovery(format!("failed to compare commit ancestry: {error}"))
	})?;
	match output.status.code() {
		Some(0) => Ok(true),
		Some(1) => Ok(false),
		_ => Err(MonochangeError::Discovery(git_stderr_trimmed(&output))),
	}
}

pub(crate) fn move_git_tag(
	root: &Path,
	tag_name: &str,
	target_commit: &str,
) -> MonochangeResult<()> {
	run_git_status(
		root,
		&["tag", "--force", tag_name, target_commit],
		&format!("failed to retarget tag `{tag_name}`"),
	)
}

pub(crate) fn push_git_tags(root: &Path, tags: &[&str]) -> MonochangeResult<()> {
	let mut args = vec!["push", "--force", "origin"];
	let tag_refs = tags
		.iter()
		.map(|tag| format!("refs/tags/{tag}:refs/tags/{tag}"))
		.collect::<Vec<_>>();
	for tag_ref in &tag_refs {
		args.push(tag_ref.as_str());
	}
	run_git_status(root, &args, "failed to push retargeted release tags")
}

pub(crate) fn resolve_git_commit_ref(root: &Path, from: &str) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["rev-parse", "--verify", &format!("{from}^{{commit}}")],
		&format!("could not resolve ref `{from}` to a commit"),
	)
}

#[rustfmt::skip]
pub(crate) fn first_parent_commits(root: &Path, commit: &str) -> MonochangeResult<Vec<String>> {
	let output = run_git_capture(
		root,
		&["rev-list", "--first-parent", commit],
		"failed to read first-parent commit ancestry",
	)?;
	Ok(output
		.lines()
		.map(str::to_string)
		.filter(|line| !line.is_empty())
		.collect())
}

pub(crate) fn read_git_commit_message(root: &Path, commit: &str) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["show", "-s", "--format=%B", commit],
		&format!("failed to read commit message for `{commit}`"),
	)
}

pub(crate) fn run_git_capture(
	root: &Path,
	args: &[&str],
	error_message: &str,
) -> MonochangeResult<String> {
	let output = git_command_output(root, args)
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;
	if !output.status.success() {
		let stderr = git_stderr_trimmed(&output);
		let detail = [error_message, stderr.as_str()]
			.into_iter()
			.filter(|part| !part.is_empty())
			.collect::<Vec<_>>()
			.join(": ");
		return Err(MonochangeError::Discovery(detail));
	}
	Ok(git_stdout_trimmed(&output))
}

pub(crate) fn run_git_status(
	root: &Path,
	args: &[&str],
	error_message: &str,
) -> MonochangeResult<()> {
	run_git_capture(root, args, error_message).map(|_| ())
}

pub(crate) fn git_stage_paths(root: &Path, tracked_paths: &[PathBuf]) -> MonochangeResult<()> {
	run_git_process(
		git_stage_paths_command(root, tracked_paths),
		"failed to stage release commit files",
	)
}

pub(crate) fn git_commit_paths(root: &Path, message: &CommitMessage) -> MonochangeResult<()> {
	run_git_process(
		git_commit_paths_command(root, message),
		"failed to create release commit",
	)
}

pub(crate) fn git_head_commit(root: &Path) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["rev-parse", "HEAD"],
		"failed to read release commit sha",
	)
}

pub(crate) fn run_git_process(
	mut command: ProcessCommand,
	error_message: &str,
) -> MonochangeResult<()> {
	let output = command
		.output()
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;
	if !output.status.success() {
		let stderr = git_error_detail(&output);
		let detail = [error_message, stderr.as_str()]
			.into_iter()
			.filter(|part| !part.is_empty())
			.collect::<Vec<_>>()
			.join(": ");
		return Err(MonochangeError::Discovery(detail));
	}
	Ok(())
}
