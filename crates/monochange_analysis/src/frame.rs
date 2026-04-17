//! Change frame detection and management.
//!
//! A "change frame" defines the boundaries of what changes to analyze:
//! working directory vs last commit, branch vs main, PR vs target, etc.

use std::env;
use std::fmt;
use std::path::Path;
use std::process::Command;

use monochange_core::MonochangeError;
use serde::Deserialize;
use serde::Serialize;

/// Error type for frame detection failures.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
	/// Git operation failed
	#[error("git error: {0}")]
	Git(String),

	/// Environment detection failed
	#[error("environment error: {0}")]
	Environment(String),

	/// Invalid frame specification
	#[error("invalid frame: {0}")]
	InvalidFrame(String),
}

impl From<FrameError> for MonochangeError {
	fn from(e: FrameError) -> Self {
		MonochangeError::Discovery(e.to_string())
	}
}

/// The context for analyzing changes.
///
/// Determines what "base" and "head" mean for diff analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeFrame {
	/// Working directory changes vs HEAD.
	///
	/// Includes both staged and unstaged changes.
	WorkingDirectory,

	/// Branch comparison: base..head
	BranchRange {
		/// Base branch (e.g., "main")
		base: String,
		/// Head branch (e.g., "feature-branch")
		head: String,
	},

	/// PR comparison: `target..pr_branch`
	PullRequest {
		/// Target branch (e.g., "main")
		target: String,
		/// PR source branch
		pr_branch: String,
	},

	/// Staged changes only (for pre-commit hooks).
	StagedOnly,

	/// Custom revision range.
	CustomRange {
		/// Base revision
		base: String,
		/// Head revision
		head: String,
	},
}

impl fmt::Display for ChangeFrame {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::WorkingDirectory => write!(f, "working directory"),
			Self::BranchRange { base, head } | Self::CustomRange { base, head } => {
				write!(f, "{base}...{head}")
			}
			Self::PullRequest { target, pr_branch } => {
				write!(f, "PR: {target} <- {pr_branch}")
			}
			Self::StagedOnly => write!(f, "staged only"),
		}
	}
}

impl ChangeFrame {
	/// Detect the appropriate frame based on git state and environment.
	///
	/// Detection priority:
	/// 1. CI/CD environment variables (PR detection)
	/// 2. Branch vs default branch
	/// 3. Working directory changes
	///
	/// # Errors
	///
	/// Returns an error if git state cannot be determined.
	pub fn detect(repo_root: &Path) -> Result<Self, FrameError> {
		// Check for PR environment variables first.
		if let Some(pr_info) = detect_pr_environment(repo_root) {
			return Ok(Self::PullRequest {
				target: pr_info.target_branch,
				pr_branch: pr_info.source_branch,
			});
		}

		// Get current branch
		let current = get_current_branch(repo_root)?;
		let default = get_default_branch(repo_root).unwrap_or_else(|_| "main".to_string());

		// If we're not on the default branch, compare against it
		if current != default {
			// Find merge base for accurate comparison
			let merge_base = get_merge_base(repo_root, &default, &current)?;

			return Ok(Self::BranchRange {
				base: merge_base,
				head: current,
			});
		}

		// Default to working directory
		Ok(Self::WorkingDirectory)
	}

	/// Get the git revision range for diff commands.
	///
	/// Returns a string suitable for `git diff` or `git log` commands.
	#[must_use]
	pub fn revision_range(&self) -> String {
		match self {
			Self::WorkingDirectory => "HEAD".to_string(),
			Self::BranchRange { base, head } | Self::CustomRange { base, head } => {
				format!("{base}...{head}")
			}
			Self::PullRequest { target, pr_branch } => format!("{target}...{pr_branch}"),
			Self::StagedOnly => "--staged".to_string(),
		}
	}

	/// Get the base revision for comparison.
	#[must_use]
	pub fn base_revision(&self) -> Option<&str> {
		match self {
			Self::WorkingDirectory | Self::StagedOnly => Some("HEAD"),
			Self::BranchRange { base, .. } | Self::CustomRange { base, .. } => Some(base.as_str()),
			Self::PullRequest { target, .. } => Some(target.as_str()),
		}
	}

