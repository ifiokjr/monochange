#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_github`
//!
//! <!-- {=monochangeGithubCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_github` turns `MonoChange` release manifests into GitHub automation requests.
//!
//! Reach for this crate when you want to preview or publish GitHub releases and release pull requests using the same structured release data that powers changelog files and release manifests.
//!
//! ## Why use it?
//!
//! - derive GitHub release payloads and release-PR bodies from `MonoChange`'s structured release manifest
//! - keep GitHub automation aligned with changelog rendering and release targets
//! - reuse one publishing path for dry-run previews and real repository updates
//!
//! ## Best for
//!
//! - building GitHub release automation on top of `mc release`
//! - previewing would-be GitHub releases and release PRs in CI before publishing
//! - converting grouped or package release targets into repository automation payloads
//!
//! ## Public entry points
//!
//! - `build_release_requests(config, manifest)` converts a release manifest into GitHub release requests
//! - `publish_release_requests(requests)` publishes requests through the `gh` CLI when available
//! - `build_release_pull_request_request(config, manifest)` converts a release manifest into a GitHub release-PR request
//! - `publish_release_pull_request(root, request, tracked_paths)` creates or updates a release PR through `git` and `gh`
//!
//! ## Example
//!
//! ```rust
//! use monochange_core::GitHubConfiguration;
//! use monochange_core::GitHubPullRequestSettings;
//! use monochange_core::GitHubReleaseSettings;
//! use monochange_core::ReleaseManifest;
//! use monochange_core::ReleaseManifestPlan;
//! use monochange_core::ReleaseManifestTarget;
//! use monochange_core::ReleaseOwnerKind;
//! use monochange_core::VersionFormat;
//! use monochange_github::build_release_requests;
//!
//! let manifest = ReleaseManifest {
//!     workflow: "release".to_string(),
//!     dry_run: true,
//!     version: Some("1.2.0".to_string()),
//!     group_version: Some("1.2.0".to_string()),
//!     release_targets: vec![ReleaseManifestTarget {
//!         id: "sdk".to_string(),
//!         kind: ReleaseOwnerKind::Group,
//!         version: "1.2.0".to_string(),
//!         tag: true,
//!         release: true,
//!         version_format: VersionFormat::Primary,
//!         tag_name: "v1.2.0".to_string(),
//!         members: vec!["core".to_string(), "app".to_string()],
//!     }],
//!     released_packages: vec!["workflow-core".to_string(), "workflow-app".to_string()],
//!     changed_files: Vec::new(),
//!     changelogs: Vec::new(),
//!     deleted_changesets: Vec::new(),
//!     deployments: Vec::new(),
//!     plan: ReleaseManifestPlan {
//!         workspace_root: std::path::PathBuf::from("."),
//!         decisions: Vec::new(),
//!         groups: Vec::new(),
//!         warnings: Vec::new(),
//!         unresolved_items: Vec::new(),
//!         compatibility_evidence: Vec::new(),
//!     },
//! };
//! let github = GitHubConfiguration {
//!     owner: "ifiokjr".to_string(),
//!     repo: "monochange".to_string(),
//!     releases: GitHubReleaseSettings::default(),
//!     pull_requests: GitHubPullRequestSettings::default(),
//! };
//!
//! let requests = build_release_requests(&github, &manifest);
//!
//! assert_eq!(requests.len(), 1);
//! assert_eq!(requests[0].tag_name, "v1.2.0");
//! assert_eq!(requests[0].repository, "ifiokjr/monochange");
//! ```
//! <!-- {/monochangeGithubCrateDocs} -->

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use monochange_core::GitHubConfiguration;
use monochange_core::GitHubReleaseNotesSource;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseOwnerKind;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubReleaseRequest {
	pub repository: String,
	pub owner: String,
	pub repo: String,
	pub target_id: String,
	pub target_kind: ReleaseOwnerKind,
	pub tag_name: String,
	pub name: String,
	pub body: Option<String>,
	pub draft: bool,
	pub prerelease: bool,
	pub generate_release_notes: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitHubReleaseOperation {
	Created,
	Updated,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubReleaseOutcome {
	pub repository: String,
	pub tag_name: String,
	pub operation: GitHubReleaseOperation,
	pub html_url: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPullRequestRequest {
	pub repository: String,
	pub owner: String,
	pub repo: String,
	pub base_branch: String,
	pub head_branch: String,
	pub title: String,
	pub body: String,
	pub labels: Vec<String>,
	pub auto_merge: bool,
	pub commit_message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitHubPullRequestOperation {
	Created,
	Updated,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPullRequestOutcome {
	pub repository: String,
	pub number: u64,
	pub head_branch: String,
	pub operation: GitHubPullRequestOperation,
	pub url: Option<String>,
}

#[must_use]
pub fn build_release_requests(
	github: &GitHubConfiguration,
	manifest: &ReleaseManifest,
) -> Vec<GitHubReleaseRequest> {
	manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
		.map(|target| GitHubReleaseRequest {
			repository: format!("{}/{}", github.owner, github.repo),
			owner: github.owner.clone(),
			repo: github.repo.clone(),
			target_id: target.id.clone(),
			target_kind: target.kind,
			tag_name: target.tag_name.clone(),
			name: release_name(target),
			body: release_body(github, manifest, target),
			draft: github.releases.draft,
			prerelease: github.releases.prerelease,
			generate_release_notes: github.releases.generate_notes,
		})
		.collect()
}

#[must_use]
pub fn build_release_pull_request_request(
	github: &GitHubConfiguration,
	manifest: &ReleaseManifest,
) -> GitHubPullRequestRequest {
	let repository = format!("{}/{}", github.owner, github.repo);
	let title = github.pull_requests.title.clone();
	GitHubPullRequestRequest {
		repository: repository.clone(),
		owner: github.owner.clone(),
		repo: github.repo.clone(),
		base_branch: github.pull_requests.base.clone(),
		head_branch: release_pull_request_branch(
			&github.pull_requests.branch_prefix,
			&manifest.workflow,
		),
		title: title.clone(),
		body: release_pull_request_body(manifest),
		labels: github.pull_requests.labels.clone(),
		auto_merge: github.pull_requests.auto_merge,
		commit_message: title,
	}
}

pub fn publish_release_requests(
	requests: &[GitHubReleaseRequest],
) -> MonochangeResult<Vec<GitHubReleaseOutcome>> {
	requests
		.iter()
		.map(publish_release_request)
		.collect::<Result<Vec<_>, _>>()
}

pub fn publish_release_pull_request(
	root: &Path,
	request: &GitHubPullRequestRequest,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<GitHubPullRequestOutcome> {
	git_checkout_branch(root, &request.head_branch)?;
	git_stage_paths(root, tracked_paths)?;
	git_commit_paths(root, &request.commit_message)?;
	git_push_branch(root, &request.head_branch)?;

	let operation = if let Some(existing) = lookup_existing_pull_request(request)? {
		update_pull_request(request, existing.number)?;
		GitHubPullRequestOperation::Updated
	} else {
		create_pull_request(request)?;
		GitHubPullRequestOperation::Created
	};
	let pull_request = lookup_existing_pull_request(request)?.ok_or_else(|| {
		MonochangeError::Config(format!(
			"failed to resolve release pull request for branch `{}` after publication",
			request.head_branch
		))
	})?;
	if request.auto_merge {
		enable_pull_request_auto_merge(request, pull_request.number)?;
	}
	Ok(GitHubPullRequestOutcome {
		repository: request.repository.clone(),
		number: pull_request.number,
		head_branch: request.head_branch.clone(),
		operation,
		url: pull_request.url,
	})
}

fn publish_release_request(
	request: &GitHubReleaseRequest,
) -> MonochangeResult<GitHubReleaseOutcome> {
	let existing = lookup_existing_release(request)?;
	let payload = serde_json::to_string(&json!({
		"tag_name": request.tag_name,
		"name": request.name,
		"body": request.body,
		"draft": request.draft,
		"prerelease": request.prerelease,
		"generate_release_notes": request.generate_release_notes,
	}))
	.map_err(|error| MonochangeError::Config(error.to_string()))?;
	let (operation, endpoint) = match existing {
		Some(existing) => (
			GitHubReleaseOperation::Updated,
			format!(
				"repos/{}/{}/releases/{}",
				request.owner, request.repo, existing.id
			),
		),
		None => (
			GitHubReleaseOperation::Created,
			format!("repos/{}/{}/releases", request.owner, request.repo),
		),
	};
	let method = match operation {
		GitHubReleaseOperation::Created => "POST",
		GitHubReleaseOperation::Updated => "PATCH",
	};
	let output = Command::new("gh")
		.arg("api")
		.arg("--method")
		.arg(method)
		.arg(endpoint)
		.arg("--input")
		.arg("-")
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped())
		.spawn()
		.and_then(|mut child| {
			use std::io::Write;
			child
				.stdin
				.as_mut()
				.ok_or_else(|| std::io::Error::other("failed to open gh stdin"))?
				.write_all(payload.as_bytes())?;
			child.wait_with_output()
		})
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to run `gh api` for {}: {error}",
				request.tag_name
			))
		})?;
	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"GitHub release publish failed for `{}`: {}",
			request.tag_name,
			String::from_utf8_lossy(&output.stderr).trim()
		)));
	}
	let response = serde_json::from_slice::<GitHubReleaseResponse>(&output.stdout)
		.map_err(|error| MonochangeError::Config(error.to_string()))?;
	Ok(GitHubReleaseOutcome {
		repository: request.repository.clone(),
		tag_name: request.tag_name.clone(),
		operation,
		html_url: response.html_url,
	})
}

