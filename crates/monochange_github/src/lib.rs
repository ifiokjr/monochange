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
//! - `publish_release_requests(requests)` publishes requests through the GitHub API via `octocrab`
//! - `build_release_pull_request_request(config, manifest)` converts a release manifest into a GitHub release-PR request
//! - `publish_release_pull_request(root, request, tracked_paths)` creates or updates a release PR through `git` and the GitHub API
//!
//! ## Example
//!
//! ```rust
//! use monochange_core::GitHubBotSettings;
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
//!     command: "release".to_string(),
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
//!     bot: GitHubBotSettings::default(),
//! };
//!
//! let requests = build_release_requests(&github, &manifest);
//!
//! assert_eq!(requests.len(), 1);
//! assert_eq!(requests[0].tag_name, "v1.2.0");
//! assert_eq!(requests[0].repository, "ifiokjr/monochange");
//! ```
//! <!-- {/monochangeGithubCrateDocs} -->

use std::env;
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
use octocrab::Octocrab;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use tokio::runtime::Builder as RuntimeBuilder;
use urlencoding::encode;

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

#[derive(Debug, Serialize)]
struct GitHubReleasePayload<'a> {
	tag_name: &'a str,
	name: &'a str,
	body: Option<&'a str>,
	draft: bool,
	prerelease: bool,
	generate_release_notes: bool,
}

#[derive(Debug, Serialize)]
struct GitHubPullRequestPayload<'a> {
	title: &'a str,
	head: &'a str,
	base: &'a str,
	body: &'a str,
	draft: bool,
}

#[derive(Debug, Serialize)]
struct GitHubPullRequestUpdatePayload<'a> {
	title: &'a str,
	body: &'a str,
	base: &'a str,
}