	/// Get the head revision for comparison.
	#[must_use]
	pub fn head_revision(&self) -> Option<&str> {
		match self {
			Self::WorkingDirectory => None, // Working directory has no revision
			Self::BranchRange { head, .. } | Self::CustomRange { head, .. } => Some(head.as_str()),
			Self::PullRequest { pr_branch, .. } => Some(pr_branch.as_str()),
			Self::StagedOnly => Some("HEAD"),
		}
	}

	/// Check if this frame includes unstaged changes.
	#[must_use]
	pub fn includes_unstaged(&self) -> bool {
		matches!(self, Self::WorkingDirectory | Self::BranchRange { .. })
	}

	/// Check if this frame includes staged changes.
	#[must_use]
	pub fn includes_staged(&self) -> bool {
		matches!(
			self,
			Self::WorkingDirectory | Self::StagedOnly | Self::BranchRange { .. }
		)
	}

	/// Get the list of changed files for this frame.
	///
	/// # Errors
	///
	/// Returns an error if git commands fail.
	pub fn changed_files(&self, repo_root: &Path) -> Result<Vec<std::path::PathBuf>, FrameError> {
		let output = match self {
			Self::WorkingDirectory => {
				// Get both staged and unstaged
				let mut staged =
					run_git_diff_name_only(repo_root, &["--staged", "--diff-filter=ACMRT"])?;
				let unstaged = run_git_diff_name_only(repo_root, &["HEAD", "--diff-filter=ACMRT"])?;

				staged.extend(unstaged);
				staged.sort();
				staged.dedup();
				return Ok(staged);
			}
			Self::StagedOnly => {
				run_git_diff_name_only(repo_root, &["--staged", "--diff-filter=ACMRT"])?
			}
			_ => {
				let range = self.revision_range();
				run_git_diff_name_only(repo_root, &[&range, "--diff-filter=ACMRT"])?
			}
		};

		Ok(output)
	}
}

/// Information about a PR environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrEnvironment {
	/// Source branch of the PR
	pub source_branch: String,
	/// Target branch of the PR
	pub target_branch: String,
	/// PR number if available
	pub pr_number: Option<String>,
	/// Provider (GitHub, GitLab, etc.)
	pub provider: String,
}

/// Detect PR environment from common CI/CD variables.
fn detect_pr_environment(repo_root: &Path) -> Option<PrEnvironment> {
	let pr_info = detect_raw_pr_environment()?;
	let target_branch = resolve_pr_target_branch(repo_root, &pr_info.target_branch)?;
	let source_branch = resolve_pr_source_branch(repo_root, &pr_info.source_branch)?;

	Some(PrEnvironment {
		source_branch,
		target_branch,
		pr_number: pr_info.pr_number,
		provider: pr_info.provider,
	})
}

