use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::WorkspaceConfiguration;
use monochange_core::git::git_command_output;
use monochange_core::git::git_error_detail;
use monochange_core::git::git_head_commit;
use serde::Deserialize;
use serde::Serialize;
use similar::TextDiff;

use crate::PreparedFileDiff;
use crate::PreparedRelease;
use crate::PreparedReleaseExecution;
use crate::StepPhaseTiming;
use crate::resolve_config_path;
use crate::root_relative;

const PREPARED_RELEASE_ARTIFACT_SCHEMA_VERSION: u32 = 1;
const DEFAULT_PREPARED_RELEASE_CACHE_PATH: &str = ".monochange/prepared-release-cache.json";

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct LoadedPreparedReleaseExecution {
	pub(crate) execution: PreparedReleaseExecution,
	pub(crate) message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreparedReleaseArtifact {
	schema_version: u32,
	configuration_snapshot: String,
	head_commit: String,
	worktree_status: Vec<String>,
	tracked_paths: Vec<PreparedReleaseTrackedPath>,
	prepared_release: PreparedRelease,
	#[serde(default)]
	file_diffs: Vec<PersistedPreparedFileDiff>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreparedReleaseTrackedPath {
	path: PathBuf,
	state: PreparedReleaseTrackedPathState,
	hash: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PreparedReleaseTrackedPathState {
	File,
	Deleted,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedPreparedFileDiff {
	path: PathBuf,
	diff: String,
	display_diff: String,
}

pub(crate) fn default_prepared_release_cache_path(root: &Path) -> PathBuf {
	root.join(DEFAULT_PREPARED_RELEASE_CACHE_PATH)
}

pub(crate) fn resolve_prepared_release_artifact_path(
	root: &Path,
	explicit_path: Option<&Path>,
) -> PathBuf {
	explicit_path.map_or_else(
		|| default_prepared_release_cache_path(root),
		|path| resolve_config_path(root, path),
	)
}

pub(crate) fn load_prepared_release_execution(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	explicit_path: Option<&Path>,
	current_dry_run: bool,
	build_file_diffs: bool,
) -> MonochangeResult<Option<LoadedPreparedReleaseExecution>> {
	let artifact_path = resolve_prepared_release_artifact_path(root, explicit_path);
	if !artifact_path.exists() {
		return Ok(None);
	}

	let artifact = read_prepared_release_artifact(&artifact_path)?;
	validate_prepared_release_artifact(
		root,
		configuration,
		&artifact_path,
		&artifact,
		current_dry_run,
		build_file_diffs,
	)?;

	let load_started_at = Instant::now();
	let file_diffs = artifact
		.file_diffs
		.into_iter()
		.map(|file_diff| {
			PreparedFileDiff {
				path: file_diff.path,
				diff: file_diff.diff,
				display_diff: file_diff.display_diff,
			}
		})
		.collect();
	let execution = PreparedReleaseExecution {
		prepared_release: artifact.prepared_release,
		file_diffs,
		phase_timings: vec![StepPhaseTiming {
			label: "load prepared release artifact".to_string(),
			duration: load_started_at.elapsed(),
		}],
	};
	let message = format!(
		"reused prepared release artifact `{}`",
		root_relative(root, &artifact_path).display()
	);
	Ok(Some(LoadedPreparedReleaseExecution { execution, message }))
}

pub(crate) fn maybe_load_prepared_release_execution(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	explicit_path: Option<&Path>,
	current_dry_run: bool,
	build_file_diffs: bool,
) -> MonochangeResult<Option<LoadedPreparedReleaseExecution>> {
	match load_prepared_release_execution(
		root,
		configuration,
		explicit_path,
		current_dry_run,
		build_file_diffs,
	) {
		Ok(Some(loaded)) => Ok(Some(loaded)),
		Ok(None) => Ok(None),
		Err(error) if explicit_path.is_none() => {
			tracing::warn!(%error, "ignoring stale prepared release artifact");
			Ok(None)
		}
		Err(error) => Err(error),
	}
}

pub(crate) fn save_prepared_release_execution(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: &PreparedRelease,
	file_diffs: &[PreparedFileDiff],
	explicit_path: Option<&Path>,
) -> MonochangeResult<()> {
	let artifact_path = resolve_prepared_release_artifact_path(root, explicit_path);
	ensure_monochange_artifact_ignored(root, &artifact_path)?;
	let parent = artifact_path.parent().map(Path::to_path_buf);
	if let Some(parent) = parent {
		fs::create_dir_all(&parent).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to create prepared release artifact directory {}: {error}",
				parent.display()
			))
		})?;
	}

	let artifact = PreparedReleaseArtifact {
		schema_version: PREPARED_RELEASE_ARTIFACT_SCHEMA_VERSION,
		configuration_snapshot: configuration_snapshot(configuration)?,
		head_commit: git_head_commit(root)?,
		worktree_status: git_status_snapshot(root, Some(&artifact_path))?,
		tracked_paths: tracked_path_snapshots(root, prepared_release)?,
		prepared_release: prepared_release.clone(),
		file_diffs: file_diffs
			.iter()
			.map(|file_diff| {
				PersistedPreparedFileDiff {
					path: file_diff.path.clone(),
					diff: file_diff.diff.clone(),
					display_diff: file_diff.display_diff.clone(),
				}
			})
			.collect(),
	};
	let rendered = serde_json::to_string_pretty(&artifact).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to serialize prepared release artifact: {error}"
		))
	})?;
	fs::write(&artifact_path, rendered).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write prepared release artifact {}: {error}",
			artifact_path.display()
		))
	})
}

