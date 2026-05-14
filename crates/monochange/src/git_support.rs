use std::fs;
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
#[cfg(test)]
use tokio::io::AsyncWriteExt;

#[must_use = "the tag commit result must be checked"]
pub(crate) async fn resolve_git_tag_commit(
	root: &Path,
	tag_name: &str,
) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&[
			"rev-parse",
			"--verify",
			&format!("refs/tags/{tag_name}^{{commit}}"),
		],
		&format!("release tag {tag_name} could not be found"),
	)
	.await
}

pub(crate) async fn git_is_ancestor(
	root: &Path,
	ancestor: &str,
	descendant: &str,
) -> MonochangeResult<bool> {
	let output = git_command_output(root, &["merge-base", "--is-ancestor", ancestor, descendant])
		.await
		.map_err(|error| {
			MonochangeError::Discovery(format!("failed to compare commit ancestry: {error}"))
		})?;

	match output.status.code() {
		Some(0) => Ok(true),
		Some(1) => Ok(false),
		_ => Err(MonochangeError::Discovery(git_stderr_trimmed(&output))),
	}
}

pub(crate) async fn create_git_tag(
	root: &Path,
	tag_name: &str,
	target_commit: &str,
) -> MonochangeResult<()> {
	run_git_status(
		root,
		&["tag", tag_name, target_commit],
		&format!("failed to create tag `{tag_name}`"),
	)
	.await
}

pub(crate) async fn move_git_tag(
	root: &Path,
	tag_name: &str,
	target_commit: &str,
) -> MonochangeResult<()> {
	run_git_status(
		root,
		&["tag", "--force", tag_name, target_commit],
		&format!("failed to retarget tag `{tag_name}`"),
	)
	.await
}

#[must_use = "the push result must be checked"]
pub(crate) async fn push_git_tags(root: &Path, tags: &[&str]) -> MonochangeResult<()> {
	push_git_tags_with_options(root, tags, true, "failed to push retargeted release tags").await
}

#[must_use = "the push result must be checked"]
pub(crate) async fn push_git_tags_without_force(
	root: &Path,
	tags: &[&str],
) -> MonochangeResult<()> {
	push_git_tags_with_options(root, tags, false, "failed to push release tags").await
}

async fn push_git_tags_with_options(
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

	run_git_status(root, &args, error_message).await
}

#[must_use = "the ref resolution result must be checked"]
pub(crate) async fn resolve_git_commit_ref(root: &Path, from: &str) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["rev-parse", "--verify", &format!("{from}^{{commit}}")],
		&format!("could not resolve ref `{from}` to a commit"),
	)
	.await
}

#[must_use = "the commit history result must be checked"]
#[tracing::instrument(skip_all, fields(commit))]
pub(crate) async fn first_parent_commits(
	root: &Path,
	commit: &str,
) -> MonochangeResult<Vec<String>> {
	let output = run_git_capture(
		root,
		&["rev-list", "--first-parent", commit],
		"failed to read first-parent commit ancestry",
	)
	.await?;

	Ok(output
		.lines()
		.map(str::to_string)
		.filter(|line| !line.is_empty())
		.collect())
}

pub(crate) async fn read_git_file_at_commit(
	root: &Path,
	commit: &str,
	path: &str,
) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["show", &format!("{commit}:{path}")],
		&format!("failed to read `{path}` at commit `{commit}`"),
	)
	.await
}

pub(crate) async fn find_release_record_files_at_commit(
	root: &Path,
	commit: &str,
) -> MonochangeResult<Vec<String>> {
	let prefix = ".monochange/releases/";
	let suffix = "/release.json";
	let filter = |line: &&str| line.starts_with(prefix) && line.ends_with(suffix);

	let first_parent = run_git_capture(
		root,
		&["cat-file", "-p", commit],
		"failed to inspect commit",
	)
	.await?
	.lines()
	.find_map(|line| line.strip_prefix("parent ").map(str::to_string));

	let resolved_commit =
		run_git_capture(root, &["rev-parse", commit], "failed to resolve commit").await?;
	let shallow_file = run_git_capture(
		root,
		&["rev-parse", "--git-path", "shallow"],
		"failed to resolve shallow boundary path",
	)
	.await
	.unwrap_or_else(|_| String::from(".git/shallow"));
	let shallow_file = root.join(PathBuf::from(shallow_file.trim()));
	let is_shallow_boundary = fs::read_to_string(shallow_file)
		.is_ok_and(|contents| contents.lines().any(|line| line == resolved_commit.trim()));
	let has_available_parent = if let Some(parent) = first_parent.as_deref() {
		git_command_output(root, &["cat-file", "-e", &format!("{parent}^{{commit}}")])
			.await
			.is_ok()
	} else {
		false
	};

	if has_available_parent && !is_shallow_boundary {
		let args = [
			"diff-tree",
			"-m",
			"--no-commit-id",
			"--name-only",
			"-r",
			commit,
		];
		let output = run_git_capture(root, &args, "failed to list files at commit").await?;
		Ok(output
			.lines()
			.filter(filter)
			.map(str::to_string)
			.collect::<std::collections::HashSet<_>>()
			.into_iter()
			.collect())
	} else {
		let args = ["ls-tree", "-r", "--name-only", commit];
		let output = run_git_capture(root, &args, "failed to list files at commit").await?;
		Ok(output.lines().filter(filter).map(str::to_string).collect())
	}
}