fn detect_raw_pr_environment() -> Option<PrEnvironment> {
	// GitHub Actions
	if let Ok(event_name) = env::var("GITHUB_EVENT_NAME")
		&& (event_name == "pull_request" || event_name == "pull_request_target")
	{
		return Some(PrEnvironment {
			source_branch: env::var("GITHUB_HEAD_REF").ok()?,
			target_branch: env::var("GITHUB_BASE_REF").ok()?,
			pr_number: env::var("GITHUB_EVENT_NUMBER").ok(),
			provider: "github".to_string(),
		});
	}

	// GitLab CI
	if env::var("GITLAB_CI").is_ok() {
		let source = env::var("CI_MERGE_REQUEST_SOURCE_BRANCH_NAME")
			.or_else(|_| env::var("CI_COMMIT_REF_NAME"))
			.ok()?;
		let target = env::var("CI_MERGE_REQUEST_TARGET_BRANCH_NAME")
			.or_else(|_| env::var("CI_DEFAULT_BRANCH"))
			.ok()?;

		return Some(PrEnvironment {
			source_branch: source,
			target_branch: target,
			pr_number: env::var("CI_MERGE_REQUEST_IID").ok(),
			provider: "gitlab".to_string(),
		});
	}

	// CircleCI
	if env::var("CIRCLECI").is_ok() {
		return Some(PrEnvironment {
			source_branch: env::var("CIRCLE_BRANCH").ok()?,
			target_branch: env::var("CIRCLE_TARGET_BRANCH")
				.or_else(|_| env::var("CIRCLE_DEFAULT_BRANCH"))
				.unwrap_or_else(|_| "main".to_string()),
			pr_number: env::var("CIRCLE_PR_NUMBER").ok(),
			provider: "circleci".to_string(),
		});
	}

	// Travis CI
	if env::var("TRAVIS").is_ok() {
		let pr = env::var("TRAVIS_PULL_REQUEST").ok();
		let is_pr = pr.as_deref() != Some("false");

		if is_pr {
			return Some(PrEnvironment {
				source_branch: env::var("TRAVIS_PULL_REQUEST_BRANCH").ok()?,
				target_branch: env::var("TRAVIS_BRANCH").ok()?,
				pr_number: pr,
				provider: "travis".to_string(),
			});
		}
	}

	// Azure Pipelines
	if env::var("TF_BUILD").is_ok() {
		let reason = env::var("BUILD_REASON").ok()?;
		if reason == "PullRequest" {
			return Some(PrEnvironment {
				source_branch: env::var("SYSTEM_PULLREQUEST_SOURCEBRANCH")
					.ok()?
					.trim_start_matches("refs/heads/")
					.to_string(),
				target_branch: env::var("SYSTEM_PULLREQUEST_TARGETBRANCH")
					.ok()?
					.trim_start_matches("refs/heads/")
					.to_string(),
				pr_number: env::var("SYSTEM_PULLREQUEST_PULLREQUESTNUMBER").ok(),
				provider: "azure".to_string(),
			});
		}
	}

	// Buildkite
	if env::var("BUILDKITE").is_ok() {
		return Some(PrEnvironment {
			source_branch: env::var("BUILDKITE_BRANCH").ok()?,
			target_branch: env::var("BUILDKITE_PULL_REQUEST_BASE_BRANCH")
				.or_else(|_| env::var("BUILDKITE_DEFAULT_BRANCH"))
				.unwrap_or_else(|_| "main".to_string()),
			pr_number: env::var("BUILDKITE_PULL_REQUEST").ok(),
			provider: "buildkite".to_string(),
		});
	}

	None
}

fn resolve_pr_target_branch(repo_root: &Path, branch: &str) -> Option<String> {
	resolve_revision_alias(repo_root, &[branch.to_string(), format!("origin/{branch}")])
}

fn resolve_pr_source_branch(repo_root: &Path, branch: &str) -> Option<String> {
	let mut candidates = vec![branch.to_string(), format!("origin/{branch}")];

	if is_detached_head(repo_root) {
		candidates.push("HEAD".to_string());
	}

	resolve_revision_alias(repo_root, &candidates)
}

fn resolve_revision_alias(repo_root: &Path, candidates: &[String]) -> Option<String> {
	for candidate in candidates {
		if revision_exists(repo_root, candidate) {
			return Some(candidate.clone());
		}
	}

	None
}

fn revision_exists(repo_root: &Path, revision: &str) -> bool {
	let Ok(output) = Command::new("git")
		.current_dir(repo_root)
		.args(["rev-parse", "--verify", revision])
		.output()
	else {
		return false;
	};

	output.status.success()
}

fn is_detached_head(repo_root: &Path) -> bool {
	let Ok(output) = Command::new("git")
		.current_dir(repo_root)
		.args(["branch", "--show-current"])
		.output()
	else {
		return false;
	};

	output.status.success() && String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

/// Get the current git branch name.
fn get_current_branch(repo_root: &Path) -> Result<String, FrameError> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["branch", "--show-current"])
		.output()
		.map_err(|e| FrameError::Git(format!("failed to run git branch: {e}")))?;

	if !output.status.success() {
		return Err(FrameError::Git("git branch command failed".to_string()));
	}

	let branch = String::from_utf8(output.stdout)
		.map_err(|e| FrameError::Git(format!("invalid utf-8: {e}")))?
		.trim()
		.to_string();

	if branch.is_empty() {
		// Detached HEAD - try to get from env or use short sha
		return get_branch_from_env().or_else(|_| get_short_sha(repo_root));
	}

	Ok(branch)
}