#[derive(Debug, Serialize)]
struct GitHubLabelsPayload<'a> {
	labels: &'a [String],
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
struct GitHubPullRequestResponse {
	number: u64,
	html_url: Option<String>,
	node_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlEnableAutoMergeResponse {
	enable_pull_request_auto_merge: Option<GraphqlPullRequestMutation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlPullRequestMutation {
	pull_request: Option<GraphqlPullRequestNode>,
}

#[derive(Debug, Deserialize)]
struct GraphqlPullRequestNode {
	#[serde(rename = "number")]
	_number: u64,
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
			&manifest.command,
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
	let runtime = github_runtime()?;
	runtime.block_on(async {
		let client = github_client_from_env()?;
		let outcome = publish_release_requests_with_client(&client, requests).await;
		outcome
	})
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

	let runtime = github_runtime()?;
	runtime.block_on(async {
		let client = github_client_from_env()?;
		let outcome = publish_release_pull_request_with_client(&client, request).await;
		outcome
	})
}

async fn publish_release_requests_with_client(
	client: &Octocrab,
	requests: &[GitHubReleaseRequest],
) -> MonochangeResult<Vec<GitHubReleaseOutcome>> {
	let mut outcomes = Vec::with_capacity(requests.len());
	for request in requests {
		outcomes.push(publish_release_request_with_client(client, request).await?);
	}
	Ok(outcomes)
}

async fn publish_release_request_with_client(
	client: &Octocrab,
	request: &GitHubReleaseRequest,
) -> MonochangeResult<GitHubReleaseOutcome> {
	let payload = GitHubReleasePayload {
		tag_name: &request.tag_name,
		name: &request.name,
		body: request.body.as_deref(),
		draft: request.draft,
		prerelease: request.prerelease,
		generate_release_notes: request.generate_release_notes,
	};
	let existing = lookup_existing_release_with_client(client, request).await?;
	let (operation, response) = match existing {
		Some(existing) => (
			GitHubReleaseOperation::Updated,
			patch_json::<_, GitHubReleaseResponse>(
				client,
				&format!(
					"/repos/{}/{}/releases/{}",
					request.owner, request.repo, existing.id
				),
				&payload,
			)
			.await?,
		),
		None => (
			GitHubReleaseOperation::Created,
			post_json::<_, GitHubReleaseResponse>(
				client,
				&format!("/repos/{}/{}/releases", request.owner, request.repo),
				&payload,
			)
			.await?,
		),
	};
	Ok(GitHubReleaseOutcome {
		repository: request.repository.clone(),
		tag_name: request.tag_name.clone(),
		operation,
		html_url: response.html_url,
	})
}

async fn publish_release_pull_request_with_client(
	client: &Octocrab,
	request: &GitHubPullRequestRequest,
) -> MonochangeResult<GitHubPullRequestOutcome> {
	let existing = lookup_existing_pull_request_with_client(client, request).await?;
	let (operation, pull_request) = match existing {
		Some(existing) => (
			GitHubPullRequestOperation::Updated,
			patch_json::<_, GitHubPullRequestResponse>(
				client,
				&format!(
					"/repos/{}/{}/pulls/{}",
					request.owner, request.repo, existing.number
				),
				&GitHubPullRequestUpdatePayload {
					title: &request.title,
					body: &request.body,
					base: &request.base_branch,
				},
			)
			.await?,
		),
		None => (
			GitHubPullRequestOperation::Created,
			post_json::<_, GitHubPullRequestResponse>(
				client,
				&format!("/repos/{}/{}/pulls", request.owner, request.repo),
				&GitHubPullRequestPayload {
					title: &request.title,
					head: &request.head_branch,
					base: &request.base_branch,
					body: &request.body,
					draft: false,
				},
			)
			.await?,
		),
	};
	if !request.labels.is_empty() {
		let _: serde_json::Value = post_json(
			client,
			&format!(
				"/repos/{}/{}/issues/{}/labels",
				request.owner, request.repo, pull_request.number
			),
			&GitHubLabelsPayload {
				labels: &request.labels,
			},
		)
		.await?;
	}
	if request.auto_merge {
		enable_pull_request_auto_merge_with_client(client, &pull_request.node_id).await?;
	}
	Ok(GitHubPullRequestOutcome {
		repository: request.repository.clone(),
		number: pull_request.number,
		head_branch: request.head_branch.clone(),
		operation,
		url: pull_request.html_url,
	})
}

async fn lookup_existing_release_with_client(
	client: &Octocrab,
	request: &GitHubReleaseRequest,
) -> MonochangeResult<Option<GitHubExistingRelease>> {
	get_optional_json(
		client,
		&format!(
			"/repos/{}/{}/releases/tags/{}",
			request.owner,
			request.repo,
			encode(&request.tag_name)
		),
	)
	.await
}

async fn lookup_existing_pull_request_with_client(
	client: &Octocrab,
	request: &GitHubPullRequestRequest,
) -> MonochangeResult<Option<GitHubPullRequestResponse>> {
	let path = format!(
		"/repos/{}/{}/pulls?state=open&head={}:{}&base={}&per_page=1",
		request.owner,
		request.repo,
		encode(&request.owner),
		encode(&request.head_branch),
		encode(&request.base_branch)
	);
	let pull_requests = get_json::<Vec<GitHubPullRequestResponse>>(client, &path).await?;
	Ok(pull_requests.into_iter().next())
}

async fn enable_pull_request_auto_merge_with_client(
	client: &Octocrab,
	node_id: &str,
) -> MonochangeResult<()> {
	let response = client
		.graphql::<GraphqlEnableAutoMergeResponse>(&json!({
			"query": "mutation($pullRequestId: ID!) { enablePullRequestAutoMerge(input: { pullRequestId: $pullRequestId, mergeMethod: SQUASH }) { pullRequest { number } } }",
			"variables": {
				"pullRequestId": node_id,
			},
		}))
		.await
		.map_err(|error| {
			MonochangeError::Config(format!(
				"failed to enable GitHub pull request auto merge: {error}"
			))
		})?;
	if response
		.enable_pull_request_auto_merge
		.and_then(|payload| payload.pull_request)
		.is_none()
	{
		return Err(MonochangeError::Config(
			"GitHub pull request auto merge returned no pull request payload".to_string(),
		));
	}
	Ok(())
}

fn github_runtime() -> MonochangeResult<tokio::runtime::Runtime> {
	RuntimeBuilder::new_current_thread()
		.enable_all()
		.build()
		.map_err(|error| MonochangeError::Io(format!("failed to build GitHub runtime: {error}")))
}

fn github_client_from_env() -> MonochangeResult<Octocrab> {
	let token = env::var("GITHUB_TOKEN")
		.or_else(|_| env::var("GH_TOKEN"))
		.map_err(|_| {
			MonochangeError::Config(
				"set `GITHUB_TOKEN` (or `GH_TOKEN`) before running GitHub automation".to_string(),
			)
		})?;
	build_github_client(&token, env::var("GITHUB_API_URL").ok().as_deref())
}

fn build_github_client(token: &str, base_uri: Option<&str>) -> MonochangeResult<Octocrab> {
	let builder = Octocrab::builder().personal_token(token.to_string());
	let builder = if let Some(base_uri) = base_uri {
		builder.base_uri(base_uri).map_err(|error| {
			MonochangeError::Config(format!(
				"failed to configure GitHub base URL `{base_uri}`: {error}"
			))
		})?
	} else {
		builder
	};
	builder.build().map_err(|error| {
		MonochangeError::Config(format!("failed to build GitHub API client: {error}"))
	})
}

async fn get_optional_json<T>(client: &Octocrab, path: &str) -> MonochangeResult<Option<T>>
where
	T: DeserializeOwned,
{
	match client.get::<T, _, _>(path, None::<&()>).await {
		Ok(value) => Ok(Some(value)),
		Err(octocrab::Error::GitHub { source, .. }) if source.status_code.as_u16() == 404 => {
			Ok(None)
		}
		Err(error) => Err(MonochangeError::Config(format!(
			"GitHub API GET `{path}` failed: {error}"
		))),
	}
}

async fn get_json<T>(client: &Octocrab, path: &str) -> MonochangeResult<T>
where
	T: DeserializeOwned,
{
	match client.get::<T, _, _>(path, None::<&()>).await {
		Ok(value) => Ok(value),
		Err(error) => Err(MonochangeError::Config(format!(
			"GitHub API GET `{path}` failed: {error}"
		))),
	}
}

async fn post_json<Body, Response>(
	client: &Octocrab,
	path: &str,
	body: &Body,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
{
	client.post(path, Some(body)).await.map_err(|error| {
		MonochangeError::Config(format!("GitHub API POST `{path}` failed: {error}"))
	})
}

async fn patch_json<Body, Response>(
	client: &Octocrab,
	path: &str,
	body: &Body,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
{
	client.patch(path, Some(body)).await.map_err(|error| {
		MonochangeError::Config(format!("GitHub API PATCH `{path}` failed: {error}"))
	})
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

fn release_pull_request_branch(branch_prefix: &str, command: &str) -> String {
	let command = command
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
	let command = if command.is_empty() {
		"release".to_string()
	} else {
		command
	};
	format!("{}/{}", branch_prefix.trim_end_matches('/'), command)
}

fn release_pull_request_body(manifest: &ReleaseManifest) -> String {
	let mut lines = vec!["## Prepared release".to_string(), String::new()];
	lines.push(format!("- command: `{}`", manifest.command));
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

#[cfg(test)]
mod __tests;
