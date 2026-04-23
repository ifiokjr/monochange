#![forbid(clippy::indexing_slicing)]

//! # `monochange_hosting`
//!
//! <!-- {=monochangeHostingCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_hosting` packages the shared git and HTTP plumbing used by hosted source providers.
//!
//! Reach for this crate when you are implementing GitHub, Gitea, or GitLab release adapters and want one place for release-body rendering, change-request branch naming, JSON requests, and git branch orchestration.
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

/// Perform a GET request and deserialize a successful JSON response.
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

/// Perform a POST request and deserialize a successful JSON response.
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

/// Perform a PUT request and deserialize a successful JSON response.
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

/// Perform a PATCH request and deserialize a successful JSON response.
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

/// Check out or reset the local release branch used for provider requests.
pub fn git_checkout_branch(root: &Path, branch: &str, context: &str) -> MonochangeResult<()> {
	if matches!(git_current_branch(root).as_deref(), Ok(current) if current == branch) {
		return Ok(());
	}
	run_command(git_checkout_branch_command(root, branch), context)
}

/// Stage the provided paths before creating a release commit.
pub fn git_stage_paths(
	root: &Path,
	tracked_paths: &[PathBuf],
	context: &str,
) -> MonochangeResult<()> {
	run_command(git_stage_paths_command(root, tracked_paths), context)
}

/// Commit the prepared release changes, tolerating a no-op commit.
pub fn git_commit_paths(
	root: &Path,
	message: &CommitMessage,
	context: &str,
	no_verify: bool,
) -> MonochangeResult<()> {
	run_commit_command_allow_nothing_to_commit(
		git_commit_paths_command(root, message, no_verify),
		context,
	)
}