/// Get the default branch (usually "main" or "master").
fn get_default_branch(repo_root: &Path) -> Result<String, FrameError> {
	// Try to get from git config
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["rev-parse", "--abbrev-ref", "origin/HEAD"])
		.output();

	if let Ok(output) = output
		&& output.status.success()
	{
		let branch = String::from_utf8(output.stdout)
			.map_err(|e| FrameError::Git(format!("invalid utf-8: {e}")))?
			.trim()
			.trim_start_matches("origin/")
			.to_string();

		if !branch.is_empty() {
			return Ok(branch);
		}
	}

	// Fallback: check common branch names
	for branch in ["main", "master"] {
		let output = Command::new("git")
			.current_dir(repo_root)
			.args(["rev-parse", "--verify", branch])
			.output();

		if let Ok(output) = output
			&& output.status.success()
		{
			return Ok(branch.to_string());
		}
	}

	Err(FrameError::Git(
		"could not determine default branch".to_string(),
	))
}

/// Get the merge base between two branches.
fn get_merge_base(repo_root: &Path, base: &str, head: &str) -> Result<String, FrameError> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["merge-base", base, head])
		.output()
		.map_err(|e| FrameError::Git(format!("failed to run git merge-base: {e}")))?;

	if !output.status.success() {
		// If merge-base fails, just use the base branch
		return Ok(base.to_string());
	}

	let sha = String::from_utf8(output.stdout)
		.map_err(|e| FrameError::Git(format!("invalid utf-8: {e}")))?
		.trim()
		.to_string();

	Ok(sha)
}

/// Get branch name from environment variables.
fn get_branch_from_env() -> Result<String, FrameError> {
	// Try common env vars
	for var in [
		"GITHUB_REF_NAME",
		"CI_COMMIT_REF_NAME",
		"CIRCLE_BRANCH",
		"TRAVIS_BRANCH",
		"BUILDKITE_BRANCH",
		"BITBUCKET_BRANCH",
	] {
		if let Ok(branch) = env::var(var) {
			return Ok(branch);
		}
	}

	Err(FrameError::Environment(
		"could not determine branch from environment".to_string(),
	))
}

/// Get short SHA as fallback for detached HEAD.
fn get_short_sha(repo_root: &Path) -> Result<String, FrameError> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["rev-parse", "--short", "HEAD"])
		.output()
		.map_err(|e| FrameError::Git(format!("failed to run git rev-parse: {e}")))?;

	if !output.status.success() {
		return Err(FrameError::Git("git rev-parse command failed".to_string()));
	}

	let sha = String::from_utf8(output.stdout)
		.map_err(|e| FrameError::Git(format!("invalid utf-8: {e}")))?
		.trim()
		.to_string();

	Ok(sha)
}

/// Run git diff --name-only and return list of files.
fn run_git_diff_name_only(
	repo_root: &Path,
	args: &[&str],
) -> Result<Vec<std::path::PathBuf>, FrameError> {
	let mut cmd = Command::new("git");
	cmd.current_dir(repo_root).arg("diff").arg("--name-only");
	cmd.args(args);

	let output = cmd
		.output()
		.map_err(|e| FrameError::Git(format!("failed to run git diff: {e}")))?;

	if !output.status.success() {
		return Err(FrameError::Git("git diff command failed".to_string()));
	}

	let stdout = String::from_utf8(output.stdout)
		.map_err(|e| FrameError::Git(format!("invalid utf-8: {e}")))?;

	let files: Vec<std::path::PathBuf> = stdout
		.lines()
		.filter(|line| !line.is_empty())
		.map(std::path::PathBuf::from)
		.collect();

	Ok(files)
}

#[cfg(test)]
mod tests {
	use std::fs;

	use monochange_test_helpers::git::git;
	use monochange_test_helpers::git_output_trimmed;
	use temp_env::with_vars;
	use tempfile::tempdir;

	use super::*;

	fn init_repo() -> tempfile::TempDir {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::write(tempdir.path().join("README.md"), "hello\n")
			.unwrap_or_else(|error| panic!("write fixture: {error}"));

		git(tempdir.path(), &["init"]);
		git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
		git(
			tempdir.path(),
			&["config", "user.email", "monochange-tests@example.com"],
		);
		git(tempdir.path(), &["add", "."]);
		git(tempdir.path(), &["commit", "-m", "initial"]);

		tempdir
	}