fn read_prepared_release_artifact(path: &Path) -> MonochangeResult<PreparedReleaseArtifact> {
	let contents = fs::read_to_string(path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read prepared release artifact {}: {error}",
			path.display()
		))
	})?;
	serde_json::from_str(&contents).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse prepared release artifact {}: {error}",
			path.display()
		))
	})
}

fn validate_prepared_release_artifact(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	artifact_path: &Path,
	artifact: &PreparedReleaseArtifact,
	current_dry_run: bool,
	build_file_diffs: bool,
) -> MonochangeResult<()> {
	if artifact.schema_version != PREPARED_RELEASE_ARTIFACT_SCHEMA_VERSION {
		return Err(stale_artifact_error(
			artifact_path,
			format!(
				"schema version {} does not match supported version {}",
				artifact.schema_version, PREPARED_RELEASE_ARTIFACT_SCHEMA_VERSION
			),
		));
	}

	if !current_dry_run && artifact.prepared_release.dry_run {
		return Err(stale_artifact_error(
			artifact_path,
			"dry-run artifacts cannot drive non-dry-run follow-up commands",
		));
	}

	if configuration_snapshot(configuration)? != artifact.configuration_snapshot {
		return Err(stale_artifact_error(
			artifact_path,
			"workspace configuration changed",
		));
	}

	let current_head = git_head_commit(root)?;
	if current_head != artifact.head_commit {
		return Err(stale_artifact_error(
			artifact_path,
			format!(
				"HEAD changed from {} to {}",
				artifact.head_commit, current_head
			),
		));
	}

	let current_status = git_status_snapshot(root, Some(artifact_path))?;
	if current_status != artifact.worktree_status {
		return Err(stale_artifact_error(
			artifact_path,
			"workspace status no longer matches the saved prepared release",
		));
	}

	let current_tracked_paths = tracked_path_snapshots(root, &artifact.prepared_release)?;
	if current_tracked_paths != artifact.tracked_paths {
		return Err(stale_artifact_error(
			artifact_path,
			"workspace content drifted from the saved prepared release",
		));
	}

	if build_file_diffs && artifact.file_diffs.is_empty() {
		return Err(stale_artifact_error(
			artifact_path,
			"diff previews were not captured in the saved prepared release",
		));
	}

	Ok(())
}

fn stale_artifact_error(path: &Path, detail: impl AsRef<str>) -> MonochangeError {
	MonochangeError::Config(format!(
		"prepared release artifact `{}` is stale: {}",
		path.display(),
		detail.as_ref()
	))
}

fn configuration_snapshot(configuration: &WorkspaceConfiguration) -> MonochangeResult<String> {
	serde_json::to_string(configuration).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to serialize workspace configuration for prepared release caching: {error}"
		))
	})
}

fn git_status_snapshot(root: &Path, excluded_path: Option<&Path>) -> MonochangeResult<Vec<String>> {
	let output = git_command_output(
		root,
		&["status", "--short", "--untracked-files=all", "--porcelain"],
	)
	.map_err(|error| MonochangeError::Io(format!("failed to read git status: {error}")))?;
	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"failed to read git status: {}",
			git_error_detail(&output)
		)));
	}

	let excluded = excluded_path.map(|path| root_relative(root, path));
	let mut lines = String::from_utf8_lossy(&output.stdout)
		.lines()
		.map(str::to_string)
		.filter(|line| {
			let Some(excluded) = &excluded else {
				return true;
			};
			status_line_path(line).is_none_or(|path| path != *excluded)
		})
		.collect::<Vec<_>>();
	lines.sort();
	Ok(lines)
}

