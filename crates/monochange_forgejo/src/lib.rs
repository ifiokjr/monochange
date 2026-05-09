//! # `monochange_forgejo`
//!
//! <!-- {=monochangeForgejoCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_forgejo` turns `monochange` release manifests into Forgejo automation requests.
//!
//! Reach for this crate when you want to preview or publish Forgejo releases and release pull requests using the same structured release data that powers changelog files and release manifests.
//!
//! ## Why use it?
//!
//! - derive Forgejo release payloads and release-PR bodies from `monochange`'s structured release manifest
//! - keep Forgejo automation aligned with changelog rendering and release targets
//! - reuse one publishing path for dry-run previews and real repository updates
//!
//! ## Best for
//!
//! - building Forgejo release automation on top of `mc release`
//! - previewing would-be Forgejo releases and release PRs in CI before publishing
//! - self-hosted Forgejo instances that need the same release workflow as GitHub or GitLab
//!
//! ## Public entry points
//!
//! - `build_release_requests(manifest, source)` builds release payloads from prepared release state
//! - `build_change_request(manifest, source)` builds a pull-request payload for the release
//! - `validate_source_configuration(source)` validates Forgejo-specific source config
//! - `source_capabilities()` returns provider feature flags
//! <!-- {/monochangeForgejoCrateDocs} -->

use std::env;
use std::path::Path;
use std::path::PathBuf;

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
use monochange_hosting::git_checkout_branch;
use monochange_hosting::git_commit_paths;
use monochange_hosting::git_push_branch;
use monochange_hosting::git_stage_paths;
use monochange_hosting::release_body;
use monochange_hosting::release_pull_request_body;
use monochange_hosting::release_pull_request_branch;
use reqwest::Client;
use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::runtime::Builder as RuntimeBuilder;
use urlencoding::encode;

/// Return the hosted-source capabilities supported by the Forgejo provider.
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

/// Shared Forgejo hosted-source adapter instance used by the workspace.
pub static HOSTED_SOURCE_ADAPTER: ForgejoHostedSourceAdapter = ForgejoHostedSourceAdapter;

/// Hosted-source adapter for Forgejo repositories.
pub struct ForgejoHostedSourceAdapter;

impl HostedSourceAdapter for ForgejoHostedSourceAdapter {
	fn provider(&self) -> SourceProvider {
		SourceProvider::Forgejo
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

/// Return the hosting metadata features available from Forgejo changeset context.
#[must_use]
pub const fn forgejo_hosting_capabilities() -> HostingCapabilities {
	HostingCapabilities {
		commit_web_urls: true,
		actor_profiles: false,
		review_request_lookup: false,
		related_issues: false,
		issue_comments: false,
	}
}

/// Extract the host name used for rendered Forgejo links.
#[must_use]
pub fn forgejo_host_name(source: &SourceConfiguration) -> Option<String> {
	let host = forgejo_host(source)
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

/// Build a web URL for a commit on the configured Forgejo repository.
#[must_use]
pub fn forgejo_commit_url(source: &SourceConfiguration, sha: &str) -> String {
	format!(
		"{}/{}/{}/commit/{sha}",
		forgejo_host(source).trim_end_matches('/'),
		source.owner,
		source.repo
	)
}

/// Apply Forgejo provider metadata and commit URLs to prepared changesets.
pub fn annotate_changeset_context(
	source: &SourceConfiguration,
	changesets: &mut [PreparedChangeset],
) {
	let host = forgejo_host_name(source);
	let capabilities = forgejo_hosting_capabilities();

	for changeset in changesets {
		let Some(context) = changeset.context.as_mut() else {
			continue;
		};

		context.provider = HostingProviderKind::Forgejo;
		context.host.clone_from(&host);
		context.capabilities = capabilities.clone();

		for revision in [&mut context.introduced, &mut context.last_updated] {
			let Some(revision) = revision.as_mut() else {
				continue;
			};

			if let Some(commit) = revision.commit.as_mut() {
				commit.provider = HostingProviderKind::Forgejo;
				commit.host.clone_from(&host);
				commit.url = Some(forgejo_commit_url(source, &commit.sha));
			}

			if let Some(actor) = revision.actor.as_mut() {
				actor.provider = HostingProviderKind::Forgejo;
				actor.host.clone_from(&host);
			}
		}
	}
}

/// Enrich changeset context for Forgejo-backed workspaces.
///
/// Forgejo currently exposes only local annotations, so this delegates to
/// [`annotate_changeset_context`].
#[tracing::instrument(skip_all)]
pub fn enrich_changeset_context(
	source: &SourceConfiguration,
	changesets: &mut [PreparedChangeset],
) {
	annotate_changeset_context(source, changesets);
}

/// Validate that a source configuration is compatible with the Forgejo provider.
#[must_use = "the validation result must be checked"]
pub fn validate_source_configuration(source: &SourceConfiguration) -> MonochangeResult<()> {
	if source.host.as_deref().is_none_or(str::is_empty) {
		return Err(MonochangeError::Config(
			"[source].host must be set for `provider = \"forgejo\"`".to_string(),
		));
	}
	if source.releases.generate_notes
		|| matches!(
			source.releases.source,
			ProviderReleaseNotesSource::GitHubGenerated
		) {
		return Err(MonochangeError::Config(
			"provider-generated release notes are not supported for `provider = \"forgejo\"`; use `source = \"monochange\"`"
				.to_string(),
		));
	}
	if source.pull_requests.auto_merge {
		return Err(MonochangeError::Config(
			"[source.pull_requests].auto_merge is not supported for `provider = \"forgejo\"`"
				.to_string(),
		));
	}
	Ok(())
}

#[derive(Debug, Serialize)]
struct ForgejoReleasePayload<'a> {
	tag_name: &'a str,
	name: &'a str,
	body: Option<&'a str>,
	draft: bool,
	prerelease: bool,
	target_commitish: &'a str,
}

#[derive(Debug, Serialize)]
struct ForgejoPullRequestPayload<'a> {
	title: &'a str,
	head: &'a str,
	base: &'a str,
	body: &'a str,
}

#[derive(Debug, Serialize)]
struct ForgejoPullRequestUpdatePayload<'a> {
	title: &'a str,
	body: &'a str,
	base: &'a str,
}

