use std::path::Path;

use monochange_core::CommitMessage;
use monochange_core::MergeReleasePrReport;
use monochange_core::MergeReleasePullRequestOperation;
use monochange_core::MergeReleasePullRequestOutcome;
use monochange_core::MergeReleasePullRequestRequest;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ReleaseRecordDiscovery;
use monochange_core::RetargetOperation;
use monochange_core::RetargetPlan;
use monochange_core::RetargetProviderOperation;
use monochange_core::RetargetProviderResult;
use monochange_core::RetargetResult;
use monochange_core::RetargetTagResult;
use monochange_core::SlashCommandAuthorizationResult;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::parse_release_record_block;
use monochange_core::release_record_release_tag_names;
use monochange_core::release_record_tag_names;
use serde::Serialize;

use crate::OutputFormat;
use crate::git_support::create_git_tag;
use crate::git_support::first_parent_commits;
use crate::git_support::git_is_ancestor;
use crate::git_support::move_git_tag;
use crate::git_support::push_git_tags;
use crate::git_support::push_git_tags_without_force;
use crate::git_support::read_git_commit_message;
use crate::git_support::resolve_git_commit_ref;
use crate::git_support::resolve_git_tag_commit;
use crate::git_support::run_git_fetch_origin;
use crate::hosted_sources;

pub(crate) fn render_release_record_discovery(
	root: &Path,
	from: &str,
	format: OutputFormat,
) -> MonochangeResult<String> {
	let discovery = discover_release_record(root, from)?;
	match format {
		OutputFormat::Json => {
			serde_json::to_string_pretty(&discovery)
				.map_err(|error| MonochangeError::Discovery(error.to_string()))
		}
		OutputFormat::Markdown | OutputFormat::Text => {
			Ok(text_release_record_discovery(&discovery))
		}
	}
}

#[tracing::instrument(skip_all, fields(from))]
/// Discover the durable release record associated with a tag or commit-ish.
///
/// The lookup resolves `from`, walks first-parent ancestry, and returns the
/// first embedded monochange release record it finds together with discovery
/// metadata such as the resolved commit and ancestry distance.
pub fn discover_release_record(
	root: &Path,
	from: &str,
) -> MonochangeResult<ReleaseRecordDiscovery> {
	let resolved_commit = resolve_git_commit_ref(root, from)?;
	for (distance, commit) in first_parent_commits(root, &resolved_commit)?
		.into_iter()
		.enumerate()
	{
		tracing::trace!(
			commit = &commit[..7],
			distance,
			"scanning for release record"
		);
		let message = read_git_commit_message(root, &commit)?;
		match parse_release_record_block(&message) {
			Ok(record) => {
				return Ok(ReleaseRecordDiscovery {
					input_ref: from.to_string(),
					resolved_commit: resolved_commit.clone(),
					record_commit: commit,
					distance,
					record,
				});
			}
			Err(monochange_core::ReleaseRecordError::NotFound) => {}
			Err(monochange_core::ReleaseRecordError::UnsupportedSchemaVersion(version)) => {
				return Err(MonochangeError::Discovery(format!(
					"release record in commit {} uses unsupported schemaVersion {}",
					crate::short_commit_sha(&commit),
					version
				)));
			}
			Err(error) => {
				return Err(MonochangeError::Discovery(format!(
					"found a malformed monochange release record in commit {}: {}",
					crate::short_commit_sha(&commit),
					error
				)));
			}
		}
	}
	Err(MonochangeError::Discovery(format!(
		"no monochange release record found in first-parent ancestry from `{from}`"
	)))
}

