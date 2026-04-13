#![forbid(clippy::indexing_slicing)]

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
use monochange_hosting::put_json;
use monochange_hosting::release_body;
use monochange_hosting::release_pull_request_body;
use monochange_hosting::release_pull_request_branch;
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde::Serialize;
use urlencoding::encode;

#[must_use]
pub const fn source_capabilities() -> SourceCapabilities {
	SourceCapabilities {
		draft_releases: false,
		prereleases: false,
		generated_release_notes: false,
		auto_merge_change_requests: false,
		released_issue_comments: false,
		requires_host: false,
	}
}

pub static HOSTED_SOURCE_ADAPTER: GitLabHostedSourceAdapter = GitLabHostedSourceAdapter;

pub struct GitLabHostedSourceAdapter;

impl HostedSourceAdapter for GitLabHostedSourceAdapter {
	fn provider(&self) -> SourceProvider {
		SourceProvider::GitLab
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

#[must_use]
pub const fn gitlab_hosting_capabilities() -> HostingCapabilities {
	HostingCapabilities {
		commit_web_urls: true,
		actor_profiles: false,
		review_request_lookup: false,
		related_issues: false,
		issue_comments: false,
	}
}

#[must_use]
pub fn gitlab_host_name(source: &SourceConfiguration) -> Option<String> {
	let host = gitlab_host(source)
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

#[must_use]
pub fn gitlab_commit_url(source: &SourceConfiguration, sha: &str) -> String {
	format!(
		"{}/{}/{}/-/commit/{sha}",
		gitlab_host(source).trim_end_matches('/'),
		source.owner,
		source.repo
	)
}

pub fn annotate_changeset_context(
	source: &SourceConfiguration,
	changesets: &mut [PreparedChangeset],
) {
	let host = gitlab_host_name(source);
	let capabilities = gitlab_hosting_capabilities();
	for changeset in changesets {
		let Some(context) = changeset.context.as_mut() else {
			continue;
		};
		context.provider = HostingProviderKind::GitLab;
		context.host.clone_from(&host);
		context.capabilities = capabilities.clone();
		for revision in [&mut context.introduced, &mut context.last_updated] {
			let Some(revision) = revision.as_mut() else {
				continue;
			};
			if let Some(commit) = revision.commit.as_mut() {
				commit.provider = HostingProviderKind::GitLab;
				commit.host.clone_from(&host);
				commit.url = Some(gitlab_commit_url(source, &commit.sha));
			}
			if let Some(actor) = revision.actor.as_mut() {
				actor.provider = HostingProviderKind::GitLab;
				actor.host.clone_from(&host);
			}
		}
	}
}

#[tracing::instrument(skip_all)]
pub fn enrich_changeset_context(
	source: &SourceConfiguration,
	changesets: &mut [PreparedChangeset],
) {
	annotate_changeset_context(source, changesets);
}

#[must_use = "the validation result must be checked"]
pub fn validate_source_configuration(source: &SourceConfiguration) -> MonochangeResult<()> {
	if source.releases.draft {
		return Err(MonochangeError::Config(
			"[source.releases].draft is not supported for `provider = \"gitlab\"`".to_string(),
		));
	}
	if source.releases.prerelease {
		return Err(MonochangeError::Config(
			"[source.releases].prerelease is not supported for `provider = \"gitlab\"`".to_string(),
		));
	}
	if source.releases.generate_notes
		|| matches!(
			source.releases.source,
			ProviderReleaseNotesSource::GitHubGenerated
		) {
		return Err(MonochangeError::Config(
			"provider-generated release notes are not supported for `provider = \"gitlab\"`; use `source = \"monochange\"`"
				.to_string(),
		));
	}
	if source.pull_requests.auto_merge {
		return Err(MonochangeError::Config(
			"[source.pull_requests].auto_merge is not supported for `provider = \"gitlab\"`"
				.to_string(),
		));
	}
	Ok(())
}

#[derive(Debug, Serialize)]
struct GitLabReleaseCreatePayload<'a> {
	name: &'a str,
	tag_name: &'a str,
	description: Option<&'a str>,
	#[serde(rename = "ref")]
	ref_: &'a str,
}

#[derive(Debug, Serialize)]
struct GitLabReleaseUpdatePayload<'a> {
	name: &'a str,
	description: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct GitLabMergeRequestPayload<'a> {
	title: &'a str,
	source_branch: &'a str,
	target_branch: &'a str,
	description: &'a str,
	labels: &'a str,
}

#[derive(Debug, Serialize)]
struct GitLabMergeRequestUpdatePayload<'a> {
	title: &'a str,
	target_branch: &'a str,
	description: &'a str,
	labels: &'a str,
}

