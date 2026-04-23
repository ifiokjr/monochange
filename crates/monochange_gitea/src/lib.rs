#![forbid(clippy::indexing_slicing)]

//! # `monochange_gitea`
//!
//! <!-- {=monochangeGiteaCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_gitea` turns `monochange` release manifests into Gitea automation requests.
//!
//! Reach for this crate when you want to preview or publish Gitea releases and release pull requests using the same structured release data that powers changelog files and release manifests.
//!
//! ## Why use it?
//!
//! - derive Gitea release payloads and release-PR bodies from `monochange`'s structured release manifest
//! - keep Gitea automation aligned with changelog rendering and release targets
//! - reuse one publishing path for dry-run previews and real repository updates
//!
//! ## Best for
//!
//! - building Gitea release automation on top of `mc release`
//! - previewing would-be Gitea releases and release PRs in CI before publishing
//! - self-hosted Gitea instances that need the same release workflow as GitHub or GitLab
//!
//! ## Public entry points
//!
//! - `build_release_requests(manifest, source)` builds release payloads from prepared release state
//! - `build_change_request(manifest, source)` builds a pull-request payload for the release
//! - `validate_source_configuration(source)` validates Gitea-specific source config
//! - `source_capabilities()` returns provider feature flags
//! <!-- {/monochangeGiteaCrateDocs} -->

use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::thread;

use monochange_core::CommitMessage;
use monochange_core::HostedSourceAdapter;
use monochange_core::HostedSourceFeatures;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PreparedChangeset;
use monochange_core::ProviderReleaseNotesSource;
use monochange_core::ReleaseManifest;
use monochange_core::SourceCapabilities;
use monochange_core::SourceChangeRequest;
use monochange_core::SourceChangeRequestOperation;
use monochange_core::SourceChangeRequestOutcome;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::SourceReleaseOperation;
use monochange_core::SourceReleaseOutcome;
use monochange_core::SourceReleaseRequest;
use monochange_core::git::git_head_commit;
use monochange_hosting::build_http_client;
use monochange_hosting::get_json;
use monochange_hosting::get_optional_json;
use monochange_hosting::git_checkout_branch;
use monochange_hosting::git_commit_paths;
use monochange_hosting::git_push_branch;
use monochange_hosting::git_stage_paths;
use monochange_hosting::patch_json;
use monochange_hosting::post_json;
use monochange_hosting::release_body;
use monochange_hosting::release_pull_request_body;
use monochange_hosting::release_pull_request_branch;
use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde::Serialize;
use urlencoding::encode;

/// Return the hosted-source capabilities supported by the Gitea provider.
#[must_use]
pub const fn source_capabilities() -> SourceCapabilities {
	SourceCapabilities {
		draft_releases: true,
		prereleases: true,
		generated_release_notes: false,
		auto_merge_change_requests: false,
		released_issue_comments: false,
		requires_host: true,
	}
}

/// Shared Gitea hosted-source adapter instance used by the workspace.
pub static HOSTED_SOURCE_ADAPTER: GiteaHostedSourceAdapter = GiteaHostedSourceAdapter;

/// Hosted-source adapter for Gitea repositories.
pub struct GiteaHostedSourceAdapter;

impl HostedSourceAdapter for GiteaHostedSourceAdapter {
	fn provider(&self) -> SourceProvider {
		SourceProvider::Gitea
	}