fn lookup_existing_release(
	request: &GitHubReleaseRequest,
) -> MonochangeResult<Option<GitHubExistingRelease>> {
	let output = Command::new("gh")
		.arg("api")
		.arg(format!(
			"repos/{}/{}/releases/tags/{}",
			request.owner, request.repo, request.tag_name
		))
		.output()
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to query existing GitHub release for `{}`: {error}",
				request.tag_name
			))
		})?;
	if output.status.success() {
		let release = serde_json::from_slice::<GitHubExistingRelease>(&output.stdout)
			.map_err(|error| MonochangeError::Config(error.to_string()))?;
		return Ok(Some(release));
	}
	let stderr = String::from_utf8_lossy(&output.stderr);
	if stderr.contains("HTTP 404") || stderr.contains("Not Found") {
		return Ok(None);
	}
	Err(MonochangeError::Config(format!(
		"failed to query GitHub release `{}`: {}",
		request.tag_name,
		stderr.trim()
	)))
}

fn create_pull_request(request: &GitHubPullRequestRequest) -> MonochangeResult<()> {
	let mut command = Command::new("gh");
	command
		.arg("pr")
		.arg("create")
		.arg("--repo")
		.arg(&request.repository)
		.arg("--base")
		.arg(&request.base_branch)
		.arg("--head")
		.arg(&request.head_branch)
		.arg("--title")
		.arg(&request.title)
		.arg("--body")
		.arg(&request.body);
	for label in &request.labels {
		command.arg("--label").arg(label);
	}
	run_command(command, "create GitHub release pull request")?;
	Ok(())
}