/// Push the release branch to `origin` with `--force-with-lease`.
pub fn git_push_branch(
	root: &Path,
	branch: &str,
	context: &str,
	no_verify: bool,
) -> MonochangeResult<()> {
	run_command(git_push_branch_command(root, branch, no_verify), context)
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use monochange_core::ReleaseManifest;
	use monochange_core::ReleaseManifestPlan;
	use monochange_core::ReleaseManifestTarget;
	use monochange_core::ReleaseOwnerKind;
	use monochange_core::VersionFormat;

	use super::*;

	fn sample_manifest() -> ReleaseManifest {
		ReleaseManifest {
			command: "release".to_string(),
			dry_run: false,
			version: None,
			group_version: None,
			release_targets: vec![],
			package_publications: vec![],
			released_packages: vec![],
			changed_files: vec![],
			changelogs: vec![],
			changesets: vec![],
			deleted_changesets: vec![],
			plan: ReleaseManifestPlan {
				workspace_root: PathBuf::from("."),
				decisions: vec![],
				groups: vec![],
				warnings: vec![],
				unresolved_items: vec![],
				compatibility_evidence: vec![],
			},
		}
	}

	fn minimal_target(id: &str) -> ReleaseManifestTarget {
		ReleaseManifestTarget {
			id: id.to_string(),
			kind: ReleaseOwnerKind::Package,
			version: "0.1.0".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Namespaced,
			tag_name: "v0.1.0".to_string(),
			members: vec![],
			rendered_title: String::new(),
			rendered_changelog_title: String::new(),
		}
	}

	#[test]
	fn push_body_entries_adds_dash_prefix_to_plain_entries() {
		let mut lines = Vec::new();
		push_body_entries(
			&mut lines,
			&["fix bug".to_string(), "add feature".to_string()],
		);
		assert_eq!(lines, vec!["- fix bug", "- add feature"]);
	}

	#[test]
	fn push_body_entries_preserves_list_markers() {
		let mut lines = Vec::new();
		push_body_entries(&mut lines, &["- already a list item".to_string()]);
		assert_eq!(lines, vec!["- already a list item"]);
	}

	#[test]
	fn push_body_entries_preserves_star_markers() {
		let mut lines = Vec::new();
		push_body_entries(&mut lines, &["* starred item".to_string()]);
		assert_eq!(lines, vec!["* starred item"]);
	}

	#[test]
	fn push_body_entries_preserves_headings() {
		let mut lines = Vec::new();
		push_body_entries(&mut lines, &["### Bug Fixes".to_string()]);
		assert_eq!(lines, vec!["### Bug Fixes"]);
	}

	#[test]
	fn push_body_entries_splits_multiline_entries() {
		let mut lines = Vec::new();
		push_body_entries(
			&mut lines,
			&["line one\nline two".to_string(), "second entry".to_string()],
		);
		assert_eq!(lines, vec!["line one", "line two", "", "- second entry"]);
	}

	#[test]
	fn push_body_entries_multiline_last_entry_has_no_trailing_blank() {
		let mut lines = Vec::new();
		push_body_entries(&mut lines, &["multi\nline".to_string()]);
		assert_eq!(lines, vec!["multi", "line"]);
	}

	#[test]
	fn minimal_release_body_includes_target_id_and_members() {
		let manifest = sample_manifest();
		let target = ReleaseManifestTarget {
			id: "my-pkg".to_string(),
			kind: ReleaseOwnerKind::Package,
			version: "1.0.0".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Namespaced,
			tag_name: "v1.0.0".to_string(),
			members: vec!["dep-a".to_string(), "dep-b".to_string()],
			rendered_title: String::new(),
			rendered_changelog_title: String::new(),
		};
		let body = minimal_release_body(&manifest, &target);
		assert!(body.contains("my-pkg"));
		assert!(body.contains("dep-a, dep-b"));
	}

	#[test]
	fn minimal_release_body_without_members_shows_prepare_release() {
		let manifest = sample_manifest();
		let target = minimal_target("core");
		let body = minimal_release_body(&manifest, &target);
		assert!(body.contains("prepare release"));
	}

	#[test]
	fn release_pull_request_branch_sanitizes_special_characters() {
		assert_eq!(
			release_pull_request_branch("release/", "My Cool PR!"),
			"release/my-cool-pr"
		);
	}

	#[test]
	fn release_pull_request_branch_falls_back_for_empty_command() {
		assert_eq!(
			release_pull_request_branch("release/", "!!!"),
			"release/release"
		);
	}

	#[test]
	fn release_pull_request_branch_preserves_alphanumeric() {
		assert_eq!(
			release_pull_request_branch("release/", "v2-Feature"),
			"release/v2-feature"
		);
	}

	#[test]
	fn release_pull_request_branch_strips_trailing_slash_from_prefix() {
		assert_eq!(
			release_pull_request_branch("monochange/release/", "Add Feature"),
			"monochange/release/add-feature"
		);
	}

	#[test]
	fn build_http_client_succeeds() {
		assert!(build_http_client("test").is_ok());
	}

	#[test]
	fn release_pull_request_body_includes_command_and_targets() {
		let manifest = ReleaseManifest {
			command: "release".to_string(),
			dry_run: false,
			version: None,
			group_version: None,
			release_targets: vec![minimal_target("core")],
			package_publications: vec![],
			released_packages: vec![],
			changed_files: vec![PathBuf::from("Cargo.toml")],
			changelogs: vec![],
			changesets: vec![],
			deleted_changesets: vec![],
			plan: ReleaseManifestPlan {
				workspace_root: PathBuf::from("."),
				decisions: vec![],
				groups: vec![],
				warnings: vec![],
				unresolved_items: vec![],
				compatibility_evidence: vec![],
			},
		};
		let body = release_pull_request_body(&manifest);
		assert!(body.contains("## Prepared release"));
		assert!(body.contains("`release`"));
		assert!(body.contains("core"));
	}

	#[test]
	fn release_pull_request_body_shows_no_outward_targets_when_none_release() {
		let mut manifest = sample_manifest();
		manifest.release_targets = vec![ReleaseManifestTarget {
			id: "internal".to_string(),
			kind: ReleaseOwnerKind::Package,
			version: "1.0.0".to_string(),
			tag: true,
			release: false,
			version_format: VersionFormat::Namespaced,
			tag_name: "v1.0.0".to_string(),
			members: vec![],
			rendered_title: String::new(),
			rendered_changelog_title: String::new(),
		}];
		let body = release_pull_request_body(&manifest);
		assert!(body.contains("no outward release targets"));
	}

	#[test]
	fn release_pull_request_body_lists_changed_files() {
		let manifest = ReleaseManifest {
			command: "release".to_string(),
			dry_run: false,
			version: None,
			group_version: None,
			release_targets: vec![],
			package_publications: vec![],
			released_packages: vec![],
			changed_files: vec![PathBuf::from("src/main.rs")],
			changelogs: vec![],
			changesets: vec![],
			deleted_changesets: vec![],
			plan: ReleaseManifestPlan {
				workspace_root: PathBuf::from("."),
				decisions: vec![],
				groups: vec![],
				warnings: vec![],
				unresolved_items: vec![],
				compatibility_evidence: vec![],
			},
		};
		let body = release_pull_request_body(&manifest);
		assert!(body.contains("## Changed files"));
		assert!(body.contains("src/main.rs"));
	}

	#[test]
	fn minimal_release_body_with_decision_reasons() {
		let manifest = ReleaseManifest {
			command: "release".to_string(),
			dry_run: false,
			version: None,
			group_version: None,
			release_targets: vec![],
			package_publications: vec![],
			released_packages: vec![],
			changed_files: vec![],
			changelogs: vec![],
			changesets: vec![],
			deleted_changesets: vec![],
			plan: ReleaseManifestPlan {
				workspace_root: PathBuf::from("."),
				decisions: vec![monochange_core::ReleaseManifestPlanDecision {
					package: "my-pkg".to_string(),
					bump: monochange_core::BumpSeverity::Patch,
					trigger: "direct-change".to_string(),
					planned_version: Some("1.0.1".to_string()),
					reasons: vec!["fix race condition".to_string()],
					upstream_sources: vec![],
				}],
				groups: vec![],
				warnings: vec![],
				unresolved_items: vec![],
				compatibility_evidence: vec![],
			},
		};
		let target = minimal_target("my-pkg");
		let body = minimal_release_body(&manifest, &target);
		assert!(body.contains("fix race condition"));
		assert!(!body.contains("prepare release"));
	}

	#[test]
	fn release_body_returns_none_for_github_generated() {
		use monochange_core::ProviderBotSettings;
		use monochange_core::ProviderMergeRequestSettings;
		use monochange_core::ProviderReleaseNotesSource;
		use monochange_core::ProviderReleaseSettings;

		let source = SourceConfiguration {
			provider: monochange_core::SourceProvider::GitHub,
			owner: "org".to_string(),
			repo: "repo".to_string(),
			host: None,
			api_url: None,
			releases: ProviderReleaseSettings {
				enabled: true,
				draft: false,
				prerelease: false,
				generate_notes: false,
				source: ProviderReleaseNotesSource::GitHubGenerated,
			},
			pull_requests: ProviderMergeRequestSettings::default(),
			bot: ProviderBotSettings::default(),
		};
		let manifest = sample_manifest();
		let target = minimal_target("core");
		assert_eq!(release_body(&source, &manifest, &target), None);
	}

	#[test]
	fn release_body_returns_rendered_changelog_for_monochange_source() {
		use monochange_core::ChangelogFormat;
		use monochange_core::ProviderBotSettings;
		use monochange_core::ProviderMergeRequestSettings;
		use monochange_core::ProviderReleaseNotesSource;
		use monochange_core::ProviderReleaseSettings;
		use monochange_core::ReleaseManifestChangelog;
		use monochange_core::ReleaseNotesDocument;
		use monochange_core::ReleaseNotesSection;

		let source = SourceConfiguration {
			provider: monochange_core::SourceProvider::GitLab,
			owner: "org".to_string(),
			repo: "repo".to_string(),
			host: None,
			api_url: None,
			releases: ProviderReleaseSettings {
				enabled: true,
				draft: false,
				prerelease: false,
				generate_notes: false,
				source: ProviderReleaseNotesSource::Monochange,
			},
			pull_requests: ProviderMergeRequestSettings::default(),
			bot: ProviderBotSettings::default(),
		};
		let mut manifest = sample_manifest();
		let target = minimal_target("core");
		manifest.changelogs = vec![ReleaseManifestChangelog {
			owner_id: "core".to_string(),
			owner_kind: ReleaseOwnerKind::Package,
			path: PathBuf::from("changelog.md"),
			format: ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "1.0.0".to_string(),
				summary: vec![],
				sections: vec![ReleaseNotesSection {
					title: "Bug Fixes".to_string(),
					entries: vec!["fix crash".to_string()],
				}],
			},
			rendered: "## 1.0.0\n\n### Bug Fixes\n\n- fix crash".to_string(),
		}];
		let body = release_body(&source, &manifest, &target);
		assert_eq!(
			body,
			Some("## 1.0.0\n\n### Bug Fixes\n\n- fix crash".to_string())
		);
	}

	#[test]
	fn release_body_falls_back_to_minimal_when_no_changelog() {
		use monochange_core::ProviderBotSettings;
		use monochange_core::ProviderMergeRequestSettings;
		use monochange_core::ProviderReleaseNotesSource;
		use monochange_core::ProviderReleaseSettings;

		let source = SourceConfiguration {
			provider: monochange_core::SourceProvider::GitLab,
			owner: "org".to_string(),
			repo: "repo".to_string(),
			host: None,
			api_url: None,
			releases: ProviderReleaseSettings {
				enabled: true,
				draft: false,
				prerelease: false,
				generate_notes: false,
				source: ProviderReleaseNotesSource::Monochange,
			},
			pull_requests: ProviderMergeRequestSettings::default(),
			bot: ProviderBotSettings::default(),
		};
		let manifest = sample_manifest();
		let target = minimal_target("core");
		let body = release_body(&source, &manifest, &target);
		assert!(body.is_some());
		assert!(body.unwrap().contains("core"));
	}
}
