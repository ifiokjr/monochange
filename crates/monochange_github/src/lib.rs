#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_github`
//!
//! <!-- {=monochangeGithubCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_github` turns `monochange` release manifests into GitHub automation requests.
//!
//! Reach for this crate when you want to preview or publish GitHub releases and release pull requests using the same structured release data that powers changelog files and release manifests.
//!
//! ## Why use it?
//!
//! - derive GitHub release payloads and release-PR bodies from `monochange`'s structured release manifest
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
//! use monochange_core::BotSettings;
//! use monochange_core::ChangeRequestSettings;
//! use monochange_core::ReleaseProviderSettings;
//! use monochange_core::SourceConfiguration;
//! use monochange_core::SourceProvider;
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
//!         rendered_title: "1.2.0 (2026-04-06)".to_string(),
//!         rendered_changelog_title: "[1.2.0](https://example.com) (2026-04-06)".to_string(),
//!     }],
//!     released_packages: vec!["workflow-core".to_string(), "workflow-app".to_string()],
//!     changed_files: Vec::new(),
//!     changesets: Vec::new(),
//!     changelogs: Vec::new(),
//!     deleted_changesets: Vec::new(),
//!     plan: ReleaseManifestPlan {
//!         workspace_root: std::path::PathBuf::from("."),
//!         decisions: Vec::new(),
//!         groups: Vec::new(),
//!         warnings: Vec::new(),
//!         unresolved_items: Vec::new(),
//!         compatibility_evidence: Vec::new(),
//!     },
//! };
//! let github = SourceConfiguration {
//!     provider: SourceProvider::GitHub,
//!     owner: "ifiokjr".to_string(),
//!     repo: "monochange".to_string(),
//!     host: None,
//!     api_url: None,
//!     releases: ReleaseProviderSettings::default(),
//!     pull_requests: ChangeRequestSettings::default(),
//!     bot: BotSettings::default(),
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

use monochange_core::CommitMessage;
use monochange_core::HostedActorRef;
use monochange_core::HostedActorSourceKind;
use monochange_core::HostedIssueRef;
use monochange_core::HostedIssueRelationshipKind;
use monochange_core::HostedReviewRequestKind;
use monochange_core::HostedReviewRequestRef;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PreparedChangeset;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseNotesSource;
use monochange_core::ReleaseOwnerKind;
use monochange_core::RetargetOperation;
use monochange_core::RetargetProviderOperation;
use monochange_core::RetargetProviderResult;
use monochange_core::RetargetTagResult;
use monochange_core::SourceCapabilities;
use monochange_core::SourceChangeRequest;
use monochange_core::SourceChangeRequestOperation;
use monochange_core::SourceChangeRequestOutcome;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::SourceReleaseOperation;
use monochange_core::SourceReleaseOutcome;
use monochange_core::SourceReleaseRequest;
use octocrab::Octocrab;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use tokio::runtime::Builder as RuntimeBuilder;
use urlencoding::encode;

pub type GitHubReleaseRequest = SourceReleaseRequest;
pub type GitHubReleaseOperation = SourceReleaseOperation;
pub type GitHubReleaseOutcome = SourceReleaseOutcome;
pub type GitHubPullRequestRequest = SourceChangeRequest;
pub type GitHubPullRequestOperation = SourceChangeRequestOperation;
pub type GitHubPullRequestOutcome = SourceChangeRequestOutcome;

#[must_use]
pub const fn source_capabilities() -> SourceCapabilities {
	SourceCapabilities {
		draft_releases: true,
		prereleases: true,
		generated_release_notes: true,
		auto_merge_change_requests: true,
		released_issue_comments: true,
		requires_host: false,
	}
}

