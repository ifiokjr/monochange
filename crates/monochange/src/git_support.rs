#[cfg(test)]
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
#[cfg(test)]
use std::process::Stdio;

use monochange_core::CommitMessage;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::git::git_command_output;
use monochange_core::git::git_error_detail;
use monochange_core::git::git_stage_paths_command;
use monochange_core::git::git_stderr_trimmed;
use monochange_core::git::git_stdout_trimmed;
use monochange_core::git::run_git_commit_message;

#[must_use = "the tag commit result must be checked"]
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

pub(crate) fn create_git_tag(
	root: &Path,
	tag_name: &str,
	target_commit: &str,
) -> MonochangeResult<()> {
	run_git_status(
		root,
		&["tag", tag_name, target_commit],
		&format!("failed to create tag `{tag_name}`"),
	)
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

#[must_use = "the push result must be checked"]
pub(crate) fn push_git_tags(root: &Path, tags: &[&str]) -> MonochangeResult<()> {
	push_git_tags_with_options(root, tags, true, "failed to push retargeted release tags")
}

#[must_use = "the push result must be checked"]
pub(crate) fn push_git_tags_without_force(root: &Path, tags: &[&str]) -> MonochangeResult<()> {
	push_git_tags_with_options(root, tags, false, "failed to push release tags")
}

fn push_git_tags_with_options(
	root: &Path,
	tags: &[&str],
	force: bool,
	error_message: &str,
) -> MonochangeResult<()> {
	let mut args = vec!["push"];
	if force {
		args.push("--force");
	}
	args.push("origin");

	let tag_refs: Vec<String> = tags
		.iter()
		.map(|tag| format!("refs/tags/{tag}:refs/tags/{tag}"))
		.collect();

	for tag_ref in &tag_refs {
		args.push(tag_ref.as_str());
	}

	run_git_status(root, &args, error_message)
}

#[must_use = "the ref resolution result must be checked"]
pub(crate) fn resolve_git_commit_ref(root: &Path, from: &str) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["rev-parse", "--verify", &format!("{from}^{{commit}}")],
		&format!("could not resolve ref `{from}` to a commit"),
	)
}

#[must_use = "the commit history result must be checked"]
#[rustfmt::skip]
#[tracing::instrument(skip_all, fields(commit))]
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

pub(crate) fn read_git_file_at_commit(
	root: &Path,
	commit: &str,
	path: &str,
) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["show", &format!("{commit}:{path}")],
		&format!("failed to read `{path}` at commit `{commit}`"),
	)
}

pub(crate) fn find_release_record_files_at_commit(
	root: &Path,
	commit: &str,
) -> MonochangeResult<Vec<String>> {
	let prefix = ".monochange/releases/";
	let suffix = "/release.json";
	let filter = |line: &&str| line.starts_with(prefix) && line.ends_with(suffix);

	let has_parent = run_git_capture(
		root,
		&["cat-file", "-p", commit],
		"failed to inspect commit",
	)?
	.lines()
	.any(|line| line.starts_with("parent "));

	if has_parent {
		let args = [
			"diff-tree",
			"-m",
			"--no-commit-id",
			"--name-only",
			"-r",
			commit,
		];
		let output = run_git_capture(root, &args, "failed to list files at commit")?;
		Ok(output
			.lines()
			.filter(filter)
			.map(str::to_string)
			.collect::<std::collections::HashSet<_>>()
			.into_iter()
			.collect())
	} else {
		let args = ["ls-tree", "-r", "--name-only", commit];
		let output = run_git_capture(root, &args, "failed to list files at commit")?;
		Ok(output.lines().filter(filter).map(str::to_string).collect())
	}
}

#[allow(dead_code)]
#[must_use = "the commit message result must be checked"]
pub(crate) fn read_git_commit_message(root: &Path, commit: &str) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["show", "-s", "--format=%B", commit],
		&format!("failed to read commit message for `{commit}`"),
	)
}