fn update_pull_request(request: &GitHubPullRequestRequest, number: u64) -> MonochangeResult<()> {
	let mut command = Command::new("gh");
	command
		.arg("pr")
		.arg("edit")
		.arg(number.to_string())
		.arg("--repo")
		.arg(&request.repository)
		.arg("--title")
		.arg(&request.title)
		.arg("--body")
		.arg(&request.body);
	for label in &request.labels {
		command.arg("--add-label").arg(label);
	}
	run_command(command, "update GitHub release pull request")?;
	Ok(())
}

fn enable_pull_request_auto_merge(
	request: &GitHubPullRequestRequest,
	number: u64,
) -> MonochangeResult<()> {
	run_command(
		{
			let mut command = Command::new("gh");
			command
				.arg("pr")
				.arg("merge")
				.arg(number.to_string())
				.arg("--repo")
				.arg(&request.repository)
				.arg("--auto")
				.arg("--squash")
				.arg("--delete-branch=false");
			command
		},
		"enable GitHub pull request auto merge",
	)?;
	Ok(())
}

fn lookup_existing_pull_request(
	request: &GitHubPullRequestRequest,
) -> MonochangeResult<Option<GitHubExistingPullRequest>> {
	let output = Command::new("gh")
		.arg("pr")
		.arg("list")
		.arg("--repo")
		.arg(&request.repository)
		.arg("--state")
		.arg("open")
		.arg("--head")
		.arg(&request.head_branch)
		.arg("--json")
		.arg("number,url")
		.output()
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to query GitHub pull requests for `{}`: {error}",
				request.head_branch
			))
		})?;
	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"failed to query GitHub pull requests for `{}`: {}",
			request.head_branch,
			String::from_utf8_lossy(&output.stderr).trim()
		)));
	}
	let pull_requests = serde_json::from_slice::<Vec<GitHubExistingPullRequest>>(&output.stdout)
		.map_err(|error| MonochangeError::Config(error.to_string()))?;
	Ok(pull_requests.into_iter().next())
}

