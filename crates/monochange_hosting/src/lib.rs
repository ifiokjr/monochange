#![forbid(clippy::indexing_slicing)]

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
use monochange_core::git::git_commit_paths_command;
use monochange_core::git::git_current_branch;
use monochange_core::git::git_push_branch_command;
use monochange_core::git::git_stage_paths_command;
use monochange_core::git::run_command;
use monochange_core::git::run_commit_command_allow_nothing_to_commit;
use reqwest::blocking::Client;
use reqwest::header::HeaderMap;
use serde::Serialize;
use serde::de::DeserializeOwned;

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

pub fn build_http_client(provider: &str) -> MonochangeResult<Client> {
	Client::builder().build().map_err(|error| {
		MonochangeError::Config(format!("failed to build {provider} HTTP client: {error}"))
	})
}

pub fn get_optional_json<T>(
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
	response.json::<T>().map(Some).map_err(|error| {
		MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
	})
}

pub fn get_json<T>(
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
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API GET `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<T>().map_err(|error| {
		MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
	})
}

pub fn post_json<Body, Response>(
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
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API POST `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API POST `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().map_err(|error| {
		MonochangeError::Config(format!("{provider} API POST `{url}` failed: {error}"))
	})
}

pub fn put_json<Body, Response>(
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
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API PUT `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API PUT `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().map_err(|error| {
		MonochangeError::Config(format!("{provider} API PUT `{url}` failed: {error}"))
	})
}

pub fn patch_json<Body, Response>(
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
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API PATCH `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API PATCH `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().map_err(|error| {
		MonochangeError::Config(format!("{provider} API PATCH `{url}` failed: {error}"))
	})
}

pub fn git_checkout_branch(root: &Path, branch: &str, context: &str) -> MonochangeResult<()> {
	if matches!(git_current_branch(root).as_deref(), Ok(current) if current == branch) {
		return Ok(());
	}
	run_command(git_checkout_branch_command(root, branch), context)
}

pub fn git_stage_paths(
	root: &Path,
	tracked_paths: &[PathBuf],
	context: &str,
) -> MonochangeResult<()> {
	run_command(git_stage_paths_command(root, tracked_paths), context)
}

pub fn git_commit_paths(
	root: &Path,
	message: &CommitMessage,
	context: &str,
) -> MonochangeResult<()> {
	run_commit_command_allow_nothing_to_commit(git_commit_paths_command(root, message), context)
}

pub fn git_push_branch(root: &Path, branch: &str, context: &str) -> MonochangeResult<()> {
	run_command(git_push_branch_command(root, branch), context)
}
