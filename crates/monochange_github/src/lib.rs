#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_github`
//!
//! <!-- {=monochangeGithubCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_github` turns `MonoChange` release manifests into GitHub release requests.
//!
//! Reach for this crate when you want to preview or publish GitHub releases using the same structured release data that powers changelog files and release manifests.
//!
//! ## Why use it?
//!
//! - derive GitHub release payloads from `MonoChange`'s structured release manifest
//! - keep GitHub release bodies aligned with changelog rendering and release targets
//! - reuse one publishing path for dry-run previews and real release publication
//!
//! ## Best for
//!
//! - building GitHub release automation on top of `mc release`
//! - previewing would-be GitHub releases in CI before publishing
//! - converting grouped or package release targets into repository release payloads
//!
//! ## Public entry points
//!
//! - `build_release_requests(config, manifest)` converts a release manifest into GitHub release requests
//! - `publish_release_requests(requests)` publishes requests through the `gh` CLI when available
//!
//! ## Example
//!
//! ```rust
//! use monochange_core::GitHubConfiguration;
//! use monochange_core::GitHubReleaseSettings;
//! use monochange_core::ReleaseManifest;
//! use monochange_core::ReleaseManifestPlan;
//! use monochange_core::ReleaseManifestTarget;
//! use monochange_core::ReleaseOwnerKind;
//! use monochange_core::VersionFormat;
//! use monochange_github::build_release_requests;
//!
//! let manifest = ReleaseManifest {
//!     workflow: "release".to_string(),
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
//!     }],
//!     released_packages: vec!["workflow-core".to_string(), "workflow-app".to_string()],
//!     changed_files: Vec::new(),
//!     changelogs: Vec::new(),
//!     deleted_changesets: Vec::new(),
//!     deployments: Vec::new(),
//!     plan: ReleaseManifestPlan {
//!         workspace_root: std::path::PathBuf::from("."),
//!         decisions: Vec::new(),
//!         groups: Vec::new(),
//!         warnings: Vec::new(),
//!         unresolved_items: Vec::new(),
//!         compatibility_evidence: Vec::new(),
//!     },
//! };
//! let github = GitHubConfiguration {
//!     owner: "ifiokjr".to_string(),
//!     repo: "monochange".to_string(),
//!     releases: GitHubReleaseSettings::default(),
//! };
//!
//! let requests = build_release_requests(&github, &manifest);
//!
//! assert_eq!(requests.len(), 1);
//! assert_eq!(requests[0].tag_name, "v1.2.0");
//! assert_eq!(requests[0].repository, "ifiokjr/monochange");
//! ```
//! <!-- {/monochangeGithubCrateDocs} -->

use std::process::Command;