pub fn validate_source_configuration(source: &SourceConfiguration) -> MonochangeResult<()> {
	if source.releases.generate_notes
		&& matches!(source.releases.source, ReleaseNotesSource::Monochange)
	{
		return Err(MonochangeError::Config(
			"[source.releases].generate_notes cannot be true when `source = \"monochange\"`; choose one release-note source"
				.to_string(),
		));
	}
	Ok(())
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubIssueCommentPlan {
	pub repository: String,
	pub issue_id: String,
	pub issue_url: Option<String>,
	pub body: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitHubIssueCommentOperation {
	Created,
	SkippedExisting,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubIssueCommentOutcome {
	pub repository: String,
	pub issue_id: String,
	pub operation: GitHubIssueCommentOperation,
	pub url: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct GitHubRelatedReviewRequest {
	review_request: HostedReviewRequestRef,
	issues: Vec<HostedIssueRef>,
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
	html_url: Option<String>,
	target_commitish: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseResponse {
	html_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct GitHubReleaseRetargetPayload<'a> {
	target_commitish: &'a str,
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

#[derive(Debug, Deserialize)]
struct GitHubUserResponse {
	id: u64,
	login: String,
	html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubCommitPullRequestResponse {
	number: u64,
	title: String,
	html_url: Option<String>,
	body: Option<String>,
	user: Option<GitHubUserResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlPullRequestIssuesResponse {
	repository: Option<GraphqlPullRequestIssuesRepository>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlPullRequestIssuesRepository {
	pull_request: Option<GraphqlPullRequestIssuesNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlPullRequestIssuesNode {
	closing_issues_references: GraphqlIssueConnection,
}

#[derive(Debug, Deserialize)]
struct GraphqlIssueConnection {
	nodes: Vec<GraphqlIssueNode>,
}

#[derive(Debug, Deserialize)]
struct GraphqlIssueNode {
	number: u64,
	title: String,
	url: String,
}

#[derive(Debug, Deserialize)]
struct GitHubIssueCommentResponse {
	html_url: Option<String>,
	body: Option<String>,
}

#[must_use]
pub fn github_hosting_capabilities() -> HostingCapabilities {
	HostingCapabilities {
		commit_web_urls: true,
		actor_profiles: true,
		review_request_lookup: true,
		related_issues: true,
		issue_comments: true,
	}
}

#[must_use]
pub fn github_web_base_url() -> String {
	env::var("GITHUB_SERVER_URL").unwrap_or_else(|_| "https://github.com".to_string())
}

#[must_use]
pub fn github_host() -> Option<String> {
	let base_url = github_web_base_url();
	let without_scheme = base_url
		.trim_start_matches("https://")
		.trim_start_matches("http://");
	let host = without_scheme.split('/').next().unwrap_or_default().trim();
	if host.is_empty() {
		None
	} else {
		Some(host.to_string())
	}
}

#[must_use]
pub fn github_commit_url(source: &SourceConfiguration, sha: &str) -> String {
	format!(
		"{}/{}/{}/commit/{}",
		github_web_base_url().trim_end_matches('/'),
		source.owner,
		source.repo,
		sha
	)
}

#[must_use]
pub fn github_pull_request_url(source: &SourceConfiguration, number: u64) -> String {
	format!(
		"{}/{}/{}/pull/{}",
		github_web_base_url().trim_end_matches('/'),
		source.owner,
		source.repo,
		number
	)
}

#[must_use]
pub fn github_issue_url(source: &SourceConfiguration, number: u64) -> String {
	format!(
		"{}/{}/{}/issues/{}",
		github_web_base_url().trim_end_matches('/'),
		source.owner,
		source.repo,
		number
	)
}

/// URL to a specific tag on the GitHub repository.
#[must_use]
pub fn tag_url(source: &SourceConfiguration, tag_name: &str) -> String {
	let base = github_web_base_url();
	let base = source.host.as_deref().unwrap_or(base.trim_end_matches('/'));
	format!(
		"{}/{}/{}/releases/tag/{tag_name}",
		base.trim_end_matches('/'),
		source.owner,
		source.repo
	)
}

/// URL comparing two tags on the GitHub repository.
#[must_use]
pub fn compare_url(source: &SourceConfiguration, previous_tag: &str, current_tag: &str) -> String {
	let base = github_web_base_url();
	let base = source.host.as_deref().unwrap_or(base.trim_end_matches('/'));
	format!(
		"{}/{}/{}/compare/{previous_tag}...{current_tag}",
		base.trim_end_matches('/'),
		source.owner,
		source.repo
	)
}

pub fn enrich_changeset_context(
	source: &SourceConfiguration,
	changesets: &mut [PreparedChangeset],
) {
	let host = github_host();
	let capabilities = github_hosting_capabilities();
	for changeset in changesets.iter_mut() {
		let Some(context) = changeset.context.as_mut() else {
			continue;
		};
		context.provider = HostingProviderKind::GitHub;
		context.host.clone_from(&host);
		context.capabilities = capabilities.clone();
		for revision in [&mut context.introduced, &mut context.last_updated] {
			let Some(revision) = revision.as_mut() else {
				continue;
			};
			if let Some(commit) = revision.commit.as_mut() {
				commit.provider = HostingProviderKind::GitHub;
				commit.host.clone_from(&host);
				commit.url = Some(github_commit_url(source, &commit.sha));
			}
			if let Some(actor) = revision.actor.as_mut() {
				actor.provider = HostingProviderKind::GitHub;
				actor.host.clone_from(&host);
			}
		}
	}

	let Ok(token) = env::var("GITHUB_TOKEN").or_else(|_| env::var("GH_TOKEN")) else {
		return;
	};
	let Ok(runtime) = github_runtime() else {
		return;
	};
	let api_base_url = env::var("GITHUB_API_URL").ok();
	runtime.block_on(async {
		let Ok(client) = build_github_client(&token, api_base_url.as_deref()) else {
			return;
		};
		enrich_changeset_context_with_client(&client, source, changesets).await;
	});
}

#[must_use]
pub fn build_release_requests(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> Vec<GitHubReleaseRequest> {
	manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
		.map(|target| GitHubReleaseRequest {
			provider: SourceProvider::GitHub,
			repository: format!("{}/{}", source.owner, source.repo),
			owner: source.owner.clone(),
			repo: source.repo.clone(),
			target_id: target.id.clone(),
			target_kind: target.kind,
			tag_name: target.tag_name.clone(),
			name: target.rendered_title.clone(),
			body: release_body(source, manifest, target),
			draft: source.releases.draft,
			prerelease: source.releases.prerelease,
			generate_release_notes: source.releases.generate_notes,
		})
		.collect()
}

#[must_use]
pub fn build_release_pull_request_request(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> GitHubPullRequestRequest {
	let repository = format!("{}/{}", source.owner, source.repo);
	let title = source.pull_requests.title.clone();
	GitHubPullRequestRequest {
		provider: SourceProvider::GitHub,
		repository: repository.clone(),
		owner: source.owner.clone(),
		repo: source.repo.clone(),
		base_branch: source.pull_requests.base.clone(),
		head_branch: release_pull_request_branch(
			&source.pull_requests.branch_prefix,
			&manifest.command,
		),
		title: title.clone(),
		body: release_pull_request_body(manifest),
		labels: source.pull_requests.labels.clone(),
		auto_merge: source.pull_requests.auto_merge,
		commit_message: CommitMessage {
			subject: title,
			body: None,
		},
	}
}

async fn enrich_changeset_context_with_client(
	client: &Octocrab,
	source: &SourceConfiguration,
	changesets: &mut [PreparedChangeset],
) {
	let host = github_host();
	let capabilities = github_hosting_capabilities();
	for changeset in changesets.iter_mut() {
		let Some(context) = changeset.context.as_mut() else {
			continue;
		};
		context.provider = HostingProviderKind::GitHub;
		context.host.clone_from(&host);
		context.capabilities = capabilities.clone();
		for revision in [&mut context.introduced, &mut context.last_updated] {
			let Some(revision) = revision.as_mut() else {
				continue;
			};
			if let Some(commit) = revision.commit.as_mut() {
				commit.provider = HostingProviderKind::GitHub;
				commit.host.clone_from(&host);
				commit.url = Some(github_commit_url(source, &commit.sha));
			}
			if let Some(actor) = revision.actor.as_mut() {
				actor.provider = HostingProviderKind::GitHub;
				actor.host.clone_from(&host);
			}
		}
	}
	let mut review_requests_by_sha =
		std::collections::BTreeMap::<String, Option<GitHubRelatedReviewRequest>>::new();
	for changeset in changesets.iter_mut() {
		let Some(context) = changeset.context.as_mut() else {
			continue;
		};
		let mut issues_by_id = std::collections::BTreeMap::<String, HostedIssueRef>::new();
		for revision in [&mut context.introduced, &mut context.last_updated] {
			let Some(revision) = revision.as_mut() else {
				continue;
			};
			let Some(commit) = revision.commit.as_ref() else {
				continue;
			};
			let commit_sha = commit.sha.clone();
			let related_review_request = if let Some(cached) =
				review_requests_by_sha.get(&commit_sha)
			{
				cached.clone()
			} else {
				let loaded = lookup_commit_review_request_with_client(client, source, &commit_sha)
					.await
					.ok()
					.flatten();
				review_requests_by_sha.insert(commit_sha.clone(), loaded.clone());
				loaded
			};
			if let Some(related_review_request) = related_review_request {
				for issue in related_review_request.issues {
					issues_by_id.entry(issue.id.clone()).or_insert(issue);
				}
				revision.review_request = Some(related_review_request.review_request.clone());
				if let Some(author) = related_review_request.review_request.author.clone() {
					revision.actor = Some(author);
				}
			}
			if let Some(actor) = revision.actor.as_mut() {
				actor.provider = HostingProviderKind::GitHub;
				actor.host.clone_from(&host);
			}
		}
		context.related_issues = issues_by_id.into_values().collect();
	}
}

async fn lookup_commit_review_request_with_client(
	client: &Octocrab,
	source: &SourceConfiguration,
	sha: &str,
) -> MonochangeResult<Option<GitHubRelatedReviewRequest>> {
	let path = format!(
		"/repos/{}/{}/commits/{}/pulls",
		source.owner, source.repo, sha
	);
	let pull_requests = get_json::<Vec<GitHubCommitPullRequestResponse>>(client, &path).await?;
	let Some(pull_request) = pull_requests.into_iter().next() else {
		return Ok(None);
	};
	let author = pull_request.user.map(|user| HostedActorRef {
		provider: HostingProviderKind::GitHub,
		host: github_host(),
		id: Some(user.id.to_string()),
		login: Some(user.login.clone()),
		display_name: Some(user.login),
		url: user.html_url,
		source: HostedActorSourceKind::ReviewRequestAuthor,
	});
	let review_request = HostedReviewRequestRef {
		provider: HostingProviderKind::GitHub,
		host: github_host(),
		kind: HostedReviewRequestKind::PullRequest,
		id: format!("#{}", pull_request.number),
		title: Some(pull_request.title),
		url: pull_request
			.html_url
			.or_else(|| Some(github_pull_request_url(source, pull_request.number))),
		author,
	};
	let issues = load_pull_request_issues_with_client(
		client,
		source,
		pull_request.number,
		pull_request.body.as_deref(),
	)
	.await
	.unwrap_or_default();
	Ok(Some(GitHubRelatedReviewRequest {
		review_request,
		issues,
	}))
}

async fn load_pull_request_issues_with_client(
	client: &Octocrab,
	source: &SourceConfiguration,
	number: u64,
	body: Option<&str>,
) -> MonochangeResult<Vec<HostedIssueRef>> {
	let response = client
		.graphql::<GraphqlPullRequestIssuesResponse>(&json!({
			"query": "query($owner: String!, $repo: String!, $number: Int!) { repository(owner: $owner, name: $repo) { pullRequest(number: $number) { closingIssuesReferences(first: 50) { nodes { number title url } } } } }",
			"owner": source.owner,
			"repo": source.repo,
			"number": number,
		}))
		.await
		.map_err(|error| {
			MonochangeError::Config(format!(
				"failed to load GitHub closing issues for pull request #{number}: {error}"
			))
		})?;
	let mut issues_by_id = std::collections::BTreeMap::<String, HostedIssueRef>::new();
	for issue in response
		.repository
		.and_then(|repository| repository.pull_request)
		.into_iter()
		.flat_map(|pull_request| pull_request.closing_issues_references.nodes)
	{
		issues_by_id.insert(
			format!("#{}", issue.number),
			HostedIssueRef {
				provider: HostingProviderKind::GitHub,
				host: github_host(),
				id: format!("#{}", issue.number),
				title: Some(issue.title),
				url: Some(issue.url),
				relationship: HostedIssueRelationshipKind::ClosedByReviewRequest,
			},
		);
	}
	for issue_number in body.map(extract_issue_numbers).unwrap_or_default() {
		issues_by_id
			.entry(format!("#{issue_number}"))
			.or_insert_with(|| HostedIssueRef {
				provider: HostingProviderKind::GitHub,
				host: github_host(),
				id: format!("#{issue_number}"),
				title: None,
				url: Some(github_issue_url(source, issue_number)),
				relationship: HostedIssueRelationshipKind::ReferencedByReviewRequest,
			});
	}
	Ok(issues_by_id.into_values().collect())
}

fn extract_issue_numbers(text: &str) -> std::collections::BTreeSet<u64> {
	let mut issue_numbers = std::collections::BTreeSet::new();
	let bytes = text.as_bytes();
	let mut index = 0;
	while let Some(byte) = bytes.get(index) {
		if *byte != b'#' {
			index += 1;
			continue;
		}
		let mut digits = String::new();
		let mut cursor = index + 1;
		while let Some(next) = bytes.get(cursor) {
			if !next.is_ascii_digit() {
				break;
			}
			digits.push(char::from(*next));
			cursor += 1;
		}
		if let Ok(number) = digits.parse::<u64>() {
			issue_numbers.insert(number);
		}
		index = cursor;
	}
	issue_numbers
}

#[must_use]
pub fn plan_released_issue_comments(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> Vec<GitHubIssueCommentPlan> {
	let release_tags = manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
		.map(|target| target.tag_name.clone())
		.collect::<Vec<_>>();
	if release_tags.is_empty() {
		return Vec::new();
	}
	let marker = release_comment_marker(&release_tags);
	let body = release_issue_comment_body(&release_tags, &marker);
	let mut plans_by_issue = std::collections::BTreeMap::<String, GitHubIssueCommentPlan>::new();
	for issue in manifest
		.changesets
		.iter()
		.filter_map(|changeset| changeset.context.as_ref())
		.flat_map(|context| context.related_issues.iter())
		.filter(|issue| issue.relationship == HostedIssueRelationshipKind::ClosedByReviewRequest)
	{
		plans_by_issue
			.entry(issue.id.clone())
			.or_insert_with(|| GitHubIssueCommentPlan {
				repository: format!("{}/{}", source.owner, source.repo),
				issue_id: issue.id.clone(),
				issue_url: issue.url.clone(),
				body: body.clone(),
			});
	}
	plans_by_issue.into_values().collect()
}

pub fn comment_released_issues(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> MonochangeResult<Vec<GitHubIssueCommentOutcome>> {
	let plans = plan_released_issue_comments(source, manifest);
	if plans.is_empty() {
		return Ok(Vec::new());
	}
	let runtime = github_runtime()?;
	runtime.block_on(async {
		let client = github_client_from_env(source)?;
		let outcome = comment_released_issues_with_client(&client, source, &plans).await;
		outcome
	})
}

async fn comment_released_issues_with_client(
	client: &Octocrab,
	source: &SourceConfiguration,
	plans: &[GitHubIssueCommentPlan],
) -> MonochangeResult<Vec<GitHubIssueCommentOutcome>> {
	let mut outcomes = Vec::with_capacity(plans.len());
	for plan in plans {
		let issue_number = plan
			.issue_id
			.trim_start_matches('#')
			.parse::<u64>()
			.map_err(|error| {
				MonochangeError::Config(format!(
					"invalid issue id `{}` for release comment: {error}",
					plan.issue_id
				))
			})?;
		let path = format!(
			"/repos/{}/{}/issues/{}/comments",
			source.owner, source.repo, issue_number
		);
		let existing_comments = get_json::<Vec<GitHubIssueCommentResponse>>(client, &path).await?;
		if existing_comments.iter().any(|comment| {
			comment
				.body
				.as_deref()
				.is_some_and(|body| body.contains(&plan.body))
		}) {
			outcomes.push(GitHubIssueCommentOutcome {
				repository: plan.repository.clone(),
				issue_id: plan.issue_id.clone(),
				operation: GitHubIssueCommentOperation::SkippedExisting,
				url: plan.issue_url.clone(),
			});
			continue;
		}
		let response = post_json::<_, GitHubIssueCommentResponse>(
			client,
			&path,
			&json!({ "body": plan.body }),
		)
		.await?;
		outcomes.push(GitHubIssueCommentOutcome {
			repository: plan.repository.clone(),
			issue_id: plan.issue_id.clone(),
			operation: GitHubIssueCommentOperation::Created,
			url: response.html_url.or_else(|| plan.issue_url.clone()),
		});
	}
	Ok(outcomes)
}

fn release_comment_marker(release_tags: &[String]) -> String {
	format!("<!-- monochange:released-in:{} -->", release_tags.join("|"))
}

fn release_issue_comment_body(release_tags: &[String], marker: &str) -> String {
	if let Some(release_tag) = release_tags.first().filter(|_| release_tags.len() == 1) {
		format!("Released in {release_tag}.\n\n{marker}")
	} else {
		format!("Released in {}.\n\n{marker}", release_tags.join(", "))
	}
}

pub fn publish_release_requests(
	source: &SourceConfiguration,
	requests: &[GitHubReleaseRequest],
) -> MonochangeResult<Vec<GitHubReleaseOutcome>> {
	let runtime = github_runtime()?;
	runtime.block_on(async {
		let client = github_client_from_env(source)?;
		let outcome = publish_release_requests_with_client(&client, requests).await;
		outcome
	})
}

pub fn publish_release_pull_request(
	source: &SourceConfiguration,
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
		let client = github_client_from_env(source)?;
		let outcome = publish_release_pull_request_with_client(&client, request).await;
		outcome
	})
}

pub fn sync_retargeted_releases(
	source: &SourceConfiguration,
	tag_updates: &[RetargetTagResult],
	dry_run: bool,
) -> MonochangeResult<Vec<RetargetProviderResult>> {
	let runtime = github_runtime()?;
	runtime.block_on(async {
		let client = github_client_from_env(source)?;
		let outcomes =
			sync_retargeted_releases_with_client(&client, source, tag_updates, dry_run).await?;
		Ok(outcomes)
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
		provider: SourceProvider::GitHub,
		repository: request.repository.clone(),
		tag_name: request.tag_name.clone(),
		operation,
		url: response.html_url,
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
		provider: SourceProvider::GitHub,
		repository: request.repository.clone(),
		number: pull_request.number,
		head_branch: request.head_branch.clone(),
		operation,
		url: pull_request.html_url,
	})
}

async fn sync_retargeted_releases_with_client(
	client: &Octocrab,
	source: &SourceConfiguration,
	tag_updates: &[RetargetTagResult],
	dry_run: bool,
) -> MonochangeResult<Vec<RetargetProviderResult>> {
	let mut results = Vec::with_capacity(tag_updates.len());
	for update in tag_updates {
		if dry_run {
			results.push(RetargetProviderResult {
				provider: SourceProvider::GitHub,
				tag_name: update.tag_name.clone(),
				target_commit: update.to_commit.clone(),
				operation: RetargetProviderOperation::Planned,
				url: None,
				message: None,
			});
			continue;
		}
		let path = format!(
			"/repos/{}/{}/releases/tags/{}",
			source.owner, source.repo, update.tag_name
		);
		let Some(existing) = get_optional_json::<GitHubExistingRelease>(client, &path).await?
		else {
			return Err(MonochangeError::Config(format!(
				"GitHub release for tag `{}` could not be found",
				update.tag_name
			)));
		};
		if existing.target_commitish.as_deref() == Some(update.to_commit.as_str())
			|| update.operation == RetargetOperation::AlreadyUpToDate
		{
			results.push(RetargetProviderResult {
				provider: SourceProvider::GitHub,
				tag_name: update.tag_name.clone(),
				target_commit: update.to_commit.clone(),
				operation: RetargetProviderOperation::AlreadyAligned,
				url: existing.html_url,
				message: None,
			});
			continue;
		}
		let response = patch_json::<_, GitHubReleaseResponse>(
			client,
			&format!(
				"/repos/{}/{}/releases/{}",
				source.owner, source.repo, existing.id
			),
			&GitHubReleaseRetargetPayload {
				target_commitish: &update.to_commit,
			},
		)
		.await?;
		results.push(RetargetProviderResult {
			provider: SourceProvider::GitHub,
			tag_name: update.tag_name.clone(),
			target_commit: update.to_commit.clone(),
			operation: RetargetProviderOperation::Synced,
			url: response.html_url,
			message: None,
		});
	}
	Ok(results)
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

fn github_client_from_env(source: &SourceConfiguration) -> MonochangeResult<Octocrab> {
	let token = env::var("GITHUB_TOKEN")
		.or_else(|_| env::var("GH_TOKEN"))
		.map_err(|_| {
			MonochangeError::Config(
				"set `GITHUB_TOKEN` (or `GH_TOKEN`) before running GitHub automation".to_string(),
			)
		})?;
	let env_api_url = env::var("GITHUB_API_URL").ok();
	let api_url = source.api_url.as_deref().or(env_api_url.as_deref());
	build_github_client(&token, api_url)
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

fn git_commit_paths(root: &Path, message: &CommitMessage) -> MonochangeResult<()> {
	let output = {
		let mut command = Command::new("git");
		command
			.current_dir(root)
			.arg("commit")
			.arg("--message")
			.arg(&message.subject);
		if let Some(body) = &message.body {
			command.arg("--message").arg(body);
		}
		command
	}
	.output()
	.map_err(|error| {
		MonochangeError::Io(format!(
			"failed to commit release pull request changes: {error}"
		))
	})?;
	if output.status.success() {
		return Ok(());
	}
	let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
	let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
	if stderr.contains("nothing to commit") || stdout.contains("nothing to commit") {
		return Ok(());
	}
	let detail = if stderr.is_empty() { stdout } else { stderr };
	Err(MonochangeError::Config(format!(
		"failed to commit release pull request changes: {detail}"
	)))
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

fn release_body(
	github: &SourceConfiguration,
	manifest: &ReleaseManifest,
	target: &ReleaseManifestTarget,
) -> Option<String> {
	match github.releases.source {
		ReleaseNotesSource::GitHubGenerated => None,
		ReleaseNotesSource::Monochange => manifest
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
