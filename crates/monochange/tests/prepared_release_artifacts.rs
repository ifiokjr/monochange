use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;

mod test_support;
use test_support::setup_scenario_workspace;

fn run_cli<I>(root: &Path, args: I) -> monochange_core::MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	monochange::run_with_args_in_dir("mc", args, root)
}

fn run_json<I>(root: &Path, args: I) -> serde_json::Value
where
	I: IntoIterator<Item = OsString>,
{
	let output = run_cli(root, args).unwrap_or_else(|error| panic!("command output: {error}"));
	serde_json::from_str(&output).unwrap_or_else(|error| panic!("parse json: {error}"))
}

fn git(root: &Path, args: &[&str]) {
	let output = git_command(root, args)
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
	let output = git_command(root, args)
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

fn git_command(root: &Path, args: &[&str]) -> Command {
	let mut command = Command::new("git");
	command.current_dir(root).args(args);
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

fn init_git_repo(root: &Path) {
	git(root, &["init", "-b", "main"]);
	git(root, &["config", "user.name", "monochange tests"]);
	git(root, &["config", "user.email", "monochange@example.com"]);
	git(root, &["add", "."]);
	git(
		root,
		&["-c", "commit.gpgsign=false", "commit", "-m", "initial"],
	);
}

fn command_args(args: &[&str]) -> Vec<OsString> {
	std::iter::once(OsString::from("mc"))
		.chain(args.iter().map(|value| OsString::from(*value)))
		.collect()
}

#[test]
fn explicit_prepared_release_artifact_drives_follow_up_release_pr() {
	let tempdir = setup_scenario_workspace("prepared-release/source-github-follow-up");
	let root = tempdir.path();
	let artifact_path = root.join(".monochange/custom-prepared-release.json");
	let artifact = artifact_path.display().to_string();
	init_git_repo(root);

	run_cli(
		root,
		command_args(&[
			"release",
			"--format",
			"json",
			"--prepared-release",
			&artifact,
		]),
	)
	.unwrap_or_else(|error| panic!("release output: {error}"));

	assert!(artifact_path.is_file());
	assert!(!root.join(".changeset/feature.md").exists());

	let value = run_json(
		root,
		command_args(&[
			"release-pr",
			"--dry-run",
			"--format",
			"json",
			"--prepared-release",
			&artifact,
		]),
	);
	assert_eq!(
		value.pointer("/releaseRequest/provider"),
		Some(&serde_json::Value::String("github".to_string()))
	);
	assert_eq!(
		value.pointer("/releaseRequest/headBranch"),
		Some(&serde_json::Value::String(
			"monochange/release/release-pr".to_string()
		))
	);
}

#[test]
fn automatic_prepared_release_cache_survives_commit_and_release_pr_follow_ups() {
	let tempdir = setup_scenario_workspace("prepared-release/source-github-follow-up");
	let root = tempdir.path();
	init_git_repo(root);

	run_cli(root, command_args(&["release", "--format", "json"]))
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		root.join(".monochange/prepared-release-cache.json")
			.is_file()
	);
	assert!(!root.join(".changeset/feature.md").exists());

	run_cli(root, command_args(&["commit-release", "--format", "json"]))
		.unwrap_or_else(|error| panic!("commit-release output: {error}"));
	assert_eq!(git_output(root, &["status", "--short"]), "");
	assert_eq!(
		git_output(root, &["log", "-1", "--pretty=%s"]),
		"chore(release): prepare release"
	);

	let value = run_json(
		root,
		command_args(&["release-pr", "--dry-run", "--format", "json"]),
	);
	assert_eq!(
		value.pointer("/releaseRequest/title"),
		Some(&serde_json::Value::String(
			"chore(release): prepare release".to_string()
		))
	);
	assert_eq!(git_output(root, &["status", "--short"]), "");
}

#[test]
fn commit_release_can_reuse_saved_prepared_release_without_prepare_step() {
	let tempdir = setup_scenario_workspace("prepared-release/commit-release-flexible");
	let root = tempdir.path();
	init_git_repo(root);

	run_cli(root, command_args(&["release", "--format", "json"]))
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		root.join(".monochange/prepared-release-cache.json")
			.is_file()
	);

	run_cli(
		root,
		command_args(&["commit-from-cache", "--format", "json"]),
	)
	.unwrap_or_else(|error| panic!("commit-from-cache output: {error}"));
	assert_eq!(git_output(root, &["status", "--short"]), "");
	assert_eq!(
		git_output(root, &["log", "-1", "--pretty=%s"]),
		"chore(release): prepare release"
	);
}

#[test]
fn commit_release_succeeds_when_manifest_is_gitignored() {
	let tempdir = setup_scenario_workspace("prepared-release/commit-release-flexible");
	let root = tempdir.path();
	init_git_repo(root);

	run_cli(
		root,
		command_args(&["release-with-manifest", "--format", "json"]),
	)
	.unwrap_or_else(|error| panic!("release-with-manifest output: {error}"));
	assert!(root.join(".monochange/release-manifest.json").is_file());
	assert_eq!(git_output(root, &["status", "--short"]), "");
	assert_eq!(
		git_output(root, &["log", "-1", "--pretty=%s"]),
		"chore(release): prepare release"
	);
}

#[test]
fn prepared_release_artifact_rejects_workspace_content_drift() {
	let tempdir = setup_scenario_workspace("prepared-release/source-github-follow-up");
	let root = tempdir.path();
	let artifact_path = root.join(".monochange/custom-prepared-release.json");
	let artifact = artifact_path.display().to_string();
	init_git_repo(root);

	run_cli(
		root,
		command_args(&[
			"release",
			"--format",
			"json",
			"--prepared-release",
			&artifact,
		]),
	)
	.unwrap_or_else(|error| panic!("release output: {error}"));
	fs::write(
		root.join("crates/core/CHANGELOG.md"),
		"# Changelog\n\nmanual drift\n",
	)
	.unwrap_or_else(|error| panic!("write changelog drift: {error}"));

	let error = run_cli(
		root,
		command_args(&[
			"release-pr",
			"--dry-run",
			"--format",
			"json",
			"--prepared-release",
			&artifact,
		]),
	)
	.unwrap_err();
	let message = error.to_string();
	assert!(message.contains("prepared release artifact"));
	assert!(message.contains("workspace"));
}