fn git_checkout_branch(root: &Path, branch: &str) -> MonochangeResult<()> {
	run_command(
		{
			let mut command = Command::new("git");
			command
				.current_dir(root)
				.arg("checkout")
				.arg("-B")
				.arg(branch);
			command
		},
		"prepare release pull request branch",
	)
}

fn git_stage_paths(root: &Path, tracked_paths: &[PathBuf]) -> MonochangeResult<()> {
	let mut command = Command::new("git");
	command.current_dir(root).arg("add").arg("-A").arg("--");
	for path in tracked_paths {
		command.arg(path);
	}
	run_command(command, "stage release pull request files")
}

fn git_commit_paths(root: &Path, message: &str) -> MonochangeResult<()> {
	run_command(
		{
			let mut command = Command::new("git");
			command
				.current_dir(root)
				.arg("commit")
				.arg("--message")
				.arg(message);
			command
		},
		"commit release pull request changes",
	)
}

fn git_push_branch(root: &Path, branch: &str) -> MonochangeResult<()> {
	run_command(
		{
			let mut command = Command::new("git");
			command
				.current_dir(root)
				.arg("push")
				.arg("--force-with-lease")
				.arg("origin")
				.arg(format!("HEAD:{branch}"));
			command
		},
		"push release pull request branch",
	)
}

fn run_command(mut command: Command, action: &str) -> MonochangeResult<()> {
	let output = command
		.output()
		.map_err(|error| MonochangeError::Io(format!("failed to {action}: {error}")))?;
	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"failed to {action}: {}",
			String::from_utf8_lossy(&output.stderr).trim()
		)));
	}
	Ok(())
}

fn release_name(target: &ReleaseManifestTarget) -> String {
	format!("{} {}", target.id, target.version)
}

fn release_body(
	github: &GitHubConfiguration,
	manifest: &ReleaseManifest,
	target: &ReleaseManifestTarget,
) -> Option<String> {
	match github.releases.source {
		GitHubReleaseNotesSource::GitHubGenerated => None,
		GitHubReleaseNotesSource::Monochange => manifest
			.changelogs
			.iter()
			.find(|changelog| {
				changelog.owner_id == target.id && changelog.owner_kind == target.kind
			})
			.map(|changelog| changelog.rendered.clone())
			.or_else(|| Some(minimal_release_body(manifest, target))),
	}
}