	#[test]
	fn detect_uses_github_pr_environment_when_refs_exist() {
		let tempdir = init_repo();
		git(tempdir.path(), &["branch", "feature-branch"]);

		let frame = with_vars(
			[
				("GITHUB_EVENT_NAME", Some("pull_request")),
				("GITHUB_HEAD_REF", Some("feature-branch")),
				("GITHUB_BASE_REF", Some("main")),
			],
			|| ChangeFrame::detect(tempdir.path()),
		)
		.unwrap_or_else(|error| panic!("detect frame: {error}"));

		assert_eq!(
			frame,
			ChangeFrame::PullRequest {
				target: "main".to_string(),
				pr_branch: "feature-branch".to_string(),
			}
		);
	}

	#[test]
	fn detect_ignores_unresolvable_pr_environment_for_local_repos() {
		let tempdir = init_repo();

		let frame = with_vars(
			[
				("GITHUB_EVENT_NAME", Some("pull_request")),
				("GITHUB_HEAD_REF", Some("feature-branch")),
				("GITHUB_BASE_REF", Some("main")),
			],
			|| ChangeFrame::detect(tempdir.path()),
		)
		.unwrap_or_else(|error| panic!("detect frame: {error}"));

		assert_eq!(frame, ChangeFrame::WorkingDirectory);
	}

	#[test]
	fn resolve_pr_source_branch_falls_back_to_head_for_detached_repos() {
		let tempdir = init_repo();
		let head = git_output_trimmed(tempdir.path(), &["rev-parse", "HEAD"]);
		git(tempdir.path(), &["checkout", &head]);

		assert_eq!(
			resolve_pr_source_branch(tempdir.path(), "missing-branch"),
			Some("HEAD".to_string())
		);
	}

	#[test]
	fn revision_helpers_return_false_for_missing_repositories() {
		let missing = tempdir()
			.unwrap_or_else(|error| panic!("tempdir: {error}"))
			.path()
			.join("missing");

		assert!(!revision_exists(&missing, "HEAD"));
		assert!(!is_detached_head(&missing));
	}

	#[test]
	fn change_frame_display() {
		assert_eq!(
			ChangeFrame::WorkingDirectory.to_string(),
			"working directory"
		);

		assert_eq!(
			ChangeFrame::BranchRange {
				base: "main".to_string(),
				head: "feature".to_string(),
			}
			.to_string(),
			"main...feature"
		);

		assert_eq!(
			ChangeFrame::PullRequest {
				target: "main".to_string(),
				pr_branch: "feature".to_string(),
			}
			.to_string(),
			"PR: main <- feature"
		);
	}

	#[test]
	fn revision_ranges() {
		assert_eq!(ChangeFrame::WorkingDirectory.revision_range(), "HEAD");
		assert_eq!(
			ChangeFrame::BranchRange {
				base: "main".to_string(),
				head: "feature".to_string(),
			}
			.revision_range(),
			"main...feature"
		);
		assert_eq!(
			ChangeFrame::PullRequest {
				target: "main".to_string(),
				pr_branch: "feature".to_string(),
			}
			.revision_range(),
			"main...feature"
		);
		assert_eq!(ChangeFrame::StagedOnly.revision_range(), "--staged");
	}

	#[test]
	fn includes_unstaged() {
		assert!(ChangeFrame::WorkingDirectory.includes_unstaged());
		assert!(
			ChangeFrame::BranchRange {
				base: "main".to_string(),
				head: "feature".to_string(),
			}
			.includes_unstaged()
		);
		assert!(!ChangeFrame::StagedOnly.includes_unstaged());
	}

	#[test]
	fn includes_staged() {
		assert!(ChangeFrame::WorkingDirectory.includes_staged());
		assert!(ChangeFrame::StagedOnly.includes_staged());
		assert!(
			ChangeFrame::BranchRange {
				base: "main".to_string(),
				head: "feature".to_string(),
			}
			.includes_staged()
		);
		assert!(
			!ChangeFrame::CustomRange {
				base: "v1.0.0".to_string(),
				head: "v2.0.0".to_string(),
			}
			.includes_staged()
		);
	}

