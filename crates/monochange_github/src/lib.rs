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
//! use monochange_core::ProviderMergeRequestSettings;
//! use monochange_core::ProviderReleaseSettings;
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
//!     package_publications: Vec::new(),
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
//!     owner: "monochange".to_string(),
//!     repo: "monochange".to_string(),
//!     host: None,
//!     api_url: None,
//!     releases: ProviderReleaseSettings::default(),
//!     pull_requests: ProviderMergeRequestSettings::default(),
//! };
//!
//! let requests = build_release_requests(&github, &manifest);
//!
//! assert_eq!(requests.len(), 1);
//! assert_eq!(requests[0].tag_name, "v1.2.0");
//! assert_eq!(requests[0].repository, "monochange/monochange");
//! ```
//! <!-- {/monochangeGithubCrateDocs} -->

use std::env;
use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::thread;

use monochange_core::CommitMessage;
use monochange_core::HostedActorRef;
use monochange_core::HostedActorSourceKind;
use monochange_core::HostedIssueCommentOperation;
use monochange_core::HostedIssueCommentOutcome;
use monochange_core::HostedIssueCommentPlan;
use monochange_core::HostedIssueRef;
use monochange_core::HostedIssueRelationshipKind;
use monochange_core::HostedReviewRequestKind;
use monochange_core::HostedReviewRequestRef;
use monochange_core::HostedSourceAdapter;
use monochange_core::HostedSourceFeatures;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PreparedChangeset;
use monochange_core::ProviderReleaseNotesSource;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestTarget;
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
use monochange_core::git::git_checkout_branch_command;
use monochange_core::git::git_command_output;
use monochange_core::git::git_current_branch;
use monochange_core::git::git_error_detail;
use monochange_core::git::git_head_commit;
use monochange_core::git::git_push_branch_command;
use monochange_core::git::git_stage_paths_command;
use monochange_core::git::run_command;
use monochange_core::git::run_git_commit_message;
use octocrab::Octocrab;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::runtime::Builder as RuntimeBuilder;
use urlencoding::encode;

pub type GitHubReleaseRequest = SourceReleaseRequest;
pub type GitHubReleaseOperation = SourceReleaseOperation;
pub type GitHubReleaseOutcome = SourceReleaseOutcome;
pub type GitHubPullRequestRequest = SourceChangeRequest;
pub type GitHubPullRequestOperation = SourceChangeRequestOperation;
pub type GitHubPullRequestOutcome = SourceChangeRequestOutcome;

type GitHubVerifiedCommitAttempt = Result<String, String>;

/// Return the hosted-source capabilities supported by the GitHub provider.
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

/// Validate that a source configuration is compatible with the GitHub provider.
#[must_use = "the validation result must be checked"]
pub fn validate_source_configuration(source: &SourceConfiguration) -> MonochangeResult<()> {
	if source.releases.generate_notes
		&& matches!(
			source.releases.source,
			ProviderReleaseNotesSource::Monochange
		) {
		return Err(MonochangeError::Config(
			"[source.releases].generate_notes cannot be true when `source = \"monochange\"`; choose one release-note source"
				.to_string(),
		));
	}

	Ok(())
}

/// Shared issue-comment planning type for GitHub issue release comments.
pub type GitHubIssueCommentPlan = HostedIssueCommentPlan;
/// Shared issue-comment operation type for GitHub issue release comments.
pub type GitHubIssueCommentOperation = HostedIssueCommentOperation;
/// Shared issue-comment outcome type for GitHub issue release comments.
pub type GitHubIssueCommentOutcome = HostedIssueCommentOutcome;

/// Shared GitHub hosted-source adapter instance used by the workspace.
pub static HOSTED_SOURCE_ADAPTER: GitHubHostedSourceAdapter = GitHubHostedSourceAdapter;

/// Hosted-source adapter for GitHub repositories.
pub struct GitHubHostedSourceAdapter;

impl HostedSourceAdapter for GitHubHostedSourceAdapter {
	fn provider(&self) -> SourceProvider {
		SourceProvider::GitHub
	}

	fn features(&self) -> HostedSourceFeatures {
		HostedSourceFeatures {
			batched_changeset_context_lookup: true,
			released_issue_comments: true,
			release_retarget_sync: true,
		}
	}

	fn annotate_changeset_context(
		&self,
		source: &SourceConfiguration,
		changesets: &mut [PreparedChangeset],
	) {
		annotate_changeset_context(source, changesets);
	}

	fn enrich_changeset_context(
		&self,
		source: &SourceConfiguration,
		changesets: &mut [PreparedChangeset],
	) {
		enrich_changeset_context(source, changesets);
	}

	fn plan_released_issue_comments(
		&self,
		source: &SourceConfiguration,
		manifest: &ReleaseManifest,
	) -> Vec<HostedIssueCommentPlan> {
		plan_released_issue_comments(source, manifest)
	}