fn release_pull_request_branch(branch_prefix: &str, workflow: &str) -> String {
	let workflow = workflow
		.chars()
		.map(|character| {
			if character.is_ascii_alphanumeric() {
				character.to_ascii_lowercase()
			} else {
				'-'
			}
		})
		.collect::<String>()
		.trim_matches('-')
		.to_string();
	let workflow = if workflow.is_empty() {
		"release".to_string()
	} else {
		workflow
	};
	format!("{}/{}", branch_prefix.trim_end_matches('/'), workflow)
}

fn release_pull_request_body(manifest: &ReleaseManifest) -> String {
	let mut lines = vec!["## Prepared release".to_string(), String::new()];
	lines.push(format!("- workflow: `{}`", manifest.workflow));
	for target in manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
	{
		lines.push(format!(
			"- {} `{}` -> `{}`",
			target.kind, target.id, target.tag_name
		));
	}
	if !manifest.release_targets.iter().any(|target| target.release) {
		lines.push("- no outward release targets".to_string());
	}
	lines.push(String::new());
	lines.push("## Release notes".to_string());
	for target in manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
	{
		lines.push(String::new());
		lines.push(format!("### {} {}", target.id, target.version));
		if let Some(changelog) = manifest.changelogs.iter().find(|changelog| {
			changelog.owner_id == target.id && changelog.owner_kind == target.kind
		}) {
			for paragraph in &changelog.notes.summary {
				lines.push(String::new());
				lines.push(paragraph.clone());
			}
			for section in &changelog.notes.sections {
				if section.entries.is_empty() {
					continue;
				}
				lines.push(String::new());
				lines.push(format!("#### {}", section.title));
				lines.push(String::new());
				push_body_entries(&mut lines, &section.entries);
			}
		} else {
			lines.push(String::new());
			lines.push(minimal_release_body(manifest, target));
		}
	}
	if !manifest.changed_files.is_empty() {
		lines.push(String::new());
		lines.push("## Changed files".to_string());
		lines.push(String::new());
		for path in &manifest.changed_files {
			lines.push(format!("- {}", path.display()));
		}
	}
	lines.join("\n")
}

fn push_body_entries(lines: &mut Vec<String>, entries: &[String]) {
	for (index, entry) in entries.iter().enumerate() {
		let trimmed = entry.trim();
		if trimmed.contains('\n') {
			lines.extend(trimmed.lines().map(ToString::to_string));
			if index + 1 < entries.len() {
				lines.push(String::new());
			}
			continue;
		}
		if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with('#') {
			lines.push(trimmed.to_string());
		} else {
			lines.push(format!("- {trimmed}"));
		}
	}
}

fn minimal_release_body(manifest: &ReleaseManifest, target: &ReleaseManifestTarget) -> String {
	let mut lines = vec![format!("Release target `{}`", target.id), String::new()];
	if !target.members.is_empty() {
		lines.push(format!("Members: {}", target.members.join(", ")));
		lines.push(String::new());
	}
	let reasons = manifest
		.plan
		.decisions
		.iter()
		.filter(|decision| {
			target.kind == ReleaseOwnerKind::Package || target.members.contains(&decision.package)
		})
		.flat_map(|decision| decision.reasons.iter().cloned())
		.collect::<Vec<_>>();
	if reasons.is_empty() {
		lines.push("- prepare release".to_string());
	} else {
		for reason in reasons {
			lines.push(format!("- {reason}"));
		}
	}
	lines.join("\n")
}

#[derive(Debug, Deserialize)]
struct GitHubExistingRelease {
	id: u64,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseResponse {
	html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubExistingPullRequest {
	number: u64,
	url: Option<String>,
}

#[cfg(test)]
mod __tests;