	#[test]
	fn test_base_revision() {
		assert_eq!(ChangeFrame::WorkingDirectory.base_revision(), Some("HEAD"));
		assert_eq!(
			ChangeFrame::BranchRange {
				base: "main".to_string(),
				head: "feature".to_string(),
			}
			.base_revision(),
			Some("main")
		);
		assert_eq!(
			ChangeFrame::PullRequest {
				target: "main".to_string(),
				pr_branch: "feature".to_string(),
			}
			.base_revision(),
			Some("main")
		);
		assert_eq!(
			ChangeFrame::CustomRange {
				base: "v1.0.0".to_string(),
				head: "v2.0.0".to_string(),
			}
			.base_revision(),
			Some("v1.0.0")
		);
		assert_eq!(ChangeFrame::StagedOnly.base_revision(), Some("HEAD"));
	}

	#[test]
	fn test_head_revision() {
		assert_eq!(ChangeFrame::WorkingDirectory.head_revision(), None);
		assert_eq!(
			ChangeFrame::BranchRange {
				base: "main".to_string(),
				head: "feature".to_string(),
			}
			.head_revision(),
			Some("feature")
		);
		assert_eq!(
			ChangeFrame::PullRequest {
				target: "main".to_string(),
				pr_branch: "feature".to_string(),
			}
			.head_revision(),
			Some("feature")
		);
		assert_eq!(
			ChangeFrame::CustomRange {
				base: "v1.0.0".to_string(),
				head: "v2.0.0".to_string(),
			}
			.head_revision(),
			Some("v2.0.0")
		);
		assert_eq!(ChangeFrame::StagedOnly.head_revision(), Some("HEAD"));
	}

	#[test]
	fn test_pull_request_includes_staged() {
		// PullRequest frame does NOT include staged changes
		assert!(
			!ChangeFrame::PullRequest {
				target: "main".to_string(),
				pr_branch: "feature".to_string(),
			}
			.includes_staged()
		);
	}

	#[test]
	fn test_custom_range_display() {
		assert_eq!(
			ChangeFrame::CustomRange {
				base: "v1.0.0".to_string(),
				head: "v2.0.0".to_string(),
			}
			.to_string(),
			"v1.0.0...v2.0.0"
		);
	}

	#[test]
	fn test_staged_only_revision_range() {
		assert_eq!(ChangeFrame::StagedOnly.revision_range(), "--staged");
	}

	#[test]
	fn test_custom_range_revision_range() {
		assert_eq!(
			ChangeFrame::CustomRange {
				base: "v1.0.0".to_string(),
				head: "v2.0.0".to_string(),
			}
			.revision_range(),
			"v1.0.0...v2.0.0"
		);
	}

	#[test]
	fn test_pull_request_includes_unstaged() {
		assert!(
			!ChangeFrame::PullRequest {
				target: "main".to_string(),
				pr_branch: "feature".to_string(),
			}
			.includes_unstaged()
		);
	}

	#[test]
	fn test_custom_range_includes_unstaged() {
		assert!(
			!ChangeFrame::CustomRange {
				base: "v1.0.0".to_string(),
				head: "v2.0.0".to_string(),
			}
			.includes_unstaged()
		);
	}

	#[test]
	fn test_frame_error_conversions() {
		let frame_error = FrameError::Git("test error".to_string());
		let monochange_error: MonochangeError = frame_error.into();
		assert!(matches!(monochange_error, MonochangeError::Discovery(_)));
	}

	#[test]
	fn test_pr_environment_display_debug() {
		let pr_env = PrEnvironment {
			source_branch: "feature".to_string(),
			target_branch: "main".to_string(),
			pr_number: Some("42".to_string()),
			provider: "github".to_string(),
		};
		// Test that debug formatting works
		let debug_str = format!("{pr_env:?}");
		assert!(debug_str.contains("PrEnvironment"));
		assert!(debug_str.contains("feature"));
		assert!(debug_str.contains("main"));
	}

	#[test]
	fn test_change_frame_custom_range() {
		let frame = ChangeFrame::CustomRange {
			base: "v1.0.0".to_string(),
			head: "v2.0.0".to_string(),
		};
		assert_eq!(frame.base_revision(), Some("v1.0.0"));
		assert_eq!(frame.head_revision(), Some("v2.0.0"));
		assert!(!frame.includes_unstaged());
		assert!(!frame.includes_staged());
	}