/// Build a retarget plan for a previously published release.
pub fn plan_release_retarget(
	root: &Path,
	discovery: &ReleaseRecordDiscovery,
	target: &str,
	force: bool,
	sync_provider: bool,
	dry_run: bool,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<RetargetPlan> {
	let target_commit = resolve_git_commit_ref(root, target)?;
	let is_descendant = git_is_ancestor(root, &discovery.record_commit, &target_commit)?;

	if !is_descendant && !force {
		return Err(MonochangeError::Config(format!(
			"target commit {} is not a descendant of release-record commit {}; rerun with --force to override",
			crate::short_commit_sha(&target_commit),
			crate::short_commit_sha(&discovery.record_commit)
		)));
	}

	validate_retarget_provider(discovery, source)?;

	let git_tag_updates = release_record_tag_names(&discovery.record)
		.into_iter()
		.map(|tag_name| {
			let from_commit = resolve_git_tag_commit(root, &tag_name)?;

			Ok(RetargetTagResult {
				tag_name,
				operation: if from_commit == target_commit {
					RetargetOperation::AlreadyUpToDate
				} else {
					RetargetOperation::Planned
				},
				from_commit,
				to_commit: target_commit.clone(),
				message: None,
			})
		})
		.collect::<MonochangeResult<Vec<_>>>()?;
	let provider = source.map(|configured| configured.provider).or_else(|| {
		discovery
			.record
			.provider
			.as_ref()
			.map(|provider| provider.kind)
	});

	let provider_updates = if sync_provider {
		match provider {
			Some(provider) => {
				let planned_provider_tags = release_record_release_tag_names(&discovery.record)
					.into_iter()
					.map(|tag_name| {
						RetargetTagResult {
							tag_name,
							operation: RetargetOperation::Planned,
							from_commit: discovery.record_commit.clone(),
							to_commit: target_commit.clone(),
							message: None,
						}
					})
					.collect::<Vec<_>>();

				hosted_sources::hosted_source_adapter(provider)
					.plan_retargeted_releases(&planned_provider_tags)
			}
			None => Vec::new(),
		}
	} else {
		Vec::new()
	};

	Ok(RetargetPlan {
		record_commit: discovery.record_commit.clone(),
		target_commit,
		is_descendant,
		force,
		git_tag_updates,
		provider_updates,
		sync_provider,
		dry_run,
	})
}

/// Execute a previously prepared release-retarget plan.
pub fn execute_release_retarget(
	root: &Path,
	source: Option<&SourceConfiguration>,
	plan: &RetargetPlan,
) -> MonochangeResult<RetargetResult> {
	let mut git_tag_results = plan.git_tag_updates.clone();

	if !plan.dry_run {
		for update in &mut git_tag_results {
			if update.from_commit == update.to_commit {
				update.operation = RetargetOperation::AlreadyUpToDate;
				continue;
			}
			move_git_tag(root, &update.tag_name, &update.to_commit)?;
			update.operation = RetargetOperation::Moved;
		}

		let moved_tags = git_tag_results
			.iter()
			.filter(|update| update.operation == RetargetOperation::Moved)
			.map(|update| update.tag_name.as_str())
			.collect::<Vec<_>>();

		if !moved_tags.is_empty() {
			push_git_tags(root, &moved_tags)?;
		}
	}

	let provider_results = if !plan.sync_provider {
		Vec::new()
	} else if plan
		.provider_updates
		.iter()
		.any(|result| result.operation == RetargetProviderOperation::Unsupported)
	{
		if plan.dry_run {
			plan.provider_updates.clone()
		} else {
			let provider = plan
				.provider_updates
				.first()
				.map_or(SourceProvider::GitHub, |result| result.provider);

			return Err(MonochangeError::Config(format!(
				"provider sync is not yet supported for {provider} release retargeting"
			)));
		}
	} else if let Some(source) = source {
		sync_retargeted_provider_releases(source, &git_tag_results, plan.dry_run)?
	} else {
		Vec::new()
	};

	Ok(RetargetResult {
		record_commit: plan.record_commit.clone(),
		target_commit: plan.target_commit.clone(),
		force: plan.force,
		git_tag_results,
		provider_results,
		sync_provider: plan.sync_provider,
		dry_run: plan.dry_run,
	})
}

/// Plan and execute a release retarget operation in one call.
pub fn retarget_release(
	root: &Path,
	discovery: &ReleaseRecordDiscovery,
	target: &str,
	force: bool,
	sync_provider: bool,
	dry_run: bool,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<RetargetResult> {
	let plan = plan_release_retarget(
		root,
		discovery,
		target,
		force,
		sync_provider,
		dry_run,
		source,
	)?;
	execute_release_retarget(root, source, &plan)
}

fn validate_retarget_provider(
	discovery: &ReleaseRecordDiscovery,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<()> {
	let Some(source) = source else {
		return Ok(());
	};
	let Some(provider) = &discovery.record.provider else {
		return Ok(());
	};

	if provider.kind != source.provider {
		return Err(MonochangeError::Config(format!(
			"release record provider `{}` does not match configured source provider `{}`",
			provider.kind, source.provider
		)));
	}

	if provider.owner != source.owner || provider.repo != source.repo {
		return Err(MonochangeError::Config(format!(
			"release record repository `{}/{}` does not match configured source repository `{}/{}`",
			provider.owner, provider.repo, source.owner, source.repo
		)));
	}

	Ok(())
}

pub(crate) fn sync_retargeted_provider_releases(
	source: &SourceConfiguration,
	tag_results: &[RetargetTagResult],
	dry_run: bool,
) -> MonochangeResult<Vec<RetargetProviderResult>> {
	hosted_sources::configured_hosted_source_adapter(source).sync_retargeted_releases(
		source,
		tag_results,
		dry_run,
	)
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReleaseTagOperation {
	Planned,
	Created,
	AlreadyUpToDate,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleaseTagResult {
	pub tag_name: String,
	pub target_commit: String,
	#[serde(default)]
	pub existing_commit: Option<String>,
	pub operation: ReleaseTagOperation,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleaseTagReport {
	pub from: String,
	pub resolved_from_commit: String,
	pub record_commit: String,
	pub distance: usize,
	pub push: bool,
	pub dry_run: bool,
	pub tag_results: Vec<ReleaseTagResult>,
	pub status: String,
}

pub(crate) fn render_release_tag_report(
	root: &Path,
	from: &str,
	format: OutputFormat,
	push: bool,
	dry_run: bool,
) -> MonochangeResult<String> {
	let discovery = discover_release_record(root, from)?;
	let report = create_release_tags(root, &discovery, push, dry_run)?;

	match format {
		OutputFormat::Json => {
			serde_json::to_string_pretty(&report)
				.map_err(|error| MonochangeError::Discovery(error.to_string()))
		}
		OutputFormat::Markdown | OutputFormat::Text => Ok(text_release_tag_report(&report)),
	}
}

pub(crate) fn create_release_tags(
	root: &Path,
	discovery: &ReleaseRecordDiscovery,
	push: bool,
	dry_run: bool,
) -> MonochangeResult<ReleaseTagReport> {
	if discovery.distance != 0 {
		return Err(MonochangeError::Config(format!(
			"resolved ref `{}` points to commit {}, but the nearest monochange release record is in ancestor commit {} at distance {}; `tag-release` only tags a commit that is itself the release commit",
			discovery.input_ref,
			crate::short_commit_sha(&discovery.resolved_commit),
			crate::short_commit_sha(&discovery.record_commit),
			discovery.distance,
		)));
	}

	let tag_results = release_record_tag_names(&discovery.record)
		.into_iter()
		.map(|tag_name| {
			let existing_commit = resolve_git_tag_commit(root, &tag_name).ok();
			let operation = if existing_commit.as_deref() == Some(discovery.record_commit.as_str())
			{
				ReleaseTagOperation::AlreadyUpToDate
			} else {
				ReleaseTagOperation::Planned
			};

			Ok(ReleaseTagResult {
				tag_name,
				target_commit: discovery.record_commit.clone(),
				existing_commit,
				operation,
			})
		})
		.collect::<MonochangeResult<Vec<_>>>()?;

	for tag_result in &tag_results {
		if let Some(existing_commit) = &tag_result.existing_commit
			&& existing_commit != &tag_result.target_commit
		{
			return Err(MonochangeError::Config(format!(
				"tag `{}` already points to commit {}; use `mc repair-release` if you need to move an existing release tag",
				tag_result.tag_name,
				crate::short_commit_sha(existing_commit),
			)));
		}
	}

	let mut tag_results = tag_results;

	if !dry_run {
		for tag_result in &mut tag_results {
			if tag_result.operation == ReleaseTagOperation::AlreadyUpToDate {
				continue;
			}

			create_git_tag(root, &tag_result.tag_name, &tag_result.target_commit)?;
			tag_result.operation = ReleaseTagOperation::Created;
		}

		if push {
			let tags_to_push = tag_results
				.iter()
				.filter(|tag_result| tag_result.operation == ReleaseTagOperation::Created)
				.map(|tag_result| tag_result.tag_name.as_str())
				.collect::<Vec<_>>();

			if !tags_to_push.is_empty() {
				push_git_tags_without_force(root, &tags_to_push)?;
			}
		}
	}

	let status = if dry_run {
		"dry_run"
	} else if tag_results.is_empty() {
		"no_tags_declared"
	} else if tag_results
		.iter()
		.all(|tag_result| tag_result.operation == ReleaseTagOperation::AlreadyUpToDate)
	{
		"already_up_to_date"
	} else {
		"completed"
	};

	Ok(ReleaseTagReport {
		from: discovery.input_ref.clone(),
		resolved_from_commit: discovery.resolved_commit.clone(),
		record_commit: discovery.record_commit.clone(),
		distance: discovery.distance,
		push,
		dry_run,
		tag_results,
		status: status.to_string(),
	})
}

pub(crate) fn text_release_tag_report(report: &ReleaseTagReport) -> String {
	let mut lines = vec!["release tags:".to_string()];
	lines.push(format!("  from: {}", report.from));
	lines.push(format!(
		"  resolved commit: {}",
		crate::short_commit_sha(&report.resolved_from_commit)
	));
	lines.push(format!(
		"  record commit: {}",
		crate::short_commit_sha(&report.record_commit)
	));
	lines.push(format!(
		"  push: {}",
		if report.push { "yes" } else { "no" }
	));

	if report.tag_results.is_empty() {
		lines.push("  tags: none declared by release record".to_string());
	} else {
		lines.push("  tags:".to_string());
		for tag_result in &report.tag_results {
			let existing = tag_result
				.existing_commit
				.as_ref()
				.map_or("missing".to_string(), |commit| {
					crate::short_commit_sha(commit)
				});
			lines.push(format!(
				"    - {} (existing: {}, target: {}) [{}]",
				tag_result.tag_name,
				existing,
				crate::short_commit_sha(&tag_result.target_commit),
				match tag_result.operation {
					ReleaseTagOperation::Planned => "planned",
					ReleaseTagOperation::Created => "created",
					ReleaseTagOperation::AlreadyUpToDate => "already_up_to_date",
				},
			));
		}
	}

	lines.push(format!("  status: {}", report.status.replace('_', "-")));
	lines.join("\n")
}

pub(crate) fn text_release_record_discovery(discovery: &ReleaseRecordDiscovery) -> String {
	let mut lines = vec!["release record:".to_string()];
	lines.push(format!("  input ref: {}", discovery.input_ref));
	lines.push(format!(
		"  resolved commit: {}",
		crate::short_commit_sha(&discovery.resolved_commit)
	));
	lines.push(format!(
		"  record commit: {}",
		crate::short_commit_sha(&discovery.record_commit)
	));
	lines.push(format!("  distance: {}", discovery.distance));
	if let Some(version) = &discovery.record.version {
		lines.push(format!("  version: {version}"));
	}
	if let Some(group_version) = &discovery.record.group_version {
		lines.push(format!("  group version: {group_version}"));
	}
	if !discovery.record.release_targets.is_empty() {
		lines.push("  targets:".to_string());
		for target in &discovery.record.release_targets {
			lines.push(format!(
				"    - {} {} -> {} (tag: {})",
				target.kind, target.id, target.version, target.tag_name
			));
		}
	}
	if !discovery.record.released_packages.is_empty() {
		lines.push("  packages:".to_string());
		for package in &discovery.record.released_packages {
			lines.push(format!("    - {package}"));
		}
	}
	if let Some(provider) = &discovery.record.provider {
		lines.push(format!(
			"  provider: {} {}/{}",
			provider.kind, provider.owner, provider.repo
		));
	}
	lines.join("\n")
}

pub(crate) fn render_merge_release_pr_report(
	root: &Path,
	pr_number: u64,
	format: OutputFormat,
	dry_run: bool,
	author: Option<&str>,
) -> MonochangeResult<String> {
	use monochange_config::load_workspace_configuration;

	let configuration = load_workspace_configuration(root)?;
	let Some(source) = configuration.source.as_ref() else {
		return Err(MonochangeError::Config(
			"merge-release-pr requires a [source] section in monochange.toml".to_string(),
		));
	};

	let adapter = hosted_sources::configured_hosted_source_adapter(source);

	let authorization = author
		.map(|value| adapter.authorize_slash_command_release(source, value))
		.transpose()?;

	let request = MergeReleasePullRequestRequest {
		provider: source.provider,
		repository: format!("{}/{}", source.owner, source.repo),
		owner: source.owner.clone(),
		repo: source.repo.clone(),
		pr_number,
		commit_message: CommitMessage {
			subject: source.pull_requests.title.clone(),
			body: None,
		},
	};

	let merge_outcome = if dry_run {
		Some(MergeReleasePullRequestOutcome {
			provider: source.provider,
			repository: request.repository.clone(),
			pr_number,
			merge_commit_sha: None,
			url: None,
			operation: MergeReleasePullRequestOperation::Skipped,
		})
	} else {
		Some(adapter.merge_release_pull_request(source, &request)?)
	};

	let mut tag_count = None;
	if let Some(ref merge) = merge_outcome {
		if let Some(ref sha) = merge.merge_commit_sha {
			// Fetch the base branch so the merge commit is available locally
			let _ = run_git_fetch_origin(root, &source.pull_requests.base);
			let discovery = discover_release_record(root, sha)?;
			let tag_report = create_release_tags(root, &discovery, true, dry_run)?;
			tag_count = Some(tag_report.tag_results.len());
		}
	}

	let report = MergeReleasePrReport {
		pr_number,
		repository: request.repository,
		dry_run,
		authorization,
		merge_outcome,
		tag_count,
		release_outcomes: None,
		status: if dry_run {
			"dry_run".to_string()
		} else {
			"completed".to_string()
		},
	};

	match format {
		OutputFormat::Json => {
			serde_json::to_string_pretty(&report)
				.map_err(|error| MonochangeError::Discovery(error.to_string()))
		}
		OutputFormat::Markdown | OutputFormat::Text => Ok(text_merge_release_pr_report(&report)),
	}
}

fn text_merge_release_pr_report(report: &MergeReleasePrReport) -> String {
	let mut lines = vec!["merge release pr:".to_string()];
	lines.push(format!("  pr number: {}", report.pr_number));
	lines.push(format!("  repository: {}", report.repository));
	lines.push(format!(
		"  dry-run: {}",
		if report.dry_run { "yes" } else { "no" }
	));
	if let Some(ref authorization) = report.authorization {
		lines.push(format!(
			"  authorization: {}",
			match authorization {
				SlashCommandAuthorizationResult::Authorized => "authorized",
				SlashCommandAuthorizationResult::Denied => "denied",
			}
		));
	}
	if let Some(ref merge) = report.merge_outcome {
		lines.push(format!(
			"  merge: {}{}",
			match merge.operation {
				MergeReleasePullRequestOperation::Merged => "merged",
				MergeReleasePullRequestOperation::Skipped => "skipped",
			},
			merge
				.merge_commit_sha
				.as_deref()
				.map_or_else(String::new, |sha| {
					format!(" ({})", crate::short_commit_sha(sha))
				})
		));
		if let Some(ref url) = merge.url {
			lines.push(format!("  url: {url}"));
		}
	}
	if let Some(count) = report.tag_count {
		lines.push(format!("  tags planned: {count}"));
	}
	lines.push(format!("  status: {}", report.status.replace('_', "-")));
	lines.join("\n")
}