#[derive(Debug, Serialize)]
struct ForgejoLabelsPayload<'a> {
	labels: &'a [String],
}

#[derive(Debug, Deserialize)]
struct ForgejoReleaseResponse {
	html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ForgejoPullRequestResponse {
	number: u64,
	html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ForgejoExistingPullRequestLabel {
	name: String,
}

#[derive(Debug, Deserialize)]
struct ForgejoExistingPullRequestBase {
	#[serde(rename = "ref")]
	ref_name: String,
}

#[derive(Debug, Deserialize)]
struct ForgejoExistingPullRequestHead {
	sha: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ForgejoExistingPullRequest {
	number: u64,
	html_url: Option<String>,
	title: String,
	body: Option<String>,
	base: ForgejoExistingPullRequestBase,
	head: ForgejoExistingPullRequestHead,
	#[serde(default)]
	labels: Vec<ForgejoExistingPullRequestLabel>,
}

fn forgejo_host(source: &SourceConfiguration) -> &str {
	source
		.host
		.as_deref()
		.unwrap_or("https://forgejo.com")
		.trim_end_matches('/')
}

/// Build the public release URL for a tag on the configured Forgejo repository.
#[must_use]
pub fn tag_url(source: &SourceConfiguration, tag_name: &str) -> String {
	let host = forgejo_host(source);
	format!(
		"{host}/{}/{}/releases/tag/{tag_name}",
		source.owner, source.repo
	)
}

/// Build the comparison URL between two tags on the configured Forgejo repository.
#[must_use]
pub fn compare_url(source: &SourceConfiguration, previous_tag: &str, current_tag: &str) -> String {
	let host = forgejo_host(source);
	format!(
		"{host}/{}/{}/compare/{previous_tag}...{current_tag}",
		source.owner, source.repo
	)
}

/// Convert releasable targets into provider-specific Forgejo release requests.
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
				provider: SourceProvider::Forgejo,
				repository: format!("{}/{}", source.owner, source.repo),
				owner: source.owner.clone(),
				repo: source.repo.clone(),
				target_id: target.id.clone(),
				target_kind: target.kind,
				tag_name: target.tag_name.clone(),
				name: if target.rendered_title.is_empty() {
					target.tag_name.clone()
				} else {
					target.rendered_title.clone()
				},
				body: release_body(source, manifest, target),
				draft: source.releases.draft,
				prerelease: source.releases.prerelease,
				generate_release_notes: source.releases.generate_notes,
			}
		})
		.collect()
}