#[derive(Debug, Deserialize)]
struct GitLabReleaseResponse {
	web_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitLabMergeRequestResponse {
	iid: u64,
	web_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitLabExistingMergeRequest {
	iid: u64,
	web_url: Option<String>,
	title: String,
	description: Option<String>,
	target_branch: String,
	#[serde(default)]
	labels: Vec<String>,
	sha: Option<String>,
}

fn gitlab_host(source: &SourceConfiguration) -> &str {
	source
		.host
		.as_deref()
		.unwrap_or("https://gitlab.com")
		.trim_end_matches('/')
}

#[must_use]
pub fn tag_url(source: &SourceConfiguration, tag_name: &str) -> String {
	let host = gitlab_host(source);
	format!(
		"{host}/{}/{}/-/releases/{tag_name}",
		source.owner, source.repo
	)
}

#[must_use]
pub fn compare_url(source: &SourceConfiguration, previous_tag: &str, current_tag: &str) -> String {
	let host = gitlab_host(source);
	format!(
		"{host}/{}/{}/-/compare/{previous_tag}...{current_tag}",
		source.owner, source.repo
	)
}

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
				provider: SourceProvider::GitLab,
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

#[must_use]
pub fn build_release_pull_request_request(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> SourceChangeRequest {
	let repository = format!("{}/{}", source.owner, source.repo);
	let title = source.pull_requests.title.clone();
	SourceChangeRequest {
		provider: SourceProvider::GitLab,
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

#[tracing::instrument(skip_all)]
#[must_use = "the publish result must be checked"]
pub fn publish_release_requests(
	source: &SourceConfiguration,
	requests: &[SourceReleaseRequest],
) -> MonochangeResult<Vec<SourceReleaseOutcome>> {
	let client = build_http_client("GitLab")?;
	let token = gitlab_token()?;
	let headers = auth_headers(&token)?;
	let api_base = gitlab_api_base(source)?;
	requests
		.iter()
		.map(|request| publish_release_request(&client, &headers, &api_base, source, request))
		.collect()
}

#[must_use = "the pull request result must be checked"]
pub fn publish_release_pull_request(
	source: &SourceConfiguration,
	root: &Path,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<SourceChangeRequestOutcome> {
	let lookup_source = source.clone();
	let lookup_request = request.clone();
	let existing_merge_request = thread::spawn(move || {
		let client = build_http_client("GitLab")?;
		let token = gitlab_token()?;
		let headers = auth_headers(&token)?;
		let api_base = gitlab_api_base(&lookup_source)?;
		lookup_existing_merge_request(&client, &headers, &api_base, &lookup_request)
	});
	git_checkout_branch(
		root,
		&request.head_branch,
		"prepare release merge request branch",
	)?;
	git_stage_paths(root, tracked_paths, "stage release merge request files")?;
	git_commit_paths(
		root,
		&request.commit_message,
		"commit release merge request changes",
	)?;
	let head_commit = git_head_commit(root)?;
	let existing = join_existing_merge_request_lookup(existing_merge_request)?;
	let head_matches_existing =
		existing.as_ref().and_then(|mr| mr.sha.as_deref()) == Some(head_commit.as_str());
	if !head_matches_existing {
		git_push_branch(
			root,
			&request.head_branch,
			"push release merge request branch",
		)?;
	}

	let client = build_http_client("GitLab")?;
	let token = gitlab_token()?;
	let headers = auth_headers(&token)?;
	let api_base = gitlab_api_base(source)?;
	publish_merge_request_with_existing(
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
	let project_id = encode(&format!("{}/{}", request.owner, request.repo)).into_owned();
	let lookup_url = format!(
		"{api_base}/projects/{project_id}/releases/{}",
		encode(&request.tag_name)
	);
	let create_url = format!("{api_base}/projects/{project_id}/releases");
	let update_url = format!(
		"{api_base}/projects/{project_id}/releases/{}",
		encode(&request.tag_name)
	);
	let existing =
		get_optional_json::<GitLabReleaseResponse>(client, headers, &lookup_url, "GitLab")?;
	let response: GitLabReleaseResponse = if existing.is_some() {
		patch_json(
			client,
			headers,
			&update_url,
			&GitLabReleaseUpdatePayload {
				name: &request.name,
				description: request.body.as_deref(),
			},
			"GitLab",
		)?
	} else {
		post_json(
			client,
			headers,
			&create_url,
			&GitLabReleaseCreatePayload {
				name: &request.name,
				tag_name: &request.tag_name,
				description: request.body.as_deref(),
				ref_: &source.pull_requests.base,
			},
			"GitLab",
		)?
	};
	Ok(SourceReleaseOutcome {
		provider: SourceProvider::GitLab,
		repository: request.repository.clone(),
		tag_name: request.tag_name.clone(),
		operation: if existing.is_some() {
			SourceReleaseOperation::Updated
		} else {
			SourceReleaseOperation::Created
		},
		url: response.web_url,
	})
}

#[cfg_attr(not(test), allow(dead_code))]
fn publish_merge_request(
	client: &Client,
	headers: &HeaderMap,
	api_base: &str,
	request: &SourceChangeRequest,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	let existing = lookup_existing_merge_request(client, headers, api_base, request)?;
	publish_merge_request_with_existing(client, headers, api_base, request, existing.as_ref(), "")
}

fn publish_merge_request_with_existing(
	client: &Client,
	headers: &HeaderMap,
	api_base: &str,
	request: &SourceChangeRequest,
	existing: Option<&GitLabExistingMergeRequest>,
	head_commit: &str,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	let labels = request.labels.join(",");
	let labels_match = existing.is_some_and(|merge_request| {
		let mut existing_labels = merge_request.labels.clone();
		existing_labels.sort();
		existing_labels.dedup();
		let mut requested_labels = request.labels.clone();
		requested_labels.sort();
		requested_labels.dedup();
		existing_labels == requested_labels
	});
	let content_matches = existing.is_some_and(|merge_request| {
		merge_request.title == request.title
			&& merge_request.description.as_deref().unwrap_or_default() == request.body
			&& merge_request.target_branch == request.base_branch
	});
	let head_matches_existing =
		existing.and_then(|merge_request| merge_request.sha.as_deref()) == Some(head_commit);
	let project_id = encode(&format!("{}/{}", request.owner, request.repo)).into_owned();
	let create_url = format!("{api_base}/projects/{project_id}/merge_requests");
	let response: GitLabMergeRequestResponse = if let Some(existing_mr) = &existing {
		if content_matches && labels_match {
			GitLabMergeRequestResponse {
				iid: existing_mr.iid,
				web_url: existing_mr.web_url.clone(),
			}
		} else {
			let update_url = format!(
				"{api_base}/projects/{project_id}/merge_requests/{}",
				existing_mr.iid
			);
			let update_payload = GitLabMergeRequestUpdatePayload {
				title: &request.title,
				target_branch: &request.base_branch,
				description: &request.body,
				labels: &labels,
			};

			put_json(client, headers, &update_url, &update_payload, "GitLab")?
		}
	} else {
		post_json(
			client,
			headers,
			&create_url,
			&GitLabMergeRequestPayload {
				title: &request.title,
				source_branch: &request.head_branch,
				target_branch: &request.base_branch,
				description: &request.body,
				labels: &labels,
			},
			"GitLab",
		)?
	};
	Ok(SourceChangeRequestOutcome {
		provider: SourceProvider::GitLab,
		repository: request.repository.clone(),
		number: response.iid,
		head_branch: request.head_branch.clone(),
		operation: if existing.is_none() {
			SourceChangeRequestOperation::Created
		} else if content_matches && labels_match && head_matches_existing {
			SourceChangeRequestOperation::Skipped
		} else {
			SourceChangeRequestOperation::Updated
		},
		url: response.web_url,
	})
}

fn lookup_existing_merge_request(
	client: &Client,
	headers: &HeaderMap,
	api_base: &str,
	request: &SourceChangeRequest,
) -> MonochangeResult<Option<GitLabExistingMergeRequest>> {
	let project_id = encode(&format!("{}/{}", request.owner, request.repo)).into_owned();
	let list_url = format!(
		"{api_base}/projects/{project_id}/merge_requests?state=opened&source_branch={}&target_branch={}",
		encode(&request.head_branch),
		encode(&request.base_branch),
	);
	Ok(
		get_json::<Vec<GitLabExistingMergeRequest>>(client, headers, &list_url, "GitLab")?
			.into_iter()
			.next(),
	)
}

fn join_existing_merge_request_lookup(
	handle: thread::JoinHandle<MonochangeResult<Option<GitLabExistingMergeRequest>>>,
) -> MonochangeResult<Option<GitLabExistingMergeRequest>> {
	handle.join().map_err(|_| {
		MonochangeError::Config("failed to join GitLab merge request lookup thread".to_string())
	})?
}

#[cfg(test)]
fn gitlab_client() -> MonochangeResult<Client> {
	build_http_client("GitLab")
}

fn gitlab_token() -> MonochangeResult<String> {
	env::var("GITLAB_TOKEN")
		.or_else(|_| env::var("GL_TOKEN"))
		.map_err(|_| {
			MonochangeError::Config(
				"set `GITLAB_TOKEN` (or `GL_TOKEN`) before running GitLab automation".to_string(),
			)
		})
}

#[allow(clippy::unnecessary_wraps)]
fn gitlab_api_base(source: &SourceConfiguration) -> MonochangeResult<String> {
	if let Some(api_url) = &source.api_url {
		return Ok(api_url.trim_end_matches('/').to_string());
	}
	let host = source
		.host
		.as_deref()
		.unwrap_or("https://gitlab.com")
		.trim_end_matches('/');
	Ok(format!("{host}/api/v4"))
}

fn auth_headers(token: &str) -> MonochangeResult<HeaderMap> {
	let mut headers = HeaderMap::new();
	headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
	headers.insert(
		"PRIVATE-TOKEN",
		HeaderValue::from_str(token).map_err(|error| {
			MonochangeError::Config(format!("invalid GitLab token header value: {error}"))
		})?,
	);
	Ok(headers)
}

#[cfg(test)]
mod __tests;