	fn features(&self) -> HostedSourceFeatures {
		HostedSourceFeatures {
			batched_changeset_context_lookup: false,
			released_issue_comments: false,
			release_retarget_sync: false,
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
}

/// Return the hosting metadata features available from Gitea changeset context.
#[must_use]
pub const fn gitea_hosting_capabilities() -> HostingCapabilities {
	HostingCapabilities {
		commit_web_urls: true,
		actor_profiles: false,
		review_request_lookup: false,
		related_issues: false,
		issue_comments: false,
	}
}

/// Extract the host name used for rendered Gitea links.
#[must_use]
pub fn gitea_host_name(source: &SourceConfiguration) -> Option<String> {
	let host = gitea_host(source)
		.trim_start_matches("https://")
		.trim_start_matches("http://")
		.split('/')
		.next()
		.unwrap_or_default()
		.trim();
	if host.is_empty() {
		None
	} else {
		Some(host.to_string())
	}
}

/// Build a web URL for a commit on the configured Gitea repository.
#[must_use]
pub fn gitea_commit_url(source: &SourceConfiguration, sha: &str) -> String {
	format!(
		"{}/{}/{}/commit/{sha}",
		gitea_host(source).trim_end_matches('/'),
		source.owner,
		source.repo
	)
}

/// Apply Gitea provider metadata and commit URLs to prepared changesets.
pub fn annotate_changeset_context(
	source: &SourceConfiguration,
	changesets: &mut [PreparedChangeset],
) {
	let host = gitea_host_name(source);
	let capabilities = gitea_hosting_capabilities();

	for changeset in changesets {
		let Some(context) = changeset.context.as_mut() else {
			continue;
		};

		context.provider = HostingProviderKind::Gitea;
		context.host.clone_from(&host);
		context.capabilities = capabilities.clone();

		for revision in [&mut context.introduced, &mut context.last_updated] {
			let Some(revision) = revision.as_mut() else {
				continue;
			};

			if let Some(commit) = revision.commit.as_mut() {
				commit.provider = HostingProviderKind::Gitea;
				commit.host.clone_from(&host);
				commit.url = Some(gitea_commit_url(source, &commit.sha));
			}

			if let Some(actor) = revision.actor.as_mut() {
				actor.provider = HostingProviderKind::Gitea;
				actor.host.clone_from(&host);
			}
		}
	}
}

/// Enrich changeset context for Gitea-backed workspaces.
///
/// Gitea currently exposes only local annotations, so this delegates to
/// [`annotate_changeset_context`].
#[tracing::instrument(skip_all)]
pub fn enrich_changeset_context(
	source: &SourceConfiguration,
	changesets: &mut [PreparedChangeset],
) {
	annotate_changeset_context(source, changesets);
}

/// Validate that a source configuration is compatible with the Gitea provider.
#[must_use = "the validation result must be checked"]
pub fn validate_source_configuration(source: &SourceConfiguration) -> MonochangeResult<()> {
	if source.host.as_deref().is_none_or(str::is_empty) {
		return Err(MonochangeError::Config(
			"[source].host must be set for `provider = \"gitea\"`".to_string(),
		));
	}
	if source.releases.generate_notes
		|| matches!(
			source.releases.source,
			ProviderReleaseNotesSource::GitHubGenerated
		) {
		return Err(MonochangeError::Config(
			"provider-generated release notes are not supported for `provider = \"gitea\"`; use `source = \"monochange\"`"
				.to_string(),
		));
	}
	if source.pull_requests.auto_merge {
		return Err(MonochangeError::Config(
			"[source.pull_requests].auto_merge is not supported for `provider = \"gitea\"`"
				.to_string(),
		));
	}
	Ok(())
}

#[derive(Debug, Serialize)]
struct GiteaReleasePayload<'a> {
	tag_name: &'a str,
	name: &'a str,
	body: Option<&'a str>,
	draft: bool,
	prerelease: bool,
	target_commitish: &'a str,
}

#[derive(Debug, Serialize)]
struct GiteaPullRequestPayload<'a> {
	title: &'a str,
	head: &'a str,
	base: &'a str,
	body: &'a str,
}

#[derive(Debug, Serialize)]
struct GiteaPullRequestUpdatePayload<'a> {
	title: &'a str,
	body: &'a str,
	base: &'a str,
}

#[derive(Debug, Serialize)]
struct GiteaLabelsPayload<'a> {
	labels: &'a [String],
}