#[tracing::instrument(skip_all, fields(args = ?args))]
pub(crate) fn run_git_capture(
	root: &Path,
	args: &[&str],
	error_message: &str,
) -> MonochangeResult<String> {
	let output = git_command_output(root, args)
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;

	if !output.status.success() {
		let stderr = git_stderr_trimmed(&output);
		tracing::warn!(args = ?args, %stderr, "git command failed");

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

#[must_use = "the staging result must be checked"]
pub(crate) fn git_stage_paths(root: &Path, tracked_paths: &[PathBuf]) -> MonochangeResult<()> {
	let stageable_paths = resolve_stageable_release_paths(root, tracked_paths)?;

	if stageable_paths.is_empty() {
		let skipped_count = tracked_paths.len();
		tracing::debug!(
			count = skipped_count,
			"no release commit paths required staging"
		);

		return Ok(());
	}

	let stageable_count = stageable_paths.len();
	tracing::debug!(
		count = stageable_count,
		?stageable_paths,
		"staging release commit paths"
	);

	run_git_process(
		git_stage_paths_command(root, &stageable_paths),
		"failed to stage release commit files",
	)
}

fn resolve_stageable_release_paths(
	root: &Path,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<Vec<PathBuf>> {
	let mut stageable_paths = Vec::with_capacity(tracked_paths.len());

	for path in tracked_paths {
		if release_path_requires_staging(root, path)? {
			stageable_paths.push(path.clone());
		} else {
			tracing::debug!(path = %path.display(), "skipping non-stageable release path");
		}
	}

	Ok(stageable_paths)
}

fn release_path_requires_staging(root: &Path, path: &Path) -> MonochangeResult<bool> {
	let absolute_path = root.join(path);

	if !absolute_path.exists() {
		return git_path_is_tracked(root, path);
	}

	if git_path_is_tracked(root, path)? {
		return Ok(true);
	}

	Ok(!git_path_is_ignored(root, path)?)
}

#[must_use = "the tracked status result must be checked"]
fn git_path_is_tracked(root: &Path, path: &Path) -> MonochangeResult<bool> {
	let relative = path.to_string_lossy();
	let output = git_command_output(root, &["ls-files", "--error-unmatch", "--", &relative])
		.map_err(|error| {
			MonochangeError::Discovery(format!(
				"failed to inspect tracked git path {}: {error}",
				path.display()
			))
		})?;

	match output.status.code() {
		Some(0) => Ok(true),
		Some(1) => Ok(false),
		_ => {
			Err(MonochangeError::Discovery(format!(
				"failed to inspect tracked git path {}: {}",
				path.display(),
				git_error_detail(&output)
			)))
		}
	}
}

#[must_use = "the ignored status result must be checked"]
fn git_path_is_ignored(root: &Path, path: &Path) -> MonochangeResult<bool> {
	let relative = path.to_string_lossy();
	let output =
		git_command_output(root, &["check-ignore", "-q", "--", &relative]).map_err(|error| {
			MonochangeError::Discovery(format!(
				"failed to inspect ignored git path {}: {error}",
				path.display()
			))
		})?;

	match output.status.code() {
		Some(0) => Ok(true),
		Some(1) => Ok(false),
		_ => {
			Err(MonochangeError::Discovery(format!(
				"failed to inspect ignored git path {}: {}",
				path.display(),
				git_error_detail(&output)
			)))
		}
	}
}

#[must_use = "the commit result must be checked"]
pub(crate) fn git_commit_paths(
	root: &Path,
	message: &CommitMessage,
	no_verify: bool,
) -> MonochangeResult<()> {
	run_git_commit_message(root, message, "create release commit", no_verify)
}

#[must_use = "the HEAD commit result must be checked"]
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
	handle_git_process_output(&output, error_message)
}

#[cfg(test)]
pub(crate) fn run_git_process_with_stdin(
	mut command: ProcessCommand,
	input: &[u8],
	error_message: &str,
) -> MonochangeResult<()> {
	let mut child = command
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;
	if let Some(mut stdin) = child.stdin.take() {
		stdin
			.write_all(input)
			.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;
	}
	let output = child
		.wait_with_output()
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;
	handle_git_process_output(&output, error_message)
}

fn handle_git_process_output(
	output: &std::process::Output,
	error_message: &str,
) -> MonochangeResult<()> {
	if !output.status.success() {
		let stderr = git_error_detail(output);
		let detail = [error_message, stderr.as_str()]
			.into_iter()
			.filter(|part| !part.is_empty())
			.collect::<Vec<_>>()
			.join(": ");
		return Err(MonochangeError::Discovery(detail));
	}
	Ok(())
}

#[cfg(test)]
#[path = "__tests/git_support.rs"]
mod tests;