	fn comment_released_issues(
		&self,
		source: &SourceConfiguration,
		manifest: &ReleaseManifest,
	) -> MonochangeResult<Vec<HostedIssueCommentOutcome>> {
		comment_released_issues(source, manifest)
	}

	fn sync_retargeted_releases(
		&self,
		source: &SourceConfiguration,
		tag_results: &[RetargetTagResult],
		dry_run: bool,
	) -> MonochangeResult<Vec<RetargetProviderResult>> {
		sync_retargeted_releases(source, tag_results, dry_run)
	}
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

#[derive(Debug, Serialize)]
struct GitHubCreateCommitPayload {
	message: String,
	tree: String,
	parents: Vec<String>,
}

#[derive(Debug, Serialize)]
struct GitHubUpdateRefPayload<'a> {
	sha: &'a str,
	force: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubGitCommitResponse {
	sha: String,
	message: String,
	tree: GitHubGitCommitTree,
	parents: Vec<GitHubGitCommitParent>,
	verification: GitHubGitCommitVerification,
}

#[derive(Debug, Deserialize)]
struct GitHubGitCommitTree {
	sha: String,
}

#[derive(Debug, Deserialize)]
struct GitHubGitCommitParent {
	sha: String,
}

#[derive(Debug, Deserialize)]
struct GitHubGitCommitVerification {
	verified: bool,
	reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubGitRefResponse {
	object: GitHubGitRefObject,
}

#[derive(Debug, Deserialize)]
struct GitHubGitRefObject {
	sha: String,
}

#[derive(Debug, Serialize)]
struct GitHubCreateBlobPayload {
	content: String,
	encoding: &'static str,
}

#[derive(Debug, Deserialize)]
struct GitHubCreateBlobResponse {
	sha: String,
}

#[derive(Debug, Serialize)]
struct GitHubCreateTreePayload<'a> {
	#[serde(skip_serializing_if = "Option::is_none")]
	base_tree: Option<&'a str>,
	tree: Vec<GitHubCreateTreeEntry>,
}

#[derive(Debug, Serialize)]
struct GitHubCreateTreeEntry {
	path: String,
	mode: &'static str,
	#[serde(rename = "type")]
	entry_type: &'static str,
	#[serde(skip_serializing_if = "Option::is_none")]
	sha: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubCreateTreeResponse {
	sha: String,
}

#[derive(Debug, Deserialize)]
struct GitHubExistingPullRequestLabel {
	name: String,
}

#[derive(Debug, Deserialize)]
struct GitHubExistingPullRequestBase {
	#[serde(rename = "ref")]
	ref_name: String,
}

#[derive(Debug, Deserialize)]
struct GitHubExistingPullRequestHead {
	sha: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubExistingPullRequest {
	number: u64,
	html_url: Option<String>,
	node_id: String,
	title: String,
	body: Option<String>,
	base: GitHubExistingPullRequestBase,
	head: GitHubExistingPullRequestHead,
	#[serde(default)]
	labels: Vec<GitHubExistingPullRequestLabel>,
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
struct GitHubIssueCommentResponse {
	html_url: Option<String>,
	body: Option<String>,
}

/// Return the hosting metadata features available from GitHub changeset context.
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

/// Return the GitHub web base URL for building browser links.
#[must_use]
pub fn github_web_base_url() -> String {
	env::var("GITHUB_SERVER_URL").unwrap_or_else(|_| "https://github.com".to_string())
}

/// Extract the host name used for rendered GitHub links.
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

/// Build a web URL for a commit on the configured GitHub repository.
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

/// Build a web URL for a pull request on the configured GitHub repository.
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

/// Build a web URL for an issue on the configured GitHub repository.
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

fn apply_github_changeset_annotations(
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
}

/// Apply GitHub URLs and provider metadata without making remote API calls.
///
/// Performance note:
/// `mc release --dry-run` should stay local and fast. The old path always went
/// on to look up PRs and related issues for every changeset commit whenever a
/// GitHub token was present, which turned a local preview into tens of seconds
/// of serialized network traffic. The dry-run release path now uses this helper
/// so the changelog context still gets stable GitHub commit links while the
/// expensive hosted lookups remain reserved for commands that truly need them.
pub fn annotate_changeset_context(
	source: &SourceConfiguration,
	changesets: &mut [PreparedChangeset],
) {
	apply_github_changeset_annotations(source, changesets);
}

/// Enrich changeset context with remote GitHub review-request and issue data.
#[tracing::instrument(skip_all)]
pub fn enrich_changeset_context(
	source: &SourceConfiguration,
	changesets: &mut [PreparedChangeset],
) {
	apply_github_changeset_annotations(source, changesets);

	let Ok(token) = env::var("GITHUB_TOKEN").or_else(|_| env::var("GH_TOKEN")) else {
		tracing::debug!("skipping GitHub enrichment: no GITHUB_TOKEN or GH_TOKEN found");
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

/// Convert releasable targets into provider-specific GitHub release requests.
#[must_use]
pub fn build_release_requests(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> Vec<GitHubReleaseRequest> {
	manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
		.map(|target| {
			GitHubReleaseRequest {
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
			}
		})
		.collect()
}

/// Build the release pull request request for the configured GitHub repository.
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
	let review_request_lookup_shas = collect_review_request_lookup_shas(changesets);
	let review_requests_by_sha =
		load_review_requests_for_commits_with_client(client, source, &review_request_lookup_shas)
			.await
			.unwrap_or_else(|error| {
				#[rustfmt::skip]
				tracing::warn!(commits = review_request_lookup_shas.len(), %error, "failed to batch load GitHub review requests; continuing with commit annotations only");
				std::collections::BTreeMap::new()
			});

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

			if let Some(related_review_request) = review_requests_by_sha
				.get(&commit.sha)
				.and_then(Clone::clone)
			{
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

fn collect_review_request_lookup_shas(changesets: &[PreparedChangeset]) -> Vec<String> {
	let mut shas = changesets
		.iter()
		.filter_map(|changeset| changeset.context.as_ref())
		.flat_map(|context| [&context.introduced, &context.last_updated])
		.filter_map(|revision| revision.as_ref())
		.filter_map(|revision| revision.commit.as_ref())
		.map(|commit| commit.sha.clone())
		.collect::<Vec<_>>();

	shas.sort();
	shas.dedup();

	shas
}

async fn load_review_requests_for_commits_with_client(
	client: &Octocrab,
	source: &SourceConfiguration,
	shas: &[String],
) -> MonochangeResult<std::collections::BTreeMap<String, Option<GitHubRelatedReviewRequest>>> {
	if shas.is_empty() {
		return Ok(std::collections::BTreeMap::new());
	}

	#[rustfmt::skip]
	tracing::info!(commits = shas.len(), requests = 1, "loading GitHub review requests");

	let review_requests_by_sha =
		load_review_request_batch_with_client(client, source, shas).await?;

	let review_requests_found = review_requests_by_sha
		.values()
		.filter(|review_request| review_request.is_some())
		.count();

	#[rustfmt::skip]
	tracing::debug!(commits = shas.len(), review_requests = review_requests_found, "resolved GitHub review requests");

	Ok(review_requests_by_sha)
}

async fn load_review_request_batch_with_client(
	client: &Octocrab,
	source: &SourceConfiguration,
	shas: &[String],
) -> MonochangeResult<std::collections::BTreeMap<String, Option<GitHubRelatedReviewRequest>>> {
	let query = build_review_request_batch_query(&source.owner, &source.repo, shas);

	let response = client
		.graphql::<serde_json::Value>(&json!({ "query": query }))
		.await
		.map_err(|error| {
			MonochangeError::Config(format!(
				"failed to batch load GitHub review requests for {} commit(s): {error}",
				shas.len()
			))
		})?;

	let repository = response
		.get("repository")
		.or_else(|| response.get("data").and_then(|data| data.get("repository")))
		.and_then(serde_json::Value::as_object)
		.ok_or_else(|| {
			MonochangeError::Config(
				"GitHub review-request lookup returned no repository payload".to_string(),
			)
		})?;

	let mut review_requests_by_sha =
		std::collections::BTreeMap::<String, Option<GitHubRelatedReviewRequest>>::new();

	for (index, sha) in shas.iter().enumerate() {
		let alias = format!("commit_{index}");
		let review_request = repository
			.get(&alias)
			.and_then(|commit| {
				commit
					.get("associatedPullRequests")
					.and_then(|pull_requests| pull_requests.get("nodes"))
					.and_then(serde_json::Value::as_array)
					.and_then(|pull_requests| pull_requests.first())
			})
			.and_then(|pull_request| parse_review_request_from_graphql(source, pull_request));
		review_requests_by_sha.insert(sha.clone(), review_request);
	}

	Ok(review_requests_by_sha)
}

fn build_review_request_batch_query(owner: &str, repo: &str, shas: &[String]) -> String {
	let mut query = format!("query {{ repository(owner: \"{owner}\", name: \"{repo}\") {{");
	for (index, sha) in shas.iter().enumerate() {
		let alias = format!("commit_{index}");
		let _ = write!(
			query,
			" {alias}: object(expression: \"{sha}\") {{ ... on Commit {{ associatedPullRequests(first: 1) {{ nodes {{ number title url body author {{ login url }} }} }} }} }}"
		);
	}
	query.push_str(" } }");
	query
}

fn parse_review_request_from_graphql(
	source: &SourceConfiguration,
	pull_request: &serde_json::Value,
) -> Option<GitHubRelatedReviewRequest> {
	let number = pull_request.get("number")?.as_u64()?;
	let title = pull_request
		.get("title")
		.and_then(serde_json::Value::as_str)?
		.to_string();
	let body = pull_request
		.get("body")
		.and_then(serde_json::Value::as_str)
		.map(str::to_string);
	let author = pull_request
		.get("author")
		.and_then(serde_json::Value::as_object)
		.map(|author| {
			HostedActorRef {
				provider: HostingProviderKind::GitHub,
				host: github_host(),
				id: None,
				login: author
					.get("login")
					.and_then(serde_json::Value::as_str)
					.map(str::to_string),
				display_name: author
					.get("login")
					.and_then(serde_json::Value::as_str)
					.map(str::to_string),
				url: author
					.get("url")
					.and_then(serde_json::Value::as_str)
					.map(str::to_string),
				source: HostedActorSourceKind::ReviewRequestAuthor,
			}
		});
	let review_request = HostedReviewRequestRef {
		provider: HostingProviderKind::GitHub,
		host: github_host(),
		kind: HostedReviewRequestKind::PullRequest,
		id: format!("#{number}"),
		title: Some(title),
		url: pull_request
			.get("url")
			.and_then(serde_json::Value::as_str)
			.map(str::to_string)
			.or_else(|| Some(github_pull_request_url(source, number))),
		author,
	};
	let mut issues_by_id = std::collections::BTreeMap::<String, HostedIssueRef>::new();
	for issue_number in body
		.as_deref()
		.map(extract_closing_issue_numbers)
		.unwrap_or_default()
	{
		issues_by_id.insert(
			format!("#{issue_number}"),
			HostedIssueRef {
				provider: HostingProviderKind::GitHub,
				host: github_host(),
				id: format!("#{issue_number}"),
				title: None,
				url: Some(github_issue_url(source, issue_number)),
				relationship: HostedIssueRelationshipKind::ClosedByReviewRequest,
			},
		);
	}
	for issue_number in body
		.as_deref()
		.map(extract_issue_numbers)
		.unwrap_or_default()
	{
		issues_by_id
			.entry(format!("#{issue_number}"))
			.or_insert_with(|| {
				HostedIssueRef {
					provider: HostingProviderKind::GitHub,
					host: github_host(),
					id: format!("#{issue_number}"),
					title: None,
					url: Some(github_issue_url(source, issue_number)),
					relationship: HostedIssueRelationshipKind::ReferencedByReviewRequest,
				}
			});
	}
	Some(GitHubRelatedReviewRequest {
		review_request,
		issues: issues_by_id.into_values().collect(),
	})
}

fn issue_reference_regex() -> &'static Regex {
	static ISSUE_REFERENCE_RE: OnceLock<Regex> = OnceLock::new();
	ISSUE_REFERENCE_RE.get_or_init(|| {
		Regex::new(r"(?:[\w.-]+/[\w.-]+)?#(?P<number>\d+)")
			.unwrap_or_else(|error| panic!("issue reference regex should compile: {error}"))
	})
}

fn closing_issue_reference_regex() -> &'static Regex {
	static CLOSING_ISSUE_REFERENCE_RE: OnceLock<Regex> = OnceLock::new();
	CLOSING_ISSUE_REFERENCE_RE.get_or_init(|| {
		Regex::new(r"(?i)\b(?:close|closes|closed|fix|fixes|fixed|resolve|resolves|resolved)\b[:\s]*(?P<refs>(?:[\w.-]+/[\w.-]+)?#\d+(?:\s*(?:,|and)\s*(?:[\w.-]+/[\w.-]+)?#\d+)*)")
		.unwrap_or_else(|error| panic!("closing issue regex should compile: {error}"))
	})
}

fn extract_closing_issue_numbers(text: &str) -> std::collections::BTreeSet<u64> {
	let mut issue_numbers = std::collections::BTreeSet::new();
	for captures in closing_issue_reference_regex().captures_iter(text) {
		let Some(references) = captures.name("refs") else {
			continue;
		};
		issue_numbers.extend(extract_issue_numbers(references.as_str()));
	}
	issue_numbers
}

fn extract_issue_numbers(text: &str) -> std::collections::BTreeSet<u64> {
	issue_reference_regex()
		.captures_iter(text)
		.filter_map(|captures| captures.name("number"))
		.filter_map(|number| number.as_str().parse::<u64>().ok())
		.collect()
}

/// Plan release comments for issues that are closed by the manifest's review requests.
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
	{
		plans_by_issue.entry(issue.id.clone()).or_insert_with(|| {
			GitHubIssueCommentPlan {
				repository: format!("{}/{}", source.owner, source.repo),
				issue_id: issue.id.clone(),
				issue_url: issue.url.clone(),
				body: body.clone(),
				close: issue.relationship != HostedIssueRelationshipKind::ClosedByReviewRequest,
			}
		});
	}
	plans_by_issue.into_values().collect()
}

/// Create release comments on linked GitHub issues when they have not been posted yet.
#[tracing::instrument(skip_all)]
#[must_use = "the comment result must be checked"]
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

		comment_released_issues_with_client(&client, source, &plans).await
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
			if plan.close {
				let issue_path = format!(
					"/repos/{}/{}/issues/{}",
					source.owner, source.repo, issue_number
				);
				let _: serde_json::Value =
					patch_json(client, &issue_path, &json!({ "state": "closed" })).await?;
			}
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
			operation: if plan.close {
				GitHubIssueCommentOperation::Closed
			} else {
				GitHubIssueCommentOperation::Created
			},
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

/// Publish or update all planned GitHub releases for a manifest.
#[tracing::instrument(skip_all)]
#[must_use = "the publish result must be checked"]
pub fn publish_release_requests(
	source: &SourceConfiguration,
	requests: &[GitHubReleaseRequest],
) -> MonochangeResult<Vec<GitHubReleaseOutcome>> {
	let runtime = github_runtime()?;
	runtime.block_on(async {
		let client = github_client_from_env(source)?;

		publish_release_requests_with_client(&client, requests).await
	})
}

/// Commit, push, and publish the release pull request against GitHub.
#[tracing::instrument(skip_all)]
#[must_use = "the pull request result must be checked"]
pub fn publish_release_pull_request(
	source: &SourceConfiguration,
	root: &Path,
	request: &GitHubPullRequestRequest,
	tracked_paths: &[PathBuf],
	no_verify: bool,
) -> MonochangeResult<GitHubPullRequestOutcome> {
	let lookup_source = source.clone();
	let lookup_request = request.clone();
	let existing_pull_request =
		thread::spawn(move || lookup_existing_pull_request(&lookup_source, &lookup_request));
	git_checkout_branch(root, &request.head_branch)?;
	git_stage_paths(root, tracked_paths)?;
	git_commit_paths(root, &request.commit_message, no_verify)?;
	let mut head_commit = git_head_commit(root)?;
	let existing = join_existing_pull_request_lookup(existing_pull_request)?;
	let head_matches_existing = existing
		.as_ref()
		.and_then(|pull_request| pull_request.head.sha.as_deref())
		== Some(head_commit.as_str());
	if !head_matches_existing {
		git_push_branch(root, &request.head_branch, no_verify)?;
		// Commits created through GitHub's Git Database API from GitHub Actions can be
		// marked verified by GitHub. Keep the pushed git commit as the fallback if the
		// API commit cannot be created, verified, or moved onto the release branch.
		head_commit = maybe_replace_release_pull_request_commit_with_verified_github_commit(
			source,
			request,
			&head_commit,
			root,
			tracked_paths,
		)
		.unwrap_or_else(|warning| {
			tracing::warn!(%warning, commit = %head_commit, "falling back to regular release pull request commit");
			head_commit.clone()
		});
	}

	let runtime = github_runtime()?;
	runtime.block_on(async {
		let client = github_client_from_env(source)?;

		publish_release_pull_request_with_existing_pull_request(
			&client,
			request,
			existing.as_ref(),
			&head_commit,
		)
		.await
	})
}

fn maybe_replace_release_pull_request_commit_with_verified_github_commit(
	source: &SourceConfiguration,
	request: &GitHubPullRequestRequest,
	fallback_commit: &str,
	root: &Path,
	tracked_paths: &[PathBuf],
) -> GitHubVerifiedCommitAttempt {
	if !github_actions_release_commit_verification_enabled(source) {
		return Ok(fallback_commit.to_string());
	}

	let runtime = github_runtime().map_err(|error| error.to_string())?;
	runtime.block_on(async {
		let client = github_client_from_env(source).map_err(|error| error.to_string())?;
		let verified_commit = create_verified_github_commit_for_release_pull_request(
			&client,
			request,
			fallback_commit,
			root,
			tracked_paths,
		)
		.await?;
		update_github_branch_ref_to_verified_commit(
			&client,
			request,
			fallback_commit,
			&verified_commit,
		)
		.await?;
		Ok(verified_commit)
	})
}

fn github_actions_release_commit_verification_enabled(source: &SourceConfiguration) -> bool {
	if !source.pull_requests.verified_commits {
		return false;
	}
	if env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
		return false;
	}
	let repository = format!("{}/{}", source.owner, source.repo);
	env::var("GITHUB_REPOSITORY").is_ok_and(|value| value.eq_ignore_ascii_case(&repository))
}

async fn create_verified_github_commit_for_release_pull_request(
	client: &Octocrab,
	request: &GitHubPullRequestRequest,
	fallback_commit: &str,
	root: &Path,
	tracked_paths: &[PathBuf],
) -> GitHubVerifiedCommitAttempt {
	let commit_path = format!(
		"/repos/{}/{}/git/commits/{}",
		request.owner, request.repo, fallback_commit
	);
	let original: GitHubGitCommitResponse = get_json(client, &commit_path)
		.await
		.map_err(|error| error.to_string())?;

	// Get the first parent's tree SHA to use as base_tree
	let base_tree = match original.parents.first() {
		Some(parent) => {
			let parent_path = format!(
				"/repos/{}/{}/git/commits/{}",
				request.owner, request.repo, parent.sha
			);
			let parent_commit: GitHubGitCommitResponse = get_json(client, &parent_path)
				.await
				.map_err(|error| error.to_string())?;
			Some(parent_commit.tree.sha)
		}
		None => None,
	};

	use std::os::unix::fs::PermissionsExt;

	let mut tree_entries = Vec::with_capacity(tracked_paths.len());
	for path in tracked_paths {
		let absolute_path = root.join(path);
		let relative_path = path.to_string_lossy().to_string();

		if !absolute_path.exists() {
			// File was deleted
			tree_entries.push(GitHubCreateTreeEntry {
				path: relative_path,
				mode: "100644",
				entry_type: "blob",
				sha: None,
			});
			continue;
		}

		let metadata = std::fs::symlink_metadata(&absolute_path)
			.map_err(|e| format!("failed to read metadata for {}: {}", path.display(), e))?;

		if metadata.is_dir() {
			// Directories are handled implicitly by their children
			continue;
		}

		let (content, mode) = if metadata.is_symlink() {
			let target = std::fs::read_link(&absolute_path)
				.map_err(|e| format!("failed to read symlink {}: {}", path.display(), e))?;
			let content = target.to_string_lossy().to_string();
			(content, "120000")
		} else {
			let content = std::fs::read_to_string(&absolute_path)
				.map_err(|e| format!("failed to read file {}: {}", path.display(), e))?;
			let mode = if metadata.permissions().mode() & 0o100 != 0 {
				"100755"
			} else {
				"100644"
			};
			(content, mode)
		};

		let blob_payload = GitHubCreateBlobPayload {
			content,
			encoding: "utf-8",
		};
		let blob_path = format!("/repos/{}/{}/git/blobs", request.owner, request.repo);
		let blob: GitHubCreateBlobResponse = post_json(client, &blob_path, &blob_payload)
			.await
			.map_err(|error| error.to_string())?;

		tree_entries.push(GitHubCreateTreeEntry {
			path: relative_path,
			mode,
			entry_type: "blob",
			sha: Some(blob.sha),
		});
	}

	// Create tree
	let tree_sha = if tree_entries.is_empty() {
		// No changes — reuse base tree
		base_tree.ok_or_else(|| "no base tree available for empty commit".to_string())?
	} else {
		let tree_payload = GitHubCreateTreePayload {
			base_tree: base_tree.as_deref(),
			tree: tree_entries,
		};
		let tree_path = format!("/repos/{}/{}/git/trees", request.owner, request.repo);
		let tree: GitHubCreateTreeResponse = post_json(client, &tree_path, &tree_payload)
			.await
			.map_err(|error| error.to_string())?;
		tree.sha
	};

	let commit_payload = GitHubCreateCommitPayload {
		message: original.message,
		tree: tree_sha,
		parents: original.parents.into_iter().map(|p| p.sha).collect(),
	};
	let create_path = format!("/repos/{}/{}/git/commits", request.owner, request.repo);
	let commit: GitHubGitCommitResponse = post_json(client, &create_path, &commit_payload)
		.await
		.map_err(|error| error.to_string())?;

	if commit.verification.verified {
		tracing::info!(
			commit = %commit.sha,
			reason = ?commit.verification.reason,
			"created verified GitHub release pull request commit"
		);
		return Ok(commit.sha);
	}

	Err(format!(
		"GitHub Git Database API created commit {} without verification ({})",
		commit.sha,
		commit
			.verification
			.reason
			.unwrap_or_else(|| "unknown reason".to_string())
	))
}

async fn update_github_branch_ref_to_verified_commit(
	client: &Octocrab,
	request: &GitHubPullRequestRequest,
	fallback_commit: &str,
	verified_commit: &str,
) -> GitHubVerifiedCommitAttempt {
	let get_ref_path = github_head_ref_get_path(request);
	let current: GitHubGitRefResponse = get_json(client, &get_ref_path)
		.await
		.map_err(|error| error.to_string())?;
	if current.object.sha != fallback_commit {
		return Err(format!(
			"release branch {} moved from {} to {}; refusing to replace it with verified commit {}",
			request.head_branch, fallback_commit, current.object.sha, verified_commit
		));
	}
	let payload = GitHubUpdateRefPayload {
		sha: verified_commit,
		force: true,
	};
	let update_ref_path = github_head_ref_update_path(request);
	let updated: GitHubGitRefResponse = patch_json(client, &update_ref_path, &payload)
		.await
		.map_err(|error| error.to_string())?;
	if updated.object.sha != verified_commit {
		return Err(format!(
			"GitHub returned {} after updating {}, expected {}",
			updated.object.sha, request.head_branch, verified_commit
		));
	}
	Ok(updated.object.sha)
}

fn github_head_ref_get_path(request: &GitHubPullRequestRequest) -> String {
	format!(
		"/repos/{}/{}/git/ref/heads/{}",
		request.owner, request.repo, request.head_branch
	)
}

fn github_head_ref_update_path(request: &GitHubPullRequestRequest) -> String {
	format!(
		"/repos/{}/{}/git/refs/heads/{}",
		request.owner, request.repo, request.head_branch
	)
}

/// Sync existing GitHub releases so retargeted tags point at the new commits.
#[tracing::instrument(skip_all)]
#[must_use = "the sync result must be checked"]
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
	tracing::info!(tag = %request.tag_name, repository = %request.repository, "publishing GitHub release");
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
		Some(existing) => {
			(
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
			)
		}
		None => {
			(
				GitHubReleaseOperation::Created,
				post_json::<_, GitHubReleaseResponse>(
					client,
					&format!("/repos/{}/{}/releases", request.owner, request.repo),
					&payload,
				)
				.await?,
			)
		}
	};
	Ok(GitHubReleaseOutcome {
		provider: SourceProvider::GitHub,
		repository: request.repository.clone(),
		tag_name: request.tag_name.clone(),
		operation,
		url: response.html_url,
	})
}

#[cfg_attr(not(test), allow(dead_code))]
async fn publish_release_pull_request_with_client(
	client: &Octocrab,
	request: &GitHubPullRequestRequest,
) -> MonochangeResult<GitHubPullRequestOutcome> {
	let existing = lookup_existing_pull_request_with_client(client, request).await?;
	publish_release_pull_request_with_existing_pull_request(client, request, existing.as_ref(), "")
		.await
}

async fn publish_release_pull_request_with_existing_pull_request(
	client: &Octocrab,
	request: &GitHubPullRequestRequest,
	existing: Option<&GitHubExistingPullRequest>,
	head_commit: &str,
) -> MonochangeResult<GitHubPullRequestOutcome> {
	let labels_match = existing.is_some_and(|pull_request| {
		request.labels.iter().all(|label| {
			pull_request
				.labels
				.iter()
				.any(|existing_label| existing_label.name == *label)
		})
	});
	let content_matches = existing.is_some_and(|pull_request| {
		pull_request.title == request.title
			&& pull_request.body.as_deref().unwrap_or_default() == request.body
			&& pull_request.base.ref_name == request.base_branch
	});
	let head_matches_existing =
		existing.and_then(|pull_request| pull_request.head.sha.as_deref()) == Some(head_commit);
	let (operation, pull_request) = match existing {
		Some(existing_pull_request) if content_matches => {
			(
				if head_matches_existing && labels_match && !request.auto_merge {
					GitHubPullRequestOperation::Skipped
				} else {
					GitHubPullRequestOperation::Updated
				},
				GitHubPullRequestResponse {
					number: existing_pull_request.number,
					html_url: existing_pull_request.html_url.clone(),
					node_id: existing_pull_request.node_id.clone(),
				},
			)
		}
		Some(existing_pull_request) => {
			(
				GitHubPullRequestOperation::Updated,
				patch_json::<_, GitHubPullRequestResponse>(
					client,
					&format!(
						"/repos/{}/{}/pulls/{}",
						request.owner, request.repo, existing_pull_request.number
					),
					&GitHubPullRequestUpdatePayload {
						title: &request.title,
						body: &request.body,
						base: &request.base_branch,
					},
				)
				.await?,
			)
		}
		None => {
			(
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
			)
		}
	};
	if !request.labels.is_empty() && !labels_match {
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
) -> MonochangeResult<Option<GitHubExistingPullRequest>> {
	let path = format!(
		"/repos/{}/{}/pulls?state=open&head={}:{}&base={}&per_page=1",
		request.owner,
		request.repo,
		encode(&request.owner),
		encode(&request.head_branch),
		encode(&request.base_branch)
	);
	let pull_requests = get_json::<Vec<GitHubExistingPullRequest>>(client, &path).await?;
	Ok(pull_requests.into_iter().next())
}

fn lookup_existing_pull_request(
	source: &SourceConfiguration,
	request: &GitHubPullRequestRequest,
) -> MonochangeResult<Option<GitHubExistingPullRequest>> {
	let runtime = github_runtime()?;
	runtime.block_on(async {
		let client = github_client_from_env(source)?;
		lookup_existing_pull_request_with_client(&client, request).await
	})
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

fn format_github_api_error(method: &str, path: &str, error: &octocrab::Error) -> String {
	match error {
		octocrab::Error::GitHub { source, .. } => {
			let mut parts = vec![
				format!("status {}", source.status_code.as_u16()),
				source.message.clone(),
			];
			if let Some(documentation_url) = &source.documentation_url {
				parts.push(format!("documentation: {documentation_url}"));
			}
			if let Some(errors) = &source.errors
				&& !errors.is_empty()
			{
				for error in errors {
					parts.push(format!("details: {error}"));
				}
			}
			format!("GitHub API {method} `{path}` failed: {}", parts.join("; "))
		}
		_ => format!("GitHub API {method} `{path}` failed: {error}"),
	}
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
		Err(error) => {
			Err(MonochangeError::Config(format_github_api_error(
				"GET", path, &error,
			)))
		}
	}
}

async fn get_json<T>(client: &Octocrab, path: &str) -> MonochangeResult<T>
where
	T: DeserializeOwned,
{
	match client.get::<T, _, _>(path, None::<&()>).await {
		Ok(value) => Ok(value),
		Err(error) => {
			Err(MonochangeError::Config(format_github_api_error(
				"GET", path, &error,
			)))
		}
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
	client
		.post(path, Some(body))
		.await
		.map_err(|error| MonochangeError::Config(format_github_api_error("POST", path, &error)))
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
	client
		.patch(path, Some(body))
		.await
		.map_err(|error| MonochangeError::Config(format_github_api_error("PATCH", path, &error)))
}

fn join_existing_pull_request_lookup(
	handle: thread::JoinHandle<MonochangeResult<Option<GitHubExistingPullRequest>>>,
) -> MonochangeResult<Option<GitHubExistingPullRequest>> {
	handle.join().map_err(|_| {
		MonochangeError::Config("failed to join GitHub pull request lookup thread".to_string())
	})?
}

fn git_checkout_branch(root: &Path, branch: &str) -> MonochangeResult<()> {
	if matches!(git_current_branch(root).as_deref(), Ok(current) if current == branch) {
		return Ok(());
	}
	run_command(
		git_checkout_branch_command(root, branch),
		"prepare release pull request branch",
	)
}

fn git_stage_paths(root: &Path, tracked_paths: &[PathBuf]) -> MonochangeResult<()> {
	let stageable_paths = resolve_stageable_release_paths(root, tracked_paths)?;
	if stageable_paths.is_empty() {
		return Ok(());
	}
	run_command(
		git_stage_paths_command(root, &stageable_paths),
		"stage release pull request files",
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
		}
	}
	Ok(stageable_paths)
}

fn release_path_requires_staging(root: &Path, path: &Path) -> MonochangeResult<bool> {
	let absolute_path = root.join(path);
	if absolute_path.exists() {
		if git_path_is_tracked(root, path)? {
			return Ok(true);
		}
		return Ok(!git_path_is_ignored(root, path)?);
	}
	git_path_is_tracked(root, path)
}

fn git_path_is_tracked(root: &Path, path: &Path) -> MonochangeResult<bool> {
	let relative = path.to_string_lossy();
	let output = git_command_output(root, &["ls-files", "--error-unmatch", "--", &relative])
		.map_err(|error| {
			MonochangeError::Config(format!(
				"failed to inspect tracked git path {}: {error}",
				path.display()
			))
		})?;
	match output.status.code() {
		Some(0) => Ok(true),
		Some(1) => Ok(false),
		_ => {
			Err(MonochangeError::Config(format!(
				"failed to inspect tracked git path {}: {}",
				path.display(),
				git_error_detail(&output)
			)))
		}
	}
}

fn git_path_is_ignored(root: &Path, path: &Path) -> MonochangeResult<bool> {
	let relative = path.to_string_lossy();
	let output =
		git_command_output(root, &["check-ignore", "-q", "--", &relative]).map_err(|error| {
			MonochangeError::Config(format!(
				"failed to inspect ignored git path {}: {error}",
				path.display()
			))
		})?;
	match output.status.code() {
		Some(0) => Ok(true),
		Some(1) => Ok(false),
		_ => {
			Err(MonochangeError::Config(format!(
				"failed to inspect ignored git path {}: {}",
				path.display(),
				git_error_detail(&output)
			)))
		}
	}
}

fn git_commit_paths(root: &Path, message: &CommitMessage, no_verify: bool) -> MonochangeResult<()> {
	run_git_commit_message(
		root,
		message,
		"commit release pull request changes",
		no_verify,
	)
}

fn git_push_branch(root: &Path, branch: &str, no_verify: bool) -> MonochangeResult<()> {
	run_command(
		git_push_branch_command(root, branch, no_verify),
		"push release pull request branch",
	)
}

fn release_body(
	github: &SourceConfiguration,
	manifest: &ReleaseManifest,
	target: &ReleaseManifestTarget,
) -> Option<String> {
	match github.releases.source {
		ProviderReleaseNotesSource::GitHubGenerated => None,
		ProviderReleaseNotesSource::Monochange => {
			manifest
				.changelogs
				.iter()
				.find(|changelog| {
					changelog.owner_id == target.id && changelog.owner_kind == target.kind
				})
				.map(|changelog| changelog.rendered.clone())
				.or_else(|| Some(minimal_release_body(manifest, target)))
		}
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
				lines.push(format!("### {}", section.title));
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