/// Build the release pull request request for the configured Forgejo repository.
#[must_use]
pub fn build_release_pull_request_request(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> SourceChangeRequest {
	let repository = format!("{}/{}", source.owner, source.repo);
	let title = source.pull_requests.title.clone();
	SourceChangeRequest {
		provider: SourceProvider::Forgejo,
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

/// Publish or update all planned Forgejo releases for a manifest.
#[tracing::instrument(skip_all)]
#[must_use = "the publish result must be checked"]
#[allow(clippy::disallowed_methods)]
pub fn publish_release_requests(
	source: &SourceConfiguration,
	requests: &[SourceReleaseRequest],
) -> MonochangeResult<Vec<SourceReleaseOutcome>> {
	let runtime = RuntimeBuilder::new_current_thread()
		.enable_all()
		.build()
		.expect("failed to build Forgejo runtime");
	runtime.block_on(async {
		let client = Client::builder()
			.build()
			.expect("failed to build Forgejo HTTP client");
		let token = forgejo_token()?;
		let headers = auth_headers(&token)?;
		let api_base = forgejo_api_base(source)?;
		let mut outcomes = Vec::with_capacity(requests.len());
		for request in requests {
			outcomes.push(
				publish_release_request(&client, &headers, &api_base, source, request).await?,
			);
		}
		Ok(outcomes)
	})
}

/// Commit, push, and publish the release pull request against Forgejo.
#[must_use = "the pull request result must be checked"]
#[allow(clippy::disallowed_methods)]
pub async fn publish_release_pull_request(
	source: &SourceConfiguration,
	root: &Path,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
	no_verify: bool,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	let lookup_source = source.clone();
	let lookup_request = request.clone();
	let existing_pull_request = std::thread::spawn(move || {
		let runtime = RuntimeBuilder::new_current_thread()
			.enable_all()
			.build()
			.expect("failed to build Forgejo runtime");
		runtime.block_on(async {
			let client = Client::builder()
				.build()
				.expect("failed to build Forgejo HTTP client");
			let token = forgejo_token()?;
			let headers = auth_headers(&token)?;
			let api_base = forgejo_api_base(&lookup_source)?;
			lookup_existing_pull_request(&client, &headers, &api_base, &lookup_request).await
		})
	});
	git_checkout_branch(
		root,
		&request.head_branch,
		"prepare release pull request branch",
	).await?;
	git_stage_paths(root, tracked_paths, "stage release pull request files").await?;
	git_commit_paths(
		root,
		&request.commit_message,
		"commit release pull request changes",
		no_verify,
	).await?;
	let head_commit = git_head_commit(root).await?;
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
		).await?;
	}

	let client = Client::builder()
		.build()
		.expect("failed to build Forgejo HTTP client");
	let token = forgejo_token()?;
	let headers = auth_headers(&token)?;
	let api_base = forgejo_api_base(source)?;
	publish_pull_request_with_existing(
		&client,
		&headers,
		&api_base,
		request,
		existing.as_ref(),
		&head_commit,
	)
	.await
}

async fn publish_release_request(
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
		get_optional_json::<ForgejoReleaseResponse>(client, headers, &lookup_url, "Forgejo")
			.await?;
	let response: ForgejoReleaseResponse = if existing.is_some() {
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
			&ForgejoReleasePayload {
				tag_name: &request.tag_name,
				name: &request.name,
				body: request.body.as_deref(),
				draft: request.draft,
				prerelease: request.prerelease,
				target_commitish: &source.pull_requests.base,
			},
			"Forgejo",
		)
		.await?
	} else {
		let create_url = format!(
			"{api_base}/repos/{}/{}/releases",
			request.owner, request.repo
		);
		post_json(
			client,
			headers,
			&create_url,
			&ForgejoReleasePayload {
				tag_name: &request.tag_name,
				name: &request.name,
				body: request.body.as_deref(),
				draft: request.draft,
				prerelease: request.prerelease,
				target_commitish: &source.pull_requests.base,
			},
			"Forgejo",
		)
		.await?
	};
	Ok(SourceReleaseOutcome {
		provider: SourceProvider::Forgejo,
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
async fn publish_pull_request(
	client: &Client,
	headers: &HeaderMap,
	api_base: &str,
	request: &SourceChangeRequest,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	let existing = lookup_existing_pull_request(client, headers, api_base, request).await?;
	publish_pull_request_with_existing(client, headers, api_base, request, existing.as_ref(), "")
		.await
}

async fn publish_pull_request_with_existing(
	client: &Client,
	headers: &HeaderMap,
	api_base: &str,
	request: &SourceChangeRequest,
	existing: Option<&ForgejoExistingPullRequest>,
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
	let response: ForgejoPullRequestResponse = match existing {
		Some(existing_pr) if content_matches => {
			ForgejoPullRequestResponse {
				number: existing_pr.number,
				html_url: existing_pr.html_url.clone(),
			}
		}
		Some(existing_pr) => {
			let update_url = format!(
				"{api_base}/repos/{}/{}/pulls/{}",
				request.owner, request.repo, existing_pr.number
			);
			let update_payload = ForgejoPullRequestUpdatePayload {
				title: &request.title,
				body: &request.body,
				base: &request.base_branch,
			};

			patch_json(client, headers, &update_url, &update_payload, "Forgejo").await?
		}
		None => {
			let create_url = format!("{api_base}/repos/{}/{}/pulls", request.owner, request.repo);
			let payload = ForgejoPullRequestPayload {
				title: &request.title,
				head: &request.head_branch,
				base: &request.base_branch,
				body: &request.body,
			};

			post_json(client, headers, &create_url, &payload, "Forgejo").await?
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
			&ForgejoLabelsPayload {
				labels: &request.labels,
			},
			"Forgejo",
		)
		.await?;
	}
	Ok(SourceChangeRequestOutcome {
		provider: SourceProvider::Forgejo,
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

async fn lookup_existing_pull_request(
	client: &Client,
	headers: &HeaderMap,
	api_base: &str,
	request: &SourceChangeRequest,
) -> MonochangeResult<Option<ForgejoExistingPullRequest>> {
	let list_url = format!(
		"{api_base}/repos/{}/{}/pulls?state=open&head={}:{}&base={}",
		request.owner,
		request.repo,
		encode(&request.owner),
		encode(&request.head_branch),
		encode(&request.base_branch),
	);
	Ok(
		get_json::<Vec<ForgejoExistingPullRequest>>(client, headers, &list_url, "Forgejo")
			.await?
			.into_iter()
			.next(),
	)
}

fn join_existing_pull_request_lookup(
	handle: std::thread::JoinHandle<MonochangeResult<Option<ForgejoExistingPullRequest>>>,
) -> MonochangeResult<Option<ForgejoExistingPullRequest>> {
	handle.join().map_err(|_| {
		MonochangeError::Config("failed to join Forgejo pull request lookup thread".to_string())
	})?
}

async fn get_optional_json<T>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	provider: &str,
) -> MonochangeResult<Option<T>>
where
	T: DeserializeOwned,
{
	let response = client
		.get(url)
		.headers(headers.clone())
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
		})?;
	let status = response.status();
	if status == StatusCode::NOT_FOUND {
		return Ok(None);
	}
	if !status.is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API GET `{url}` failed with status {status}"
		)));
	}
	response.json::<T>().await.map(Some).map_err(|error| {
		MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
	})
}
async fn get_json<T>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	provider: &str,
) -> MonochangeResult<T>
where
	T: DeserializeOwned,
{
	let response = client
		.get(url)
		.headers(headers.clone())
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
		})?;
	let status = response.status();
	if !status.is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API GET `{url}` failed with status {status}"
		)));
	}
	response.json::<T>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
	})
}

