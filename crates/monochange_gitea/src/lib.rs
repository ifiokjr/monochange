#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

use std::env;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::git::git_checkout_branch_command;
use monochange_core::git::git_commit_paths_command;
use monochange_core::git::git_push_branch_command;
use monochange_core::git::git_stage_paths_command;
use monochange_core::git::run_command;
use monochange_core::git::run_commit_command_allow_nothing_to_commit;
use monochange_core::CommitMessage;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
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
use reqwest::blocking::Client;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use urlencoding::encode;

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

fn gitea_host(source: &SourceConfiguration) -> &str {
	source
		.host
		.as_deref()
		.unwrap_or("https://gitea.com")
		.trim_end_matches('/')
}

/// URL to a specific tag on the Gitea repository.
#[must_use]
pub fn tag_url(source: &SourceConfiguration, tag_name: &str) -> String {
	let host = gitea_host(source);
	format!(
		"{host}/{}/{}/releases/tag/{tag_name}",
		source.owner, source.repo
	)
}

/// URL comparing two tags on the Gitea repository.
#[must_use]
pub fn compare_url(source: &SourceConfiguration, previous_tag: &str, current_tag: &str) -> String {
	let host = gitea_host(source);
	format!(
		"{host}/{}/{}/compare/{previous_tag}...{current_tag}",
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
		.map(|target| SourceReleaseRequest {
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

pub fn publish_release_requests(
	source: &SourceConfiguration,
	requests: &[SourceReleaseRequest],
) -> MonochangeResult<Vec<SourceReleaseOutcome>> {
	let client = gitea_client()?;
	let token = gitea_token()?;
	let api_base = gitea_api_base(source)?;
	requests
		.iter()
		.map(|request| publish_release_request(&client, &token, &api_base, source, request))
		.collect()
}

pub fn publish_release_pull_request(
	source: &SourceConfiguration,
	root: &Path,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<SourceChangeRequestOutcome> {
	git_checkout_branch(root, &request.head_branch)?;
	git_stage_paths(root, tracked_paths)?;
	git_commit_paths(root, &request.commit_message)?;
	git_push_branch(root, &request.head_branch)?;

	let client = gitea_client()?;
	let token = gitea_token()?;
	let api_base = gitea_api_base(source)?;
	publish_pull_request(&client, &token, &api_base, request)
}

fn publish_release_request(
	client: &Client,
	token: &str,
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
	let existing = get_optional_json::<GiteaReleaseResponse>(client, token, &lookup_url)?;
	let response: GiteaReleaseResponse = if existing.is_some() {
		let update_url = format!(
			"{api_base}/repos/{}/{}/releases/tags/{}",
			request.owner,
			request.repo,
			encode(&request.tag_name)
		);
		patch_json(
			client,
			token,
			&update_url,
			&GiteaReleasePayload {
				tag_name: &request.tag_name,
				name: &request.name,
				body: request.body.as_deref(),
				draft: request.draft,
				prerelease: request.prerelease,
				target_commitish: &source.pull_requests.base,
			},
		)?
	} else {
		let create_url = format!(
			"{api_base}/repos/{}/{}/releases",
			request.owner, request.repo
		);
		post_json(
			client,
			token,
			&create_url,
			&GiteaReleasePayload {
				tag_name: &request.tag_name,
				name: &request.name,
				body: request.body.as_deref(),
				draft: request.draft,
				prerelease: request.prerelease,
				target_commitish: &source.pull_requests.base,
			},
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

fn publish_pull_request(
	client: &Client,
	token: &str,
	api_base: &str,
	request: &SourceChangeRequest,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	let list_url = format!(
		"{api_base}/repos/{}/{}/pulls?state=open&head={}:{}&base={}",
		request.owner,
		request.repo,
		encode(&request.owner),
		encode(&request.head_branch),
		encode(&request.base_branch),
	);
	let existing = get_json::<Vec<GiteaPullRequestResponse>>(client, token, &list_url)?
		.into_iter()
		.next();
	let response: GiteaPullRequestResponse = if let Some(existing_pr) = &existing {
		let update_url = format!(
			"{api_base}/repos/{}/{}/pulls/{}",
			request.owner, request.repo, existing_pr.number
		);
		patch_json(
			client,
			token,
			&update_url,
			&GiteaPullRequestUpdatePayload {
				title: &request.title,
				body: &request.body,
				base: &request.base_branch,
			},
		)?
	} else {
		let create_url = format!("{api_base}/repos/{}/{}/pulls", request.owner, request.repo);
		post_json(
			client,
			token,
			&create_url,
			&GiteaPullRequestPayload {
				title: &request.title,
				head: &request.head_branch,
				base: &request.base_branch,
				body: &request.body,
			},
		)?
	};
	if !request.labels.is_empty() {
		let labels_url = format!(
			"{api_base}/repos/{}/{}/issues/{}/labels",
			request.owner, request.repo, response.number
		);
		let _: serde_json::Value = post_json(
			client,
			token,
			&labels_url,
			&GiteaLabelsPayload {
				labels: &request.labels,
			},
		)?;
	}
	Ok(SourceChangeRequestOutcome {
		provider: SourceProvider::Gitea,
		repository: request.repository.clone(),
		number: response.number,
		head_branch: request.head_branch.clone(),
		operation: if existing.is_some() {
			SourceChangeRequestOperation::Updated
		} else {
			SourceChangeRequestOperation::Created
		},
		url: response.html_url,
	})
}

fn gitea_client() -> MonochangeResult<Client> {
	Client::builder().build().map_err(|error| {
		MonochangeError::Config(format!("failed to build Gitea HTTP client: {error}"))
	})
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

fn get_optional_json<T>(client: &Client, token: &str, url: &str) -> MonochangeResult<Option<T>>
where
	T: DeserializeOwned,
{
	let response = client
		.get(url)
		.headers(auth_headers(token)?)
		.send()
		.map_err(|error| {
			MonochangeError::Config(format!("Gitea API GET `{url}` failed: {error}"))
		})?;
	if response.status().as_u16() == 404 {
		return Ok(None);
	}
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"Gitea API GET `{url}` failed with status {}",
			response.status()
		)));
	}
	response
		.json::<T>()
		.map(Some)
		.map_err(|error| MonochangeError::Config(format!("Gitea API GET `{url}` failed: {error}")))
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
			MonochangeError::Config(format!("Gitea API GET `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"Gitea API GET `{url}` failed with status {}",
			response.status()
		)));
	}
	response
		.json::<T>()
		.map_err(|error| MonochangeError::Config(format!("Gitea API GET `{url}` failed: {error}")))
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
			MonochangeError::Config(format!("Gitea API POST `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"Gitea API POST `{url}` failed with status {}",
			response.status()
		)));
	}
	response
		.json::<Response>()
		.map_err(|error| MonochangeError::Config(format!("Gitea API POST `{url}` failed: {error}")))
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
			MonochangeError::Config(format!("Gitea API PATCH `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"Gitea API PATCH `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().map_err(|error| {
		MonochangeError::Config(format!("Gitea API PATCH `{url}` failed: {error}"))
	})
}

fn git_checkout_branch(root: &Path, branch: &str) -> MonochangeResult<()> {
	run_command(
		git_checkout_branch_command(root, branch),
		"prepare release pull request branch",
	)
}

fn git_stage_paths(root: &Path, tracked_paths: &[PathBuf]) -> MonochangeResult<()> {
	run_command(
		git_stage_paths_command(root, tracked_paths),
		"stage release pull request files",
	)
}

fn git_commit_paths(root: &Path, message: &CommitMessage) -> MonochangeResult<()> {
	run_commit_command_allow_nothing_to_commit(
		git_commit_paths_command(root, message),
		"commit release pull request changes",
	)
}

fn git_push_branch(root: &Path, branch: &str) -> MonochangeResult<()> {
	run_command(
		git_push_branch_command(root, branch),
		"push release pull request branch",
	)
}

fn release_body(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
	target: &ReleaseManifestTarget,
) -> Option<String> {
	match source.releases.source {
		ProviderReleaseNotesSource::GitHubGenerated => None,
		ProviderReleaseNotesSource::Monochange => manifest
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