fn status_line_path(line: &str) -> Option<PathBuf> {
	if line.len() < 4 {
		return None;
	}
	Some(PathBuf::from(line[3..].trim()))
}

fn tracked_path_snapshots(
	root: &Path,
	prepared_release: &PreparedRelease,
) -> MonochangeResult<Vec<PreparedReleaseTrackedPath>> {
	let mut tracked_paths = prepared_release.changed_files.clone();
	tracked_paths.extend(prepared_release.deleted_changesets.clone());
	tracked_paths.sort();
	tracked_paths.dedup();

	tracked_paths
		.into_iter()
		.map(|path| {
			let absolute_path = resolve_config_path(root, &path);
			if absolute_path.exists() {
				Ok(PreparedReleaseTrackedPath {
					path,
					state: PreparedReleaseTrackedPathState::File,
					hash: Some(hash_file_at_path(root, &absolute_path)?),
				})
			} else {
				Ok(PreparedReleaseTrackedPath {
					path,
					state: PreparedReleaseTrackedPathState::Deleted,
					hash: None,
				})
			}
		})
		.collect()
}

fn hash_file_at_path(root: &Path, path: &Path) -> MonochangeResult<String> {
	let relative_path = root_relative(root, path);
	let relative = relative_path.to_string_lossy().into_owned();
	let output = git_command_output(root, &["hash-object", "--", &relative]).map_err(|error| {
		MonochangeError::Io(format!("failed to hash {}: {error}", path.display()))
	})?;
	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"failed to hash {}: {}",
			path.display(),
			git_error_detail(&output)
		)));
	}
	Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(crate) fn ensure_monochange_artifact_ignored(
	root: &Path,
	artifact_path: &Path,
) -> MonochangeResult<()> {
	let monochange_dir = root.join(".monochange");
	if !artifact_path.starts_with(&monochange_dir) {
		return Ok(());
	}

	let output = git_command_output(root, &["rev-parse", "--git-path", "info/exclude"]).map_err(
		|error| MonochangeError::Io(format!("failed to resolve git exclude path: {error}")),
	)?;
	if !output.status.success() {
		// Non-git workspaces can still use the artifact; the ignore rule is only
		// needed to keep git status clean when a repository exists.
		return Ok(());
	}

	let exclude_path = {
		let raw_path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
		if raw_path.is_absolute() {
			raw_path
		} else {
			root.join(raw_path)
		}
	};
	if let Some(parent) = exclude_path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to create git exclude directory {}: {error}",
				parent.display()
			))
		})?;
	}
	let existing = fs::read_to_string(&exclude_path).unwrap_or_default();
	if existing.lines().any(|line| line.trim() == ".monochange/") {
		return Ok(());
	}

	let mut updated = existing;
	if !updated.is_empty() && !updated.ends_with('\n') {
		updated.push('\n');
	}
	updated.push_str(".monochange/\n");
	fs::write(&exclude_path, updated).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to update git exclude file {}: {error}",
			exclude_path.display()
		))
	})
}