async fn post_json<Body, JsonResponse>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	body: &Body,
	provider: &str,
) -> MonochangeResult<JsonResponse>
where
	Body: Serialize + ?Sized,
	JsonResponse: DeserializeOwned,
{
	let response = client
		.post(url)
		.headers(headers.clone())
		.json(body)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API POST `{url}` failed: {error}"))
		})?;
	let status = response.status();
	if !status.is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API POST `{url}` failed with status {status}"
		)));
	}
	response.json::<JsonResponse>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API POST `{url}` failed: {error}"))
	})
}

async fn patch_json<Body, JsonResponse>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	body: &Body,
	provider: &str,
) -> MonochangeResult<JsonResponse>
where
	Body: Serialize + ?Sized,
	JsonResponse: DeserializeOwned,
{
	let response = client
		.patch(url)
		.headers(headers.clone())
		.json(body)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API PATCH `{url}` failed: {error}"))
		})?;
	let status = response.status();
	if !status.is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API PATCH `{url}` failed with status {status}"
		)));
	}
	response.json::<JsonResponse>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API PATCH `{url}` failed: {error}"))
	})
}

fn forgejo_token() -> MonochangeResult<String> {
	env::var("FORGEJO_TOKEN").map_err(|_| {
		MonochangeError::Config("set `FORGEJO_TOKEN` before running Forgejo automation".to_string())
	})
}

fn forgejo_api_base(source: &SourceConfiguration) -> MonochangeResult<String> {
	if let Some(api_url) = &source.api_url {
		return Ok(api_url.trim_end_matches('/').to_string());
	}
	let host = source.host.as_deref().ok_or_else(|| {
		MonochangeError::Config(
			"[source].host must be set for `provider = \"forgejo\"`".to_string(),
		)
	})?;
	Ok(format!("{}/api/v1", host.trim_end_matches('/')))
}

fn auth_headers(token: &str) -> MonochangeResult<HeaderMap> {
	let mut headers = HeaderMap::new();
	headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
	headers.insert(
		AUTHORIZATION,
		HeaderValue::from_str(&format!("token {token}")).map_err(|error| {
			MonochangeError::Config(format!("invalid Forgejo token header value: {error}"))
		})?,
	);
	Ok(headers)
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