	#[test]
	fn test_change_frame_staged_only() {
		let frame = ChangeFrame::StagedOnly;
		assert_eq!(frame.base_revision(), Some("HEAD"));
		assert_eq!(frame.head_revision(), Some("HEAD"));
		assert!(!frame.includes_unstaged());
		assert!(frame.includes_staged());
	}

	#[test]
	fn test_change_frame_branch_range() {
		let frame = ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		};
		assert_eq!(frame.base_revision(), Some("main"));
		assert_eq!(frame.head_revision(), Some("feature"));
		assert!(frame.includes_unstaged());
		assert!(frame.includes_staged());
	}

	#[test]
	fn test_change_frame_pull_request() {
		let frame = ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		};
		assert_eq!(frame.base_revision(), Some("main"));
		assert_eq!(frame.head_revision(), Some("feature"));
		assert!(!frame.includes_unstaged());
		// PullRequest does NOT include staged changes
		assert!(!frame.includes_staged());
	}

	#[test]
	fn test_change_frame_working_directory() {
		let frame = ChangeFrame::WorkingDirectory;
		assert_eq!(frame.base_revision(), Some("HEAD"));
		assert_eq!(frame.head_revision(), None);
		assert!(frame.includes_unstaged());
		assert!(frame.includes_staged());
	}

	#[test]
	fn test_change_frame_serialize_deserialize() {
		// Test that ChangeFrame can be serialized and deserialized
		let frame = ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		};
		let json =
			serde_json::to_string(&frame).unwrap_or_else(|e| panic!("Should serialize: {e}"));
		let deserialized: ChangeFrame =
			serde_json::from_str(&json).unwrap_or_else(|e| panic!("Should deserialize: {e}"));
		assert_eq!(frame.to_string(), deserialized.to_string());
	}

	#[test]
	fn test_change_frame_working_directory_serialize() {
		let frame = ChangeFrame::WorkingDirectory;
		let json =
			serde_json::to_string(&frame).unwrap_or_else(|e| panic!("Should serialize: {e}"));
		let deserialized: ChangeFrame =
			serde_json::from_str(&json).unwrap_or_else(|e| panic!("Should deserialize: {e}"));
		assert!(matches!(deserialized, ChangeFrame::WorkingDirectory));
	}

	#[test]
	fn test_change_frame_staged_only_serialize() {
		let frame = ChangeFrame::StagedOnly;
		let json =
			serde_json::to_string(&frame).unwrap_or_else(|e| panic!("Should serialize: {e}"));
		let deserialized: ChangeFrame =
			serde_json::from_str(&json).unwrap_or_else(|e| panic!("Should deserialize: {e}"));
		assert!(matches!(deserialized, ChangeFrame::StagedOnly));
	}

	#[test]
	fn test_change_frame_pull_request_serialize() {
		let frame = ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		};
		let json =
			serde_json::to_string(&frame).unwrap_or_else(|e| panic!("Should serialize: {e}"));
		let deserialized: ChangeFrame =
			serde_json::from_str(&json).unwrap_or_else(|e| panic!("Should deserialize: {e}"));
		assert!(matches!(deserialized, ChangeFrame::PullRequest { .. }));
	}

	#[test]
	fn test_change_frame_custom_range_serialize() {
		let frame = ChangeFrame::CustomRange {
			base: "v1.0.0".to_string(),
			head: "v2.0.0".to_string(),
		};
		let json =
			serde_json::to_string(&frame).unwrap_or_else(|e| panic!("Should serialize: {e}"));
		let deserialized: ChangeFrame =
			serde_json::from_str(&json).unwrap_or_else(|e| panic!("Should deserialize: {e}"));
		assert!(matches!(deserialized, ChangeFrame::CustomRange { .. }));
	}

	#[test]
	fn test_pr_environment_with_none_pr_number() {
		let pr_env = PrEnvironment {
			source_branch: "feature".to_string(),
			target_branch: "main".to_string(),
			pr_number: None,
			provider: "github".to_string(),
		};
		assert_eq!(pr_env.source_branch, "feature");
		assert_eq!(pr_env.target_branch, "main");
		assert_eq!(pr_env.pr_number, None);
		assert_eq!(pr_env.provider, "github");
	}

	#[test]
	fn test_pr_environment_clone() {
		let pr_env = PrEnvironment {
			source_branch: "feature".to_string(),
			target_branch: "main".to_string(),
			pr_number: Some("42".to_string()),
			provider: "github".to_string(),
		};
		let cloned = pr_env.clone();
		assert_eq!(cloned.source_branch, "feature");
		assert_eq!(cloned.pr_number, Some("42".to_string()));
	}

	#[test]
	fn test_frame_error_display() {
		let git_error = FrameError::Git("test error".to_string());
		assert_eq!(git_error.to_string(), "git error: test error");

		let env_error = FrameError::Environment("env error".to_string());
		assert_eq!(env_error.to_string(), "environment error: env error");

		let invalid_error = FrameError::InvalidFrame("invalid".to_string());
		assert_eq!(invalid_error.to_string(), "invalid frame: invalid");
	}

	#[test]
	fn test_frame_error_source() {
		// Test that FrameError implements Error trait
		let error = FrameError::Git("test".to_string());
		let _ = std::error::Error::source(&error);
	}

	#[test]
	fn test_pr_environment_eq() {
		let pr1 = PrEnvironment {
			source_branch: "feature".to_string(),
			target_branch: "main".to_string(),
			pr_number: Some("42".to_string()),
			provider: "github".to_string(),
		};
		let pr2 = PrEnvironment {
			source_branch: "feature".to_string(),
			target_branch: "main".to_string(),
			pr_number: Some("42".to_string()),
			provider: "github".to_string(),
		};
		let pr3 = PrEnvironment {
			source_branch: "other".to_string(),
			target_branch: "main".to_string(),
			pr_number: Some("42".to_string()),
			provider: "github".to_string(),
		};
		assert_eq!(pr1, pr2);
		assert_ne!(pr1, pr3);
	}

	#[test]
	fn test_change_frame_staged_only_display() {
		let frame = ChangeFrame::StagedOnly;
		assert_eq!(frame.to_string(), "staged only");
	}

	#[test]
	fn test_change_frame_equality() {
		let frame1 = ChangeFrame::WorkingDirectory;
		let frame2 = ChangeFrame::WorkingDirectory;
		let frame3 = ChangeFrame::StagedOnly;
		assert_eq!(frame1, frame2);
		assert_ne!(frame1, frame3);
	}

	#[test]
	fn test_change_frame_branch_range_equality() {
		let frame1 = ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		};
		let frame2 = ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		};
		let frame3 = ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "other".to_string(),
		};
		assert_eq!(frame1, frame2);
		assert_ne!(frame1, frame3);
	}

	#[test]
	fn test_change_frame_serialize_roundtrip() {
		let frames = vec![
			ChangeFrame::WorkingDirectory,
			ChangeFrame::StagedOnly,
			ChangeFrame::BranchRange {
				base: "main".to_string(),
				head: "feature".to_string(),
			},
			ChangeFrame::PullRequest {
				target: "main".to_string(),
				pr_branch: "feature".to_string(),
			},
			ChangeFrame::CustomRange {
				base: "v1.0.0".to_string(),
				head: "v2.0.0".to_string(),
			},
		];

		for frame in frames {
			let json =
				serde_json::to_string(&frame).unwrap_or_else(|e| panic!("should serialize: {e}"));
			let deserialized: ChangeFrame =
				serde_json::from_str(&json).unwrap_or_else(|e| panic!("should deserialize: {e}"));
			assert_eq!(frame.to_string(), deserialized.to_string());
		}
	}

	#[test]
	fn test_pr_environment_debug() {
		let pr_env = PrEnvironment {
			source_branch: "feature".to_string(),
			target_branch: "main".to_string(),
			pr_number: Some("42".to_string()),
			provider: "github".to_string(),
		};
		let debug = format!("{pr_env:?}");
		assert!(debug.contains("PrEnvironment"));
		assert!(debug.contains("feature"));
		assert!(debug.contains("main"));
		assert!(debug.contains("github"));
	}

	#[test]
	fn test_change_frame_debug() {
		let frame = ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		};
		let debug = format!("{frame:?}");
		assert!(debug.contains("BranchRange"));
		assert!(debug.contains("main"));
		assert!(debug.contains("feature"));
	}
}
