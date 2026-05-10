use std::path::Path;

use glob::Pattern;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ProviderReleaseSettings;
use monochange_core::SourceConfiguration;
use serde::Serialize;

use crate::git_support;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleaseBranchVerificationReport {
	pub ref_name: String,
	pub commit: String,
	pub allowed_branches: Vec<String>,
	pub matched_branch: String,
}

pub(crate) async fn verify_release_ref_for_tags(
	root: &Path,
	source: Option<&SourceConfiguration>,
	ref_name: &str,
) -> MonochangeResult<Option<ReleaseBranchVerificationReport>> {
	let Some(source) = source else {
		return Ok(None);
	};
	if !source.releases.enforce_for_tags {
		return Ok(None);
	}
	verify_release_ref(root, &source.releases, ref_name)
		.await
		.map(Some)
}

pub(crate) async fn verify_release_ref_for_publish(
	root: &Path,
	source: Option<&SourceConfiguration>,
	ref_name: &str,
) -> MonochangeResult<Option<ReleaseBranchVerificationReport>> {
	let Some(source) = source else {
		return Ok(None);
	};
	if !source.releases.enforce_for_publish {
		return Ok(None);
	}
	verify_release_ref(root, &source.releases, ref_name)
		.await
		.map(Some)
}

pub(crate) async fn verify_release_ref_for_commit(
	root: &Path,
	source: Option<&SourceConfiguration>,
	ref_name: &str,
) -> MonochangeResult<Option<ReleaseBranchVerificationReport>> {
	let Some(source) = source else {
		return Ok(None);
	};
	if !source.releases.enforce_for_commit {
		return Ok(None);
	}
	verify_release_ref(root, &source.releases, ref_name)
		.await
		.map(Some)
}

pub(crate) async fn verify_release_ref(
	root: &Path,
	policy: &ProviderReleaseSettings,
	ref_name: &str,
) -> MonochangeResult<ReleaseBranchVerificationReport> {
	if policy.branches.is_empty() {
		return Err(MonochangeError::Config(
			"[source.releases].branches must contain at least one release branch pattern"
				.to_string(),
		));
	}

	let commit = git_support::resolve_git_commit_ref(root, ref_name).await?;
	let branch_refs = candidate_release_branch_refs(root, &policy.branches).await?;

	for branch_ref in &branch_refs {
		if git_support::git_is_ancestor(root, &commit, &branch_ref.ref_name).await? {
			return Ok(ReleaseBranchVerificationReport {
				ref_name: ref_name.to_string(),
				commit,
				allowed_branches: policy.branches.clone(),
				matched_branch: branch_ref.display_name.clone(),
			});
		}
	}

	let available = branch_refs
		.iter()
		.map(|branch| branch.display_name.as_str())
		.collect::<Vec<_>>()
		.join(", ");
	let available = if available.is_empty() {
		"none found".to_string()
	} else {
		available
	};

	Err(MonochangeError::Config(format!(
		"release ref `{ref_name}` resolves to commit {}, which is not reachable from any configured release branch pattern [{}]; matching branch refs: {available}",
		crate::short_commit_sha(&commit),
		policy.branches.join(", ")
	)))
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct BranchRef {
	ref_name: String,
	display_name: String,
}

async fn candidate_release_branch_refs(
	root: &Path,
	patterns: &[String],
) -> MonochangeResult<Vec<BranchRef>> {
	let compiled = patterns
		.iter()
		.map(|pattern| {
			Pattern::new(pattern).map_err(|error| {
				MonochangeError::Config(format!(
					"invalid [source.releases].branches pattern `{pattern}`: {error}"
				))
			})
		})
		.collect::<MonochangeResult<Vec<_>>>()?;

	let output = git_support::run_git_capture(
		root,
		&[
			"for-each-ref",
			"--format=%(refname)",
			"refs/heads",
			"refs/remotes",
		],
		"failed to list git branches for release branch verification",
	)
	.await?;

	let mut branches = Vec::new();
	for (ref_name, display_name) in output
		.lines()
		.filter(|line| !line.trim().is_empty())
		.filter_map(|ref_name| {
			display_branch_name(ref_name).map(|display_name| (ref_name, display_name))
		}) {
		if branch_matches(&compiled, ref_name, &display_name) {
			branches.push(BranchRef {
				ref_name: ref_name.to_string(),
				display_name,
			});
		}
	}

	Ok(branches)
}

fn display_branch_name(ref_name: &str) -> Option<String> {
	if let Some(local) = ref_name.strip_prefix("refs/heads/") {
		return Some(local.to_string());
	}
	let remote = ref_name.strip_prefix("refs/remotes/")?;
	if remote.ends_with("/HEAD") {
		return None;
	}
	Some(remote.to_string())
}

fn branch_matches(patterns: &[Pattern], ref_name: &str, display_name: &str) -> bool {
	let remote_stripped = display_name.split_once('/').map(|(_, branch)| branch);
	patterns.iter().any(|pattern| {
		pattern.matches(display_name)
			|| remote_stripped.is_some_and(|branch| pattern.matches(branch))
			|| pattern.matches(ref_name)
	})
}

#[cfg(test)]
#[path = "__tests__/release_branch_policy_tests.rs"]
mod tests;
