#![forbid(clippy::indexing_slicing)]

//! # `monochange_hosting`
//!
//! <!-- {=monochangeHostingCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_hosting` packages the shared git and HTTP plumbing used by hosted source providers.
//!
//! Reach for this crate when you are implementing GitHub, Gitea, Forgejo, or GitLab release adapters and want one place for release-body rendering, change-request branch naming, JSON requests, and git branch orchestration.
//!
//! ## Why use it?
//!
//! - keep provider adapters focused on provider-specific payloads instead of repeated plumbing
//! - share one markdown rendering path for release bodies and release pull requests
//! - reuse one set of blocking HTTP helpers with consistent error messages
//!
//! ## Best for
//!
//! - implementing or testing hosted source adapters
//! - generating release pull request bodies from prepared manifests
//! - staging, committing, and pushing release branches through shared wrappers
//!
//! ## Public entry points
//!
//! - `release_body(source, manifest, target)` resolves the outward release body for a target
//! - `release_pull_request_body(manifest)` renders the provider change-request body
//! - `release_pull_request_branch(prefix, command)` normalizes the change-request branch name
//! - `get_json`, `post_json`, `patch_json`, and `put_json` wrap provider API requests
//! - `git_checkout_branch`, `git_stage_paths`, `git_commit_paths`, and `git_push_branch` wrap shared git operations
//! <!-- {/monochangeHostingCrateDocs} -->

use std::path::Path;
use std::path::PathBuf;

use monochange_core::CommitMessage;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ProviderReleaseNotesSource;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseOwnerKind;
use monochange_core::SourceConfiguration;
use monochange_core::git::git_checkout_branch_command;
use monochange_core::git::git_current_branch;
use monochange_core::git::git_push_branch_command;
use monochange_core::git::git_stage_paths_command;
use monochange_core::git::run_command;
use monochange_core::git::run_git_commit_message;
use reqwest::Client;
use reqwest::header::HeaderMap;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Append release-note entries to a markdown body, normalizing bullet formatting.
pub fn push_body_entries(lines: &mut Vec<String>, entries: &[String]) {
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

/// Render a fallback release body when no changelog body is available.
pub fn minimal_release_body(manifest: &ReleaseManifest, target: &ReleaseManifestTarget) -> String {
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

/// Build the provider change-request branch for a release command.
pub fn release_pull_request_branch(branch_prefix: &str, command: &str) -> String {
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

/// Render the markdown body used for provider release requests.
pub fn release_pull_request_body(manifest: &ReleaseManifest) -> String {
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

/// Resolve the provider release body for one outward release target.
pub fn release_body(
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

/// Build a blocking HTTP client for provider API calls.
pub fn build_http_client(provider: &str) -> MonochangeResult<Client> {
	Client::builder().build().map_err(|error| {
		MonochangeError::Config(format!("failed to build {provider} HTTP client: {error}"))
	})
}

/// Perform a GET request that treats `404` as `Ok(None)`.
pub async fn get_optional_json<T>(
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
	if response.status().as_u16() == 404 {
		return Ok(None);
	}
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API GET `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<T>().await.map(Some).map_err(|error| {
		MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
	})
}

/// Perform a GET request and deserialize a successful JSON response.
pub async fn get_json<T>(
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
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API GET `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<T>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
	})
}

/// Perform a POST request and deserialize a successful JSON response.
pub async fn post_json<Body, Response>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	body: &Body,
	provider: &str,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
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
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API POST `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API POST `{url}` failed: {error}"))
	})
}

/// Perform a PUT request and deserialize a successful JSON response.
pub async fn put_json<Body, Response>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	body: &Body,
	provider: &str,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
{
	let response = client
		.put(url)
		.headers(headers.clone())
		.json(body)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API PUT `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API PUT `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API PUT `{url}` failed: {error}"))
	})
}

/// Perform a PATCH request and deserialize a successful JSON response.
pub async fn patch_json<Body, Response>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	body: &Body,
	provider: &str,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
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
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API PATCH `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API PATCH `{url}` failed: {error}"))
	})
}

/// Check out or reset the local release branch used for provider requests.
pub async fn git_checkout_branch(root: &Path, branch: &str, context: &str) -> MonochangeResult<()> {
	if matches!(git_current_branch(root).await.as_deref(), Ok(current) if current == branch) {
		return Ok(());
	}
	run_command(git_checkout_branch_command(root, branch), context).await
}

/// Stage the provided paths before creating a release commit.
pub async fn git_stage_paths(
	root: &Path,
	tracked_paths: &[PathBuf],
	context: &str,
) -> MonochangeResult<()> {
	run_command(git_stage_paths_command(root, tracked_paths), context).await
}

/// Commit the prepared release changes, tolerating a no-op commit.
pub async fn git_commit_paths(
	root: &Path,
	message: &CommitMessage,
	context: &str,
	no_verify: bool,
) -> MonochangeResult<()> {
	run_git_commit_message(root, message, context, no_verify).await
}

/// Push the release branch to `origin` with `--force-with-lease`.
pub async fn git_push_branch(
	root: &Path,
	branch: &str,
	context: &str,
	no_verify: bool,
) -> MonochangeResult<()> {
	run_command(git_push_branch_command(root, branch, no_verify), context).await
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