#[allow(dead_code)]
#[must_use = "the commit message result must be checked"]
pub(crate) async fn read_git_commit_message(root: &Path, commit: &str) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["show", "-s", "--format=%B", commit],
		&format!("failed to read commit message for `{commit}`"),
	)
	.await
}

#[tracing::instrument(skip_all, fields(args = ?args))]
pub(crate) async fn run_git_capture(
	root: &Path,
	args: &[&str],
	error_message: &str,
) -> MonochangeResult<String> {
	let output = git_command_output(root, args)
		.await
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;

	if !output.status.success() {
		let stderr = git_stderr_trimmed(&output);
		tracing::warn!(args = ?args, %stderr, "git command failed");

		return Err(MonochangeError::Discovery(git_error_message_with_detail(
			error_message,
			&stderr,
		)));
	}

	Ok(git_stdout_trimmed(&output))
}

pub(crate) async fn run_git_status(
	root: &Path,
	args: &[&str],
	error_message: &str,
) -> MonochangeResult<()> {
	run_git_capture(root, args, error_message).await.map(|_| ())
}

#[must_use = "the staging result must be checked"]
pub(crate) async fn git_stage_paths(
	root: &Path,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<()> {
	let stageable_paths = resolve_stageable_release_paths(root, tracked_paths).await?;

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
	.await
}

async fn resolve_stageable_release_paths(
	root: &Path,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<Vec<PathBuf>> {
	let mut stageable_paths = Vec::with_capacity(tracked_paths.len());

	for path in tracked_paths {
		if release_path_requires_staging(root, path).await? {
			stageable_paths.push(path.clone());
		} else {
			tracing::debug!(path = %path.display(), "skipping non-stageable release path");
		}
	}

	Ok(stageable_paths)
}

async fn release_path_requires_staging(root: &Path, path: &Path) -> MonochangeResult<bool> {
	let absolute_path = root.join(path);

	if !absolute_path.exists() {
		return git_path_is_tracked(root, path).await;
	}

	if git_path_is_tracked(root, path).await? {
		return Ok(true);
	}

	let relative = if path.is_absolute() {
		path.strip_prefix(root).unwrap_or(path)
	} else {
		path
	};
	if relative.starts_with(".monochange/releases") {
		return Ok(true);
	}

	Ok(!git_path_is_ignored(root, path).await?)
}

#[must_use = "the tracked status result must be checked"]
async fn git_path_is_tracked(root: &Path, path: &Path) -> MonochangeResult<bool> {
	let relative = path.to_string_lossy();
	let output = git_command_output(root, &["ls-files", "--error-unmatch", "--", &relative])
		.await
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
async fn git_path_is_ignored(root: &Path, path: &Path) -> MonochangeResult<bool> {
	let relative = path.to_string_lossy();
	let output = git_command_output(root, &["check-ignore", "-q", "--", &relative])
		.await
		.map_err(|error| {
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
pub(crate) async fn git_commit_paths(
	root: &Path,
	message: &CommitMessage,
	no_verify: bool,
) -> MonochangeResult<()> {
	run_git_commit_message(root, message, "create release commit", no_verify).await
}

#[must_use = "the HEAD commit result must be checked"]
pub(crate) async fn git_head_commit(root: &Path) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["rev-parse", "HEAD"],
		"failed to read release commit sha",
	)
	.await
}

pub(crate) async fn run_git_process(
	command: ProcessCommand,
	error_message: &str,
) -> MonochangeResult<()> {
	let output = tokio::process::Command::from(command)
		.output()
		.await
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;
	handle_git_process_output(&output, error_message)
}

#[cfg(test)]
pub(crate) async fn run_git_process_with_stdin(
	command: ProcessCommand,
	input: &[u8],
	error_message: &str,
) -> MonochangeResult<()> {
	let mut child = tokio::process::Command::from(command)
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;
	if let Some(mut stdin) = child.stdin.take() {
		stdin
			.write_all(input)
			.await
			.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;
	}
	let output = child
		.wait_with_output()
		.await
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;
	handle_git_process_output(&output, error_message)
}

fn git_error_message_with_detail(error_message: &str, stderr: &str) -> String {
	if error_message.is_empty() {
		return stderr.to_string();
	}
	if stderr.is_empty() {
		return error_message.to_string();
	}

	let mut detail = String::with_capacity(error_message.len() + 2 + stderr.len());
	detail.push_str(error_message);
	detail.push_str(": ");
	detail.push_str(stderr);
	detail
}

fn handle_git_process_output(
	output: &std::process::Output,
	error_message: &str,
) -> MonochangeResult<()> {
	if !output.status.success() {
		let stderr = git_error_detail(output);
		return Err(MonochangeError::Discovery(git_error_message_with_detail(
			error_message,
			&stderr,
		)));
	}
	Ok(())
}

#[cfg(test)]
#[path = "__tests__/git_support_tests.rs"]
mod tests;