#[allow(dead_code)]
fn render_unified_file_diff(path: &Path, before: &[u8], after: &[u8]) -> String {
	let before_text = String::from_utf8_lossy(before);
	let after_text = String::from_utf8_lossy(after);
	let context_radius = before_text.lines().count().max(after_text.lines().count());
	let diff = TextDiff::from_lines(before_text.as_ref(), after_text.as_ref());
	let mut unified = diff.unified_diff();
	unified.context_radius(context_radius).header(
		&format!("a/{}", path.display()),
		&format!("b/{}", path.display()),
	);
	unified.to_string().trim_end_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
	use std::process::Command;

	use monochange_config::load_workspace_configuration;
	use monochange_test_helpers::fs::setup_scenario_workspace_from;
	use tempfile::TempDir;

	use super::*;

	fn setup_prepared_release_repo() -> TempDir {
		let tempdir = setup_scenario_workspace_from(
			env!("CARGO_MANIFEST_DIR"),
			"prepared-release/source-github-follow-up",
		);
		let root = tempdir.path();
		git(root, &["init", "-b", "main"]);
		git(root, &["config", "user.name", "monochange tests"]);
		git(root, &["config", "user.email", "monochange@example.com"]);
		git(root, &["add", "."]);
		git(
			root,
			&["-c", "commit.gpgsign=false", "commit", "-m", "initial"],
		);
		tempdir
	}

	fn git(root: &Path, args: &[&str]) {
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
		let output = command
			.output()
			.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
		assert!(
			output.status.success(),
			"git {args:?} failed: {}{}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
	}

	fn explicit_artifact_path(root: &Path) -> PathBuf {
		root.join(".monochange/unit-prepared-release.json")
	}

	fn save_artifact(root: &Path, dry_run: bool, explicit_path: &Path) -> WorkspaceConfiguration {
		let configuration = load_workspace_configuration(root)
			.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
		let prepared = crate::prepare_release_execution_with_file_diffs(root, dry_run, false)
			.unwrap_or_else(|error| panic!("prepare release execution: {error}"));
		save_prepared_release_execution(
			root,
			&configuration,
			&prepared.prepared_release,
			&prepared.file_diffs,
			Some(explicit_path),
		)
		.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));
		configuration
	}

	#[test]
	fn load_prepared_release_execution_rejects_non_dry_run_follow_up_from_dry_run_artifact() {
		let tempdir = setup_prepared_release_repo();
		let root = tempdir.path();
		let artifact_path = explicit_artifact_path(root);
		let configuration = save_artifact(root, true, &artifact_path);

		let error = load_prepared_release_execution(
			root,
			&configuration,
			Some(&artifact_path),
			false,
			false,
		)
		.unwrap_err();
		assert!(error.to_string().contains("dry-run artifacts"));
	}

	#[test]
	fn load_prepared_release_execution_rejects_configuration_drift() {
		let tempdir = setup_prepared_release_repo();
		let root = tempdir.path();
		let artifact_path = explicit_artifact_path(root);
		let _original_configuration = save_artifact(root, true, &artifact_path);
		fs::write(
			root.join("monochange.toml"),
			fs::read_to_string(root.join("monochange.toml"))
				.unwrap_or_else(|error| panic!("read monochange.toml: {error}"))
				.replacen("repo = \"monochange\"", "repo = \"monochange-next\"", 1),
		)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
		let changed_configuration = load_workspace_configuration(root)
			.unwrap_or_else(|error| panic!("reload workspace configuration: {error}"));

		let error = load_prepared_release_execution(
			root,
			&changed_configuration,
			Some(&artifact_path),
			true,
			false,
		)
		.unwrap_err();
		assert!(
			error
				.to_string()
				.contains("workspace configuration changed")
		);
	}

	#[test]
	fn load_prepared_release_execution_rejects_schema_mismatch() {
		let tempdir = setup_prepared_release_repo();
		let root = tempdir.path();
		let artifact_path = explicit_artifact_path(root);
		let configuration = save_artifact(root, true, &artifact_path);
		let mut artifact = read_prepared_release_artifact(&artifact_path)
			.unwrap_or_else(|error| panic!("read prepared release artifact: {error}"));
		artifact.schema_version += 1;
		fs::write(
			&artifact_path,
			serde_json::to_string_pretty(&artifact)
				.unwrap_or_else(|error| panic!("serialize artifact: {error}")),
		)
		.unwrap_or_else(|error| panic!("rewrite artifact: {error}"));

		let error = load_prepared_release_execution(
			root,
			&configuration,
			Some(&artifact_path),
			true,
			false,
		)
		.unwrap_err();
		assert!(error.to_string().contains("schema version"));
	}

	#[test]
	fn maybe_load_prepared_release_execution_ignores_stale_default_cache() {
		let tempdir = setup_prepared_release_repo();
		let root = tempdir.path();
		let default_path = default_prepared_release_cache_path(root);
		let configuration = load_workspace_configuration(root)
			.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
		let prepared = crate::prepare_release_execution_with_file_diffs(root, false, false)
			.unwrap_or_else(|error| panic!("prepare release execution: {error}"));
		save_prepared_release_execution(
			root,
			&configuration,
			&prepared.prepared_release,
			&prepared.file_diffs,
			None,
		)
		.unwrap_or_else(|error| panic!("save default prepared release artifact: {error}"));
		assert!(default_path.is_file());
		fs::write(
			root.join("crates/core/CHANGELOG.md"),
			"# Changelog\n\nautomatic drift\n",
		)
		.unwrap_or_else(|error| panic!("write drifted changelog: {error}"));

		let loaded =
			maybe_load_prepared_release_execution(root, &configuration, None, false, false)
				.unwrap_or_else(|error| panic!("maybe load prepared release execution: {error}"));
		assert!(loaded.is_none());
	}

	#[test]
	fn load_prepared_release_execution_rejects_missing_diff_previews_when_requested() {
		let tempdir = setup_prepared_release_repo();
		let root = tempdir.path();
		let artifact_path = explicit_artifact_path(root);
		let configuration = save_artifact(root, false, &artifact_path);

		let error = load_prepared_release_execution(
			root,
			&configuration,
			Some(&artifact_path),
			false,
			true,
		)
		.unwrap_err();
		assert!(error.to_string().contains("diff previews"));
	}
}