#[derive(Debug, Deserialize)]
struct GiteaReleaseResponse {
	html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GiteaPullRequestResponse {
	number: u64,
	html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GiteaExistingPullRequestLabel {
	name: String,
}

#[derive(Debug, Deserialize)]
struct GiteaExistingPullRequestBase {
	#[serde(rename = "ref")]
	ref_name: String,
}

#[derive(Debug, Deserialize)]
struct GiteaExistingPullRequestHead {
	sha: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GiteaExistingPullRequest {
	number: u64,
	html_url: Option<String>,
	title: String,
	body: Option<String>,
	base: GiteaExistingPullRequestBase,
	head: GiteaExistingPullRequestHead,
	#[serde(default)]
	labels: Vec<GiteaExistingPullRequestLabel>,
}

fn gitea_host(source: &SourceConfiguration) -> &str {
	source
		.host
		.as_deref()
		.unwrap_or("https://gitea.com")
		.trim_end_matches('/')
}

/// Build the public release URL for a tag on the configured Gitea repository.
#[must_use]
pub fn tag_url(source: &SourceConfiguration, tag_name: &str) -> String {
	let host = gitea_host(source);
	format!(
		"{host}/{}/{}/releases/tag/{tag_name}",
		source.owner, source.repo
	)
}

/// Build the comparison URL between two tags on the configured Gitea repository.
#[must_use]
pub fn compare_url(source: &SourceConfiguration, previous_tag: &str, current_tag: &str) -> String {
	let host = gitea_host(source);
	format!(
		"{host}/{}/{}/compare/{previous_tag}...{current_tag}",
		source.owner, source.repo
	)
}

/// Convert releasable targets into provider-specific Gitea release requests.
#[must_use]
pub fn build_release_requests(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> Vec<SourceReleaseRequest> {
	manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
		.map(|target| {
			SourceReleaseRequest {
				provider: SourceProvider::Gitea,
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

/// Build the release pull request request for the configured Gitea repository.
#[must_use]
pub fn build_release_pull_request_request(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> SourceChangeRequest {
	let repository = format!("{}/{}", source.owner, source.repo);
	let title = source.pull_requests.title.clone();
	SourceChangeRequest {
		provider: SourceProvider::Gitea,
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

/// Publish or update all planned Gitea releases for a manifest.
#[tracing::instrument(skip_all)]
#[must_use = "the publish result must be checked"]
pub fn publish_release_requests(
	source: &SourceConfiguration,
	requests: &[SourceReleaseRequest],
) -> MonochangeResult<Vec<SourceReleaseOutcome>> {
	let client = build_http_client("Gitea")?;
	let token = gitea_token()?;
	let headers = auth_headers(&token)?;
	let api_base = gitea_api_base(source)?;
	requests
		.iter()
		.map(|request| publish_release_request(&client, &headers, &api_base, source, request))
		.collect()
}

/// Commit, push, and publish the release pull request against Gitea.
#[must_use = "the pull request result must be checked"]
pub fn publish_release_pull_request(
	source: &SourceConfiguration,
	root: &Path,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
	no_verify: bool,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	let lookup_source = source.clone();
	let lookup_request = request.clone();
	let existing_pull_request = thread::spawn(move || {
		let client = build_http_client("Gitea")?;
		let token = gitea_token()?;
		let headers = auth_headers(&token)?;
		let api_base = gitea_api_base(&lookup_source)?;
		lookup_existing_pull_request(&client, &headers, &api_base, &lookup_request)
	});
	git_checkout_branch(
		root,
		&request.head_branch,
		"prepare release pull request branch",
	)?;
	git_stage_paths(root, tracked_paths, "stage release pull request files")?;
	git_commit_paths(
		root,
		&request.commit_message,
		"commit release pull request changes",
		no_verify,
	)?;
	let head_commit = git_head_commit(root)?;
	let existing = join_existing_pull_request_lookup(existing_pull_request)?;
	let head_matches_existing = existing
		.as_ref()
		.and_then(|pull_request| pull_request.head.sha.as_deref())
		== Some(head_commit.as_str());
	if !head_matches_existing {
		git_push_branch(
			root,
			&request.head_branch,
			"push release pull request branch",
			no_verify,
		)?;
	}

	let client = build_http_client("Gitea")?;
	let token = gitea_token()?;
	let headers = auth_headers(&token)?;
	let api_base = gitea_api_base(source)?;
	publish_pull_request_with_existing(
		&client,
		&headers,
		&api_base,
		request,
		existing.as_ref(),
		&head_commit,
	)
}

fn publish_release_request(
	client: &Client,
	headers: &HeaderMap,
	api_base: &str,
	source: &SourceConfiguration,
	request: &SourceReleaseRequest,
) -> MonochangeResult<SourceReleaseOutcome> {
	let lookup_url = format!(
		"{api_base}/repos/{}/{}/releases/tags/{}",
		request.owner,
		request.repo,
		encode(&request.tag_name)
	);
	let existing =
		get_optional_json::<GiteaReleaseResponse>(client, headers, &lookup_url, "Gitea")?;
	let response: GiteaReleaseResponse = if existing.is_some() {
		let update_url = format!(
			"{api_base}/repos/{}/{}/releases/tags/{}",
			request.owner,
			request.repo,
			encode(&request.tag_name)
		);
		patch_json(
			client,
			headers,
			&update_url,
			&GiteaReleasePayload {
				tag_name: &request.tag_name,
				name: &request.name,
				body: request.body.as_deref(),
				draft: request.draft,
				prerelease: request.prerelease,
				target_commitish: &source.pull_requests.base,
			},
			"Gitea",
		)?
	} else {
		let create_url = format!(
			"{api_base}/repos/{}/{}/releases",
			request.owner, request.repo
		);
		post_json(
			client,
			headers,
			&create_url,
			&GiteaReleasePayload {
				tag_name: &request.tag_name,
				name: &request.name,
				body: request.body.as_deref(),
				draft: request.draft,
				prerelease: request.prerelease,
				target_commitish: &source.pull_requests.base,
			},
			"Gitea",
		)?
	};
	Ok(SourceReleaseOutcome {
		provider: SourceProvider::Gitea,
		repository: request.repository.clone(),
		tag_name: request.tag_name.clone(),
		operation: if existing.is_some() {
			SourceReleaseOperation::Updated
		} else {
			SourceReleaseOperation::Created
		},
		url: response.html_url,
	})
}

#[cfg_attr(not(test), allow(dead_code))]
fn publish_pull_request(
	client: &Client,
	headers: &HeaderMap,
	api_base: &str,
	request: &SourceChangeRequest,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	let existing = lookup_existing_pull_request(client, headers, api_base, request)?;
	publish_pull_request_with_existing(client, headers, api_base, request, existing.as_ref(), "")
}

fn publish_pull_request_with_existing(
	client: &Client,
	headers: &HeaderMap,
	api_base: &str,
	request: &SourceChangeRequest,
	existing: Option<&GiteaExistingPullRequest>,
	head_commit: &str,
) -> MonochangeResult<SourceChangeRequestOutcome> {
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
	let response: GiteaPullRequestResponse = match existing {
		Some(existing_pr) if content_matches => {
			GiteaPullRequestResponse {
				number: existing_pr.number,
				html_url: existing_pr.html_url.clone(),
			}
		}
		Some(existing_pr) => {
			let update_url = format!(
				"{api_base}/repos/{}/{}/pulls/{}",
				request.owner, request.repo, existing_pr.number
			);
			let update_payload = GiteaPullRequestUpdatePayload {
				title: &request.title,
				body: &request.body,
				base: &request.base_branch,
			};

			patch_json(client, headers, &update_url, &update_payload, "Gitea")?
		}
		None => {
			let create_url = format!("{api_base}/repos/{}/{}/pulls", request.owner, request.repo);
			let payload = GiteaPullRequestPayload {
				title: &request.title,
				head: &request.head_branch,
				base: &request.base_branch,
				body: &request.body,
			};

			post_json(client, headers, &create_url, &payload, "Gitea")?
		}
	};
	if !request.labels.is_empty() && !labels_match {
		let labels_url = format!(
			"{api_base}/repos/{}/{}/issues/{}/labels",
			request.owner, request.repo, response.number
		);
		let _: serde_json::Value = post_json(
			client,
			headers,
			&labels_url,
			&GiteaLabelsPayload {
				labels: &request.labels,
			},
			"Gitea",
		)?;
	}
	Ok(SourceChangeRequestOutcome {
		provider: SourceProvider::Gitea,
		repository: request.repository.clone(),
		number: response.number,
		head_branch: request.head_branch.clone(),
		operation: match existing {
			None => SourceChangeRequestOperation::Created,
			Some(_) if content_matches && labels_match && head_matches_existing => {
				SourceChangeRequestOperation::Skipped
			}
			Some(_) => SourceChangeRequestOperation::Updated,
		},
		url: response.html_url,
	})
}

fn lookup_existing_pull_request(
	client: &Client,
	headers: &HeaderMap,
	api_base: &str,
	request: &SourceChangeRequest,
) -> MonochangeResult<Option<GiteaExistingPullRequest>> {
	let list_url = format!(
		"{api_base}/repos/{}/{}/pulls?state=open&head={}:{}&base={}",
		request.owner,
		request.repo,
		encode(&request.owner),
		encode(&request.head_branch),
		encode(&request.base_branch),
	);
	Ok(
		get_json::<Vec<GiteaExistingPullRequest>>(client, headers, &list_url, "Gitea")?
			.into_iter()
			.next(),
	)
}

fn join_existing_pull_request_lookup(
	handle: thread::JoinHandle<MonochangeResult<Option<GiteaExistingPullRequest>>>,
) -> MonochangeResult<Option<GiteaExistingPullRequest>> {
	handle.join().map_err(|_| {
		MonochangeError::Config("failed to join Gitea pull request lookup thread".to_string())
	})?
}

#[cfg(test)]
fn gitea_client() -> MonochangeResult<Client> {
	build_http_client("Gitea")
}

fn gitea_token() -> MonochangeResult<String> {
	env::var("GITEA_TOKEN").map_err(|_| {
		MonochangeError::Config("set `GITEA_TOKEN` before running Gitea automation".to_string())
	})
}

fn gitea_api_base(source: &SourceConfiguration) -> MonochangeResult<String> {
	if let Some(api_url) = &source.api_url {
		return Ok(api_url.trim_end_matches('/').to_string());
	}
	let host = source.host.as_deref().ok_or_else(|| {
		MonochangeError::Config("[source].host must be set for `provider = \"gitea\"`".to_string())
	})?;
	Ok(format!("{}/api/v1", host.trim_end_matches('/')))
}

fn auth_headers(token: &str) -> MonochangeResult<HeaderMap> {
	let mut headers = HeaderMap::new();
	headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
	headers.insert(
		AUTHORIZATION,
		HeaderValue::from_str(&format!("token {token}")).map_err(|error| {
			MonochangeError::Config(format!("invalid Gitea token header value: {error}"))
		})?,
	);
	Ok(headers)
}

#[cfg(test)]
mod __tests;
