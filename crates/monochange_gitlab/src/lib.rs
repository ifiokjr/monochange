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
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseOwnerKind;
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
use monochange_core::git::git_commit_paths_command;
use monochange_core::git::git_current_branch;
use monochange_core::git::git_head_commit;
use monochange_core::git::git_push_branch_command;
use monochange_core::git::git_stage_paths_command;
use monochange_core::git::run_command;
use monochange_core::git::run_commit_command_allow_nothing_to_commit;
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
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

/// URL to a specific tag on the GitLab repository.
#[must_use]
pub fn tag_url(source: &SourceConfiguration, tag_name: &str) -> String {
	let host = gitlab_host(source);
	format!(
		"{host}/{}/{}/-/releases/{tag_name}",
		source.owner, source.repo
	)
}

/// URL comparing two tags on the GitLab repository.
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
	let client = gitlab_client()?;
	let token = gitlab_token()?;
	let api_base = gitlab_api_base(source)?;
	requests
		.iter()
		.map(|request| publish_release_request(&client, &token, &api_base, source, request))
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
		let client = gitlab_client()?;
		let token = gitlab_token()?;
		let api_base = gitlab_api_base(&lookup_source)?;
		lookup_existing_merge_request(&client, &token, &api_base, &lookup_request)
	});
	git_checkout_branch(root, &request.head_branch)?;
	git_stage_paths(root, tracked_paths)?;
	git_commit_paths(root, &request.commit_message)?;
	let head_commit = git_head_commit(root)?;
	let existing = join_existing_merge_request_lookup(existing_merge_request)?;
	let head_matches_existing =
		existing.as_ref().and_then(|mr| mr.sha.as_deref()) == Some(head_commit.as_str());
	if !head_matches_existing {
		git_push_branch(root, &request.head_branch)?;
	}

	let client = gitlab_client()?;
	let token = gitlab_token()?;
	let api_base = gitlab_api_base(source)?;
	publish_merge_request_with_existing(
		&client,
		&token,
		&api_base,
		request,
		existing.as_ref(),
		&head_commit,
	)
}

fn publish_release_request(
	client: &Client,
	token: &str,
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
	let existing = get_optional_json::<GitLabReleaseResponse>(client, token, &lookup_url)?;
	let response: GitLabReleaseResponse = if existing.is_some() {
		patch_json(
			client,
			token,
			&update_url,
			&GitLabReleaseUpdatePayload {
				name: &request.name,
				description: request.body.as_deref(),
			},
		)?
	} else {
		post_json(
			client,
			token,
			&create_url,
			&GitLabReleaseCreatePayload {
				name: &request.name,
				tag_name: &request.tag_name,
				description: request.body.as_deref(),
				ref_: &source.pull_requests.base,
			},
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
	token: &str,
	api_base: &str,
	request: &SourceChangeRequest,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	let existing = lookup_existing_merge_request(client, token, api_base, request)?;
	publish_merge_request_with_existing(client, token, api_base, request, existing.as_ref(), "")
}

fn publish_merge_request_with_existing(
	client: &Client,
	token: &str,
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

			put_json(client, token, &update_url, &update_payload)?
		}
	} else {
		post_json(
			client,
			token,
			&create_url,
			&GitLabMergeRequestPayload {
				title: &request.title,
				source_branch: &request.head_branch,
				target_branch: &request.base_branch,
				description: &request.body,
				labels: &labels,
			},
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
	token: &str,
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
		get_json::<Vec<GitLabExistingMergeRequest>>(client, token, &list_url)?
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

fn gitlab_client() -> MonochangeResult<Client> {
	Client::builder().build().map_err(|error| {
		MonochangeError::Config(format!("failed to build GitLab HTTP client: {error}"))
	})
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

fn get_optional_json<T>(client: &Client, token: &str, url: &str) -> MonochangeResult<Option<T>>
where
	T: DeserializeOwned,
{
	let response = client
		.get(url)
		.headers(auth_headers(token)?)
		.send()
		.map_err(|error| {
			MonochangeError::Config(format!("GitLab API GET `{url}` failed: {error}"))
		})?;
	if response.status().as_u16() == 404 {
		return Ok(None);
	}
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"GitLab API GET `{url}` failed with status {}",
			response.status()
		)));
	}
	response
		.json::<T>()
		.map(Some)
		.map_err(|error| MonochangeError::Config(format!("GitLab API GET `{url}` failed: {error}")))
}

fn get_json<T>(client: &Client, token: &str, url: &str) -> MonochangeResult<T>
where
	T: DeserializeOwned,
{
	let response = client
		.get(url)
		.headers(auth_headers(token)?)
		.send()
		.map_err(|error| {
			MonochangeError::Config(format!("GitLab API GET `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"GitLab API GET `{url}` failed with status {}",
			response.status()
		)));
	}
	response
		.json::<T>()
		.map_err(|error| MonochangeError::Config(format!("GitLab API GET `{url}` failed: {error}")))
}

fn post_json<Body, Response>(
	client: &Client,
	token: &str,
	url: &str,
	body: &Body,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
{
	let response = client
		.post(url)
		.headers(auth_headers(token)?)
		.json(body)
		.send()
		.map_err(|error| {
			MonochangeError::Config(format!("GitLab API POST `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"GitLab API POST `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().map_err(|error| {
		MonochangeError::Config(format!("GitLab API POST `{url}` failed: {error}"))
	})
}

fn put_json<Body, Response>(
	client: &Client,
	token: &str,
	url: &str,
	body: &Body,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
{
	let response = client
		.put(url)
		.headers(auth_headers(token)?)
		.json(body)
		.send()
		.map_err(|error| {
			MonochangeError::Config(format!("GitLab API PUT `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"GitLab API PUT `{url}` failed with status {}",
			response.status()
		)));
	}
	response
		.json::<Response>()
		.map_err(|error| MonochangeError::Config(format!("GitLab API PUT `{url}` failed: {error}")))
}

fn patch_json<Body, Response>(
	client: &Client,
	token: &str,
	url: &str,
	body: &Body,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
{
	let response = client
		.patch(url)
		.headers(auth_headers(token)?)
		.json(body)
		.send()
		.map_err(|error| {
			MonochangeError::Config(format!("GitLab API PATCH `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"GitLab API PATCH `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().map_err(|error| {
		MonochangeError::Config(format!("GitLab API PATCH `{url}` failed: {error}"))
	})
}

fn git_checkout_branch(root: &Path, branch: &str) -> MonochangeResult<()> {
	if matches!(git_current_branch(root).as_deref(), Ok(current) if current == branch) {
		return Ok(());
	}
	run_command(
		git_checkout_branch_command(root, branch),
		"prepare release merge request branch",
	)
}

fn git_stage_paths(root: &Path, tracked_paths: &[PathBuf]) -> MonochangeResult<()> {
	run_command(
		git_stage_paths_command(root, tracked_paths),
		"stage release merge request files",
	)
}

fn git_commit_paths(root: &Path, message: &CommitMessage) -> MonochangeResult<()> {
	run_commit_command_allow_nothing_to_commit(
		git_commit_paths_command(root, message),
		"commit release merge request changes",
	)
}

fn git_push_branch(root: &Path, branch: &str) -> MonochangeResult<()> {
	run_command(
		git_push_branch_command(root, branch),
		"push release merge request branch",
	)
}

fn release_body(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
	target: &ReleaseManifestTarget,
) -> Option<String> {
	match source.releases.source {
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
