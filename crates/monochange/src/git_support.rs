use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use monochange_core::CommitMessage;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::git::git_command_output;
use monochange_core::git::git_commit_paths_command;
use monochange_core::git::git_error_detail;
use monochange_core::git::git_stage_paths_command;
use monochange_core::git::git_stderr_trimmed;
use monochange_core::git::git_stdout_trimmed;

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
pub(crate) fn git_commit_paths(root: &Path, message: &CommitMessage) -> MonochangeResult<()> {
	run_git_process(
		git_commit_paths_command(root, message),
		"failed to create release commit",
	)
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

#[cfg(test)]
mod tests {
	use std::fs;

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

	#[test]
	fn git_stage_paths_returns_ok_when_all_paths_are_non_stageable() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		fs::write(root.join(".gitignore"), ".monochange/\n")
			.unwrap_or_else(|error| panic!("write .gitignore: {error}"));
		fs::write(root.join("tracked.txt"), "tracked\n")
			.unwrap_or_else(|error| panic!("write tracked file: {error}"));
		init_git_repo(root);
		git(root, &["add", "."]);
		git(root, &["commit", "-m", "initial"]);
		fs::create_dir_all(root.join(".monochange"))
			.unwrap_or_else(|error| panic!("create .monochange: {error}"));
		fs::write(root.join(".monochange/release-manifest.json"), "{}\n")
			.unwrap_or_else(|error| panic!("write release manifest: {error}"));

		git_stage_paths(
			root,
			&[
				PathBuf::from(".monochange/release-manifest.json"),
				PathBuf::from(".changeset/missing.md"),
			],
		)
		.unwrap_or_else(|error| panic!("git stage paths: {error}"));

		assert_eq!(git_output(root, &["diff", "--cached", "--name-only"]), "");
	}

	#[test]
	fn git_path_is_tracked_reports_command_failures() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path().to_path_buf();
		drop(tempdir);

		let error = git_path_is_tracked(&root, Path::new("release.txt"))
			.err()
			.unwrap_or_else(|| panic!("expected tracked inspection failure"));
		assert!(
			error
				.to_string()
				.contains("failed to inspect tracked git path release.txt")
		);
	}

	#[test]
	fn git_path_is_ignored_reports_false_for_unignored_paths() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		init_git_repo(root);
		fs::write(root.join("release.txt"), "release\n")
			.unwrap_or_else(|error| panic!("write release file: {error}"));

		assert!(
			!git_path_is_ignored(root, Path::new("release.txt"))
				.unwrap_or_else(|error| panic!("git path ignored: {error}"))
		);
	}

	#[test]
	fn git_path_is_ignored_reports_inspection_failures() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		fs::write(root.join("release.txt"), "release\n")
			.unwrap_or_else(|error| panic!("write release file: {error}"));

		let error = git_path_is_ignored(root, Path::new("release.txt"))
			.err()
			.unwrap_or_else(|| panic!("expected ignored inspection failure"));
		assert!(
			error
				.to_string()
				.contains("failed to inspect ignored git path release.txt")
		);
	}

	#[test]
	fn git_path_is_ignored_reports_command_failures() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path().to_path_buf();
		drop(tempdir);

		let error = git_path_is_ignored(&root, Path::new("release.txt"))
			.err()
			.unwrap_or_else(|| panic!("expected ignored command failure"));
		assert!(
			error
				.to_string()
				.contains("failed to inspect ignored git path release.txt")
		);
	}

	#[test]
	fn run_git_capture_includes_stderr_for_failed_commands() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		init_git_repo(root);

		let error = run_git_capture(root, &["show", "missing-commit"], "capture failure")
			.err()
			.unwrap_or_else(|| panic!("expected failed git capture"));
		assert!(error.to_string().contains("capture failure"));
		assert!(error.to_string().contains("missing-commit"));
	}

	#[test]
	fn run_git_process_reports_nonzero_exit_status_details() {
		let mut command = ProcessCommand::new("git");
		command.arg("definitely-not-a-real-git-command");

		let error = run_git_process(command, "process failure")
			.err()
			.unwrap_or_else(|| panic!("expected failed git process"));
		assert!(error.to_string().contains("process failure"));
		assert!(
			error
				.to_string()
				.contains("definitely-not-a-real-git-command")
		);
	}
}
