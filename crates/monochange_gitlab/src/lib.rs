#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use monochange_core::CommitMessage;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseNotesSource;
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
use reqwest::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
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
		|| matches!(source.releases.source, ReleaseNotesSource::GitHubGenerated)
	{
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
		.map(|target| SourceReleaseRequest {
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

	let client = gitlab_client()?;
	let token = gitlab_token()?;
	let api_base = gitlab_api_base(source)?;
	publish_merge_request(&client, &token, &api_base, request)
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

fn publish_merge_request(
	client: &Client,
	token: &str,
	api_base: &str,
	request: &SourceChangeRequest,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	let project_id = encode(&format!("{}/{}", request.owner, request.repo)).into_owned();
	let list_url = format!(
		"{api_base}/projects/{project_id}/merge_requests?state=opened&source_branch={}&target_branch={}",
		encode(&request.head_branch),
		encode(&request.base_branch),
	);
	let create_url = format!("{api_base}/projects/{project_id}/merge_requests");
	let labels = request.labels.join(",");
	let existing = get_json::<Vec<GitLabMergeRequestResponse>>(client, token, &list_url)?
		.into_iter()
		.next();
	let response: GitLabMergeRequestResponse = if let Some(existing_mr) = &existing {
		let update_url = format!(
			"{api_base}/projects/{project_id}/merge_requests/{}",
			existing_mr.iid
		);
		put_json(
			client,
			token,
			&update_url,
			&GitLabMergeRequestUpdatePayload {
				title: &request.title,
				target_branch: &request.base_branch,
				description: &request.body,
				labels: &labels,
			},
		)?
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
		operation: if existing.is_some() {
			SourceChangeRequestOperation::Updated
		} else {
			SourceChangeRequestOperation::Created
		},
		url: response.web_url,
	})
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
		"prepare release merge request branch",
	)
}

fn git_stage_paths(root: &Path, tracked_paths: &[PathBuf]) -> MonochangeResult<()> {
	let mut command = Command::new("git");
	command.current_dir(root).arg("add").arg("-A").arg("--");
	for path in tracked_paths {
		command.arg(path);
	}
	run_command(command, "stage release merge request files")
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
			"failed to commit release merge request changes: {error}"
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
		"failed to commit release merge request changes: {detail}"
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
		"push release merge request branch",
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
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
	target: &ReleaseManifestTarget,
) -> Option<String> {
	match source.releases.source {
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