use monochange_core::GitHubConfiguration;
use monochange_core::GitHubReleaseNotesSource;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseOwnerKind;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubReleaseRequest {
	pub repository: String,
	pub owner: String,
	pub repo: String,
	pub target_id: String,
	pub target_kind: ReleaseOwnerKind,
	pub tag_name: String,
	pub name: String,
	pub body: Option<String>,
	pub draft: bool,
	pub prerelease: bool,
	pub generate_release_notes: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitHubReleaseOperation {
	Created,
	Updated,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubReleaseOutcome {
	pub repository: String,
	pub tag_name: String,
	pub operation: GitHubReleaseOperation,
	pub html_url: Option<String>,
}

#[must_use]
pub fn build_release_requests(
	github: &GitHubConfiguration,
	manifest: &ReleaseManifest,
) -> Vec<GitHubReleaseRequest> {
	manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
		.map(|target| GitHubReleaseRequest {
			repository: format!("{}/{}", github.owner, github.repo),
			owner: github.owner.clone(),
			repo: github.repo.clone(),
			target_id: target.id.clone(),
			target_kind: target.kind,
			tag_name: target.tag_name.clone(),
			name: release_name(target),
			body: release_body(github, manifest, target),
			draft: github.releases.draft,
			prerelease: github.releases.prerelease,
			generate_release_notes: github.releases.generate_notes,
		})
		.collect()
}

pub fn publish_release_requests(
	requests: &[GitHubReleaseRequest],
) -> MonochangeResult<Vec<GitHubReleaseOutcome>> {
	requests
		.iter()
		.map(publish_release_request)
		.collect::<Result<Vec<_>, _>>()
}

fn publish_release_request(
	request: &GitHubReleaseRequest,
) -> MonochangeResult<GitHubReleaseOutcome> {
	let existing = lookup_existing_release(request)?;
	let payload = serde_json::to_string(&json!({
		"tag_name": request.tag_name,
		"name": request.name,
		"body": request.body,
		"draft": request.draft,
		"prerelease": request.prerelease,
		"generate_release_notes": request.generate_release_notes,
	}))
	.map_err(|error| MonochangeError::Config(error.to_string()))?;
	let (operation, endpoint) = match existing {
		Some(existing) => (
			GitHubReleaseOperation::Updated,
			format!(
				"repos/{}/{}/releases/{}",
				request.owner, request.repo, existing.id
			),
		),
		None => (
			GitHubReleaseOperation::Created,
			format!("repos/{}/{}/releases", request.owner, request.repo),
		),
	};
	let method = match operation {
		GitHubReleaseOperation::Created => "POST",
		GitHubReleaseOperation::Updated => "PATCH",
	};
	let output = Command::new("gh")
		.arg("api")
		.arg("--method")
		.arg(method)
		.arg(endpoint)
		.arg("--input")
		.arg("-")
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped())
		.spawn()
		.and_then(|mut child| {
			use std::io::Write;
			child
				.stdin
				.as_mut()
				.ok_or_else(|| std::io::Error::other("failed to open gh stdin"))?
				.write_all(payload.as_bytes())?;
			child.wait_with_output()
		})
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to run `gh api` for {}: {error}",
				request.tag_name
			))
		})?;
	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"GitHub release publish failed for `{}`: {}",
			request.tag_name,
			String::from_utf8_lossy(&output.stderr).trim()
		)));
	}
	let response = serde_json::from_slice::<GitHubReleaseResponse>(&output.stdout)
		.map_err(|error| MonochangeError::Config(error.to_string()))?;
	Ok(GitHubReleaseOutcome {
		repository: request.repository.clone(),
		tag_name: request.tag_name.clone(),
		operation,
		html_url: response.html_url,
	})
}

fn lookup_existing_release(
	request: &GitHubReleaseRequest,
) -> MonochangeResult<Option<GitHubExistingRelease>> {
	let output = Command::new("gh")
		.arg("api")
		.arg(format!(
			"repos/{}/{}/releases/tags/{}",
			request.owner, request.repo, request.tag_name
		))
		.output()
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to query existing GitHub release for `{}`: {error}",
				request.tag_name
			))
		})?;
	if output.status.success() {
		let release = serde_json::from_slice::<GitHubExistingRelease>(&output.stdout)
			.map_err(|error| MonochangeError::Config(error.to_string()))?;
		return Ok(Some(release));
	}
	let stderr = String::from_utf8_lossy(&output.stderr);
	if stderr.contains("HTTP 404") || stderr.contains("Not Found") {
		return Ok(None);
	}
	Err(MonochangeError::Config(format!(
		"failed to query GitHub release `{}`: {}",
		request.tag_name,
		stderr.trim()
	)))
}

fn release_name(target: &ReleaseManifestTarget) -> String {
	format!("{} {}", target.id, target.version)
}

fn release_body(
	github: &GitHubConfiguration,
	manifest: &ReleaseManifest,
	target: &ReleaseManifestTarget,
) -> Option<String> {
	match github.releases.source {
		GitHubReleaseNotesSource::GitHubGenerated => None,
		GitHubReleaseNotesSource::Monochange => manifest
			.changelogs
			.iter()
			.find(|changelog| {
				changelog.owner_id == target.id && changelog.owner_kind == target.kind
			})
			.map(|changelog| changelog.rendered.clone())
			.or_else(|| Some(minimal_release_body(manifest, target))),
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

#[derive(Debug, Deserialize)]
struct GitHubExistingRelease {
	id: u64,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseResponse {
	html_url: Option<String>,
}

#[cfg(test)]
mod __tests;
