use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use minijinja::Environment;
use minijinja::UndefinedBehavior;
use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogSettings;
use monochange_core::ChangelogTarget;
use monochange_core::ChangesetTargetKind;
use monochange_core::GroupChangelogInclude;
use monochange_core::HostedActorRef;
use monochange_core::HostedIssueRef;
use monochange_core::HostedIssueRelationshipKind;
use monochange_core::HostedReviewRequestRef;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::PreparedChangeset;
use monochange_core::PreparedChangesetTarget;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseNotesSection;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleasePlan;
use monochange_core::VersionFormat;
use monochange_core::relative_to_root;
use monochange_core::render_release_notes;
use typed_builder::TypedBuilder;

pub type PackageChangelogTargets = BTreeMap<String, ChangelogTarget>;
pub type GroupChangelogTargets = BTreeMap<String, ChangelogTarget>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReleaseTarget {
	pub id: String,
	pub kind: ReleaseOwnerKind,
	pub version: String,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
	pub tag_name: String,
	pub members: Vec<String>,
	pub rendered_title: String,
	pub rendered_changelog_title: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FileUpdate {
	pub path: PathBuf,
	pub content: Vec<u8>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChangelogUpdate {
	pub file: FileUpdate,
	pub owner_id: String,
	pub owner_kind: ReleaseOwnerKind,
	pub format: ChangelogFormat,
	pub notes: ReleaseNotesDocument,
	pub rendered: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReleaseNoteChange {
	pub package_id: String,
	pub package_name: String,
	pub package_labels: Vec<String>,
	pub source_path: Option<String>,
	pub summary: String,
	pub details: Option<String>,
	pub bump: BumpSeverity,
	pub change_type: Option<String>,
	pub context: Option<String>,
	pub changeset_path: Option<String>,
	pub change_owner: Option<String>,
	pub change_owner_link: Option<String>,
	pub review_request: Option<String>,
	pub review_request_link: Option<String>,
	pub introduced_commit: Option<String>,
	pub introduced_commit_link: Option<String>,
	pub last_updated_commit: Option<String>,
	pub last_updated_commit_link: Option<String>,
	pub related_issues: Option<String>,
	pub related_issue_links: Option<String>,
	pub closed_issues: Option<String>,
	pub closed_issue_links: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
struct RenderedChangesetContext {
	context: String,
	changeset_path: String,
	change_owner: Option<String>,
	change_owner_link: Option<String>,
	review_request: Option<String>,
	review_request_link: Option<String>,
	introduced_commit: Option<String>,
	introduced_commit_link: Option<String>,
	last_updated_commit: Option<String>,
	last_updated_commit_link: Option<String>,
	related_issues: Option<String>,
	related_issue_links: Option<String>,
	closed_issues: Option<String>,
	closed_issue_links: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct GroupReleaseNoteKey {
	source_path: Option<String>,
	summary: String,
	details: Option<String>,
	bump: BumpSeverity,
	change_type: Option<String>,
	context: Option<String>,
}

fn root_relative(root: &Path, path: &Path) -> PathBuf {
	let relative = relative_to_root(root, path).unwrap_or_else(|| path.to_path_buf());
	if relative.as_os_str().is_empty() {
		PathBuf::from(".")
	} else {
		relative
	}
}

#[derive(Clone, Copy, Debug, TypedBuilder)]
pub struct ChangelogBuildContext<'a> {
	pub root: &'a Path,
	pub configuration: &'a monochange_core::WorkspaceConfiguration,
	pub packages: &'a [PackageRecord],
	pub plan: &'a ReleasePlan,
	pub change_signals: &'a [ChangeSignal],
	pub changesets: &'a [PreparedChangeset],
	pub changelog_targets: &'a (PackageChangelogTargets, GroupChangelogTargets),
	pub release_targets: &'a [ReleaseTarget],
}

#[tracing::instrument(skip_all)]
pub fn build_changelog_updates(
	context: ChangelogBuildContext<'_>,
) -> MonochangeResult<Vec<ChangelogUpdate>> {
	let changeset_context_by_path = context
		.changesets
		.iter()
		.map(|changeset| {
			(
				changeset.path.clone(),
				build_rendered_changeset_context(context.root, changeset),
			)
		})
		.collect::<BTreeMap<_, _>>();
	let changeset_targets_by_path = context
		.changesets
		.iter()
		.map(|changeset| (changeset.path.clone(), changeset.targets.clone()))
		.collect::<BTreeMap<_, _>>();
	let release_note_changes = context
		.change_signals
		.iter()
		.filter_map(|signal| {
			build_release_note_change(
				signal,
				context.packages,
				context.root,
				&changeset_context_by_path,
			)
		})
		.fold(
			BTreeMap::<String, Vec<ReleaseNoteChange>>::new(),
			|mut acc, change| {
				acc.entry(change.package_id.clone())
					.or_default()
					.push(change);
				acc
			},
		);

	let group_definitions_by_id = context
		.configuration
		.groups
		.iter()
		.map(|group| (group.id.as_str(), group))
		.collect::<BTreeMap<_, _>>();
	let package_definitions_by_record_id = context
		.packages
		.iter()
		.filter_map(|package| {
			package.metadata.get("config_id").and_then(|config_id| {
				context
					.configuration
					.package_by_id(config_id)
					.map(|definition| (package.id.as_str(), definition))
			})
		})
		.collect::<BTreeMap<_, _>>();

	let mut updates = Vec::new();
	let package_changelog_targets = &context.changelog_targets.0;
	let group_changelog_targets = &context.changelog_targets.1;
	for decision in context
		.plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
	{
		let Some(changelog_target) = package_changelog_targets.get(&decision.package_id) else {
			continue;
		};
		let Some(package) = context
			.packages
			.iter()
			.find(|package| package.id == decision.package_id)
		else {
			continue;
		};
		let Some(planned_version) = decision.planned_version.as_ref() else {
			continue;
		};
		let package_id = config_package_id(package);
		let package_definition = package_definitions_by_record_id
			.get(decision.package_id.as_str())
			.copied();
		let group_definition = decision
			.group_id
			.as_deref()
			.and_then(|group_id| group_definitions_by_id.get(group_id).copied());
		let changes = package_release_note_changes(
			context.configuration,
			package_definition,
			group_definition,
			decision,
			package,
			release_note_changes.get(&decision.package_id),
			&planned_version.to_string(),
		);
		let changelog_title = context
			.release_targets
			.iter()
			.find(|rt| {
				(rt.kind == ReleaseOwnerKind::Package && rt.id == package_id)
					|| (rt.kind == ReleaseOwnerKind::Group && rt.members.contains(&package_id))
			})
			.map_or_else(
				|| planned_version.to_string(),
				|rt| rt.rendered_changelog_title.clone(),
			);
		let document = build_release_notes_document(
			&package_id,
			&changelog_title,
			Vec::new(),
			&context.configuration.changelog,
			&changes,
		);
		let rendered = render_release_notes(changelog_target.format, &document);
		let initial_header = render_package_initial_changelog_header(
			context,
			changelog_target,
			&package_id,
			package,
			group_definition,
			planned_version,
		);
		let next_changelog = append_changelog_section(
			&changelog_target.path,
			&rendered,
			Some(initial_header.as_str()),
		)?;
		updates.push(ChangelogUpdate {
			file: FileUpdate {
				path: changelog_target.path.clone(),
				content: next_changelog.into_bytes(),
			},
			owner_id: package_id,
			owner_kind: ReleaseOwnerKind::Package,
			format: changelog_target.format,
			notes: document,
			rendered,
		});
	}

	for planned_group in context
		.plan
		.groups
		.iter()
		.filter(|group| group.recommended_bump.is_release())
	{
		let Some(changelog_target) = group_changelog_targets.get(&planned_group.group_id) else {
			continue;
		};
		let Some(planned_version) = planned_group.planned_version.as_ref() else {
			continue;
		};
		let member_ids = context
			.configuration
			.groups
			.iter()
			.find(|group| group.id == planned_group.group_id)
			.map(|group| group.packages.clone())
			.unwrap_or_default();
		let group_definition = group_definitions_by_id
			.get(planned_group.group_id.as_str())
			.copied();
		let changes = group_release_note_changes(
			context.configuration,
			group_definition,
			planned_group,
			&release_note_changes,
			&changeset_targets_by_path,
			context.packages,
			&planned_version.to_string(),
		);
		let changelog_title = context
			.release_targets
			.iter()
			.find(|rt| rt.kind == ReleaseOwnerKind::Group && rt.id == planned_group.group_id)
			.map_or_else(
				|| planned_version.to_string(),
				|rt| rt.rendered_changelog_title.clone(),
			);
		let document = build_release_notes_document(
			&planned_group.group_id,
			&changelog_title,
			group_release_summary(&planned_group.group_id),
			&context.configuration.changelog,
			&changes,
		);
		let rendered = render_release_notes(changelog_target.format, &document);
		let initial_header = render_group_initial_changelog_header(
			context,
			changelog_target,
			planned_group,
			group_definition,
			planned_version,
			&member_ids,
		);
		let next_changelog = append_changelog_section(
			&changelog_target.path,
			&rendered,
			Some(initial_header.as_str()),
		)?;
		updates.push(ChangelogUpdate {
			file: FileUpdate {
				path: changelog_target.path.clone(),
				content: next_changelog.into_bytes(),
			},
			owner_id: planned_group.group_id.clone(),
			owner_kind: ReleaseOwnerKind::Group,
			format: changelog_target.format,
			notes: document,
			rendered,
		});
	}

	Ok(dedup_changelog_updates(updates))
}

fn default_initial_changelog_header(format: ChangelogFormat) -> &'static str {
	if format == ChangelogFormat::KeepAChangelog {
		return monochange_core::DEFAULT_INITIAL_CHANGELOG_HEADER_KEEP_A_CHANGELOG;
	}

	monochange_core::DEFAULT_INITIAL_CHANGELOG_HEADER_MONOCHANGE
}

fn render_package_initial_changelog_header(
	context: ChangelogBuildContext<'_>,
	changelog_target: &ChangelogTarget,
	owner_id: &str,
	package: &PackageRecord,
	group_definition: Option<&monochange_core::GroupDefinition>,
	planned_version: &semver::Version,
) -> String {
	let mut metadata = initial_changelog_header_metadata(context, changelog_target);
	let package_dir = package
		.manifest_path
		.parent()
		.unwrap_or(package.manifest_path.as_path());
	let package_path = monochange_core::normalize_path(package_dir)
		.strip_prefix(monochange_core::normalize_path(context.root))
		.map_or_else(|_| package_dir.to_path_buf(), Path::to_path_buf);
	metadata.insert("package", package.name.clone());
	metadata.insert("package_id", owner_id.to_string());
	metadata.insert("package_name", package.name.clone());
	metadata.insert("package_path", package_path.to_string_lossy().to_string());
	metadata.insert("release_owner", owner_id.to_string());
	metadata.insert("release_owner_kind", "package".to_string());
	metadata.insert("version", planned_version.to_string());
	metadata.insert("new_version", planned_version.to_string());
	metadata.insert(
		"current_version",
		package
			.current_version
			.as_ref()
			.map(ToString::to_string)
			.unwrap_or_default(),
	);
	if let Some(group) = group_definition {
		metadata.insert("group", group.id.clone());
		metadata.insert("group_id", group.id.clone());
		metadata.insert("group_name", group.id.clone());
	}
	render_initial_changelog_header(changelog_target, &metadata)
}

fn render_group_initial_changelog_header(
	context: ChangelogBuildContext<'_>,
	changelog_target: &ChangelogTarget,
	planned_group: &monochange_core::PlannedVersionGroup,
	group_definition: Option<&monochange_core::GroupDefinition>,
	planned_version: &semver::Version,
	member_ids: &[String],
) -> String {
	let mut metadata = initial_changelog_header_metadata(context, changelog_target);
	let group_id =
		group_definition.map_or_else(|| planned_group.group_id.clone(), |group| group.id.clone());
	metadata.insert("group", group_id.clone());
	metadata.insert("group_id", group_id.clone());
	metadata.insert("group_name", group_id.clone());
	metadata.insert("member_count", member_ids.len().to_string());
	metadata.insert("members", member_ids.join(", "));
	metadata.insert("release_owner", group_id);
	metadata.insert("release_owner_kind", "group".to_string());
	metadata.insert("version", planned_version.to_string());
	metadata.insert("new_version", planned_version.to_string());
	render_initial_changelog_header(changelog_target, &metadata)
}

fn initial_changelog_header_metadata(
	context: ChangelogBuildContext<'_>,
	changelog_target: &ChangelogTarget,
) -> BTreeMap<&'static str, String> {
	let config_path = context.configuration.root_path.join("monochange.toml");
	let changelog_path = root_relative(context.root, &changelog_target.path);
	let workspace_root = monochange_core::normalize_path(context.root);
	let mut metadata = BTreeMap::new();
	metadata.insert("monochange_version", env!("CARGO_PKG_VERSION").to_string());
	metadata.insert("config_path", config_path.to_string_lossy().to_string());
	metadata.insert(
		"monochange_config_path",
		config_path.to_string_lossy().to_string(),
	);
	metadata.insert(
		"workspace_root",
		workspace_root.to_string_lossy().to_string(),
	);
	metadata.insert(
		"changelog_path",
		changelog_path.to_string_lossy().to_string(),
	);
	metadata.insert("changelog_format", format!("{:?}", changelog_target.format));
	metadata
}

fn render_initial_changelog_header(
	changelog_target: &ChangelogTarget,
	metadata: &BTreeMap<&'static str, String>,
) -> String {
	let template = changelog_target
		.initial_header
		.as_deref()
		.filter(|header| !header.trim().is_empty())
		.unwrap_or_else(|| default_initial_changelog_header(changelog_target.format));
	render_message_template(template, metadata)
}

fn append_changelog_section(
	path: &Path,
	section: &str,
	initial_header: Option<&str>,
) -> MonochangeResult<String> {
	let current = if path.exists() {
		fs::read_to_string(path).map_err(|error| {
			MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
		})?
	} else {
		String::new()
	};

	let current = current.trim_end();
	if current.is_empty() {
		let Some(initial_header) = initial_header
			.map(str::trim)
			.filter(|header| !header.is_empty())
		else {
			return Ok(format!("{section}\n"));
		};
		return Ok(format!("{initial_header}\n\n{section}\n"));
	}

	let Some(offset) = current
		.lines()
		.scan(0usize, |start, line| {
			let offset = *start;
			*start += line.len() + 1;
			Some((offset, line))
		})
		.find_map(|(offset, line)| is_release_heading(line).then_some(offset))
	else {
		return Ok(format!("{current}\n\n{section}\n"));
	};

	let prefix = current[..offset].trim_end();
	let suffix = current[offset..].trim_start();
	let mut content = String::new();

	if !prefix.is_empty() {
		content.push_str(prefix);
		content.push_str("\n\n");
	}

	content.push_str(section);
	content.push('\n');

	if !suffix.is_empty() {
		content.push('\n');
		content.push_str(suffix);
		content.push('\n');
	}

	Ok(content)
}

fn is_release_heading(line: &str) -> bool {
	let Some(heading) = line.strip_prefix("## ") else {
		return false;
	};

	let heading = heading.trim_start();
	heading.starts_with('[')
		|| heading
			.chars()
			.next()
			.is_some_and(|character| character.is_ascii_digit())
}

fn dedup_changelog_updates(updates: Vec<ChangelogUpdate>) -> Vec<ChangelogUpdate> {
	updates
		.into_iter()
		.fold(
			BTreeMap::<PathBuf, ChangelogUpdate>::new(),
			|mut acc, update| {
				acc.insert(update.file.path.clone(), update);
				acc
			},
		)
		.into_values()
		.collect()
}

fn build_release_note_change(
	signal: &ChangeSignal,
	packages: &[PackageRecord],
	root: &Path,
	changeset_context_by_path: &BTreeMap<PathBuf, RenderedChangesetContext>,
) -> Option<ReleaseNoteChange> {
	let summary = signal.notes.clone()?;
	let package = packages
		.iter()
		.find(|package| package.id == signal.package_id)?;
	let package_id = config_package_id(package);
	let source_path = root_relative(root, &signal.source_path);
	let rendered_context = changeset_context_by_path.get(&source_path);
	Some(ReleaseNoteChange {
		package_id: signal.package_id.clone(),
		package_name: package_id.clone(),
		package_labels: Vec::new(),
		source_path: Some(source_path.display().to_string()),
		summary,
		details: signal.details.clone(),
		bump: signal.requested_bump.unwrap_or(BumpSeverity::Patch),
		change_type: signal.change_type.clone(),
		context: rendered_context.map(|context| context.context.clone()),
		changeset_path: rendered_context.map(|context| context.changeset_path.clone()),
		change_owner: rendered_context.and_then(|context| context.change_owner.clone()),
		change_owner_link: rendered_context.and_then(|context| context.change_owner_link.clone()),
		review_request: rendered_context.and_then(|context| context.review_request.clone()),
		review_request_link: rendered_context
			.and_then(|context| context.review_request_link.clone()),
		introduced_commit: rendered_context.and_then(|context| context.introduced_commit.clone()),
		introduced_commit_link: rendered_context
			.and_then(|context| context.introduced_commit_link.clone()),
		last_updated_commit: rendered_context
			.and_then(|context| context.last_updated_commit.clone()),
		last_updated_commit_link: rendered_context
			.and_then(|context| context.last_updated_commit_link.clone()),
		related_issues: rendered_context.and_then(|context| context.related_issues.clone()),
		related_issue_links: rendered_context
			.and_then(|context| context.related_issue_links.clone()),
		closed_issues: rendered_context.and_then(|context| context.closed_issues.clone()),
		closed_issue_links: rendered_context.and_then(|context| context.closed_issue_links.clone()),
	})
}

fn build_rendered_changeset_context(
	root: &Path,
	changeset: &PreparedChangeset,
) -> RenderedChangesetContext {
	let changeset_path = root_relative(root, &changeset.path).display().to_string();
	let mut rendered = RenderedChangesetContext {
		changeset_path: changeset_path.clone(),
		..RenderedChangesetContext::default()
	};
	let mut lines = Vec::new();
	let Some(context) = changeset.context.as_ref() else {
		rendered.context = lines.join("\n");
		return rendered;
	};

	let primary_revision = context
		.introduced
		.as_ref()
		.or(context.last_updated.as_ref());
	if let Some(actor) = primary_revision.and_then(|revision| revision.actor.as_ref()) {
		let label = render_actor_label(actor);
		let link = render_markdown_link(&label, actor.url.as_deref());
		rendered.change_owner = Some(label.clone());
		rendered.change_owner_link = Some(link.clone());
		lines.push(format!("> _Owner:_ {link}"));
	}

	let review_request = context
		.introduced
		.as_ref()
		.and_then(|revision| revision.review_request.as_ref())
		.or_else(|| {
			context
				.last_updated
				.as_ref()
				.and_then(|revision| revision.review_request.as_ref())
		});
	if let Some(review_request) = review_request {
		let label = render_review_request_label(review_request);
		let link = render_markdown_link(&label, review_request.url.as_deref());
		rendered.review_request = Some(label.clone());
		rendered.review_request_link = Some(link.clone());
		lines.push(format!("> _Review:_ {link}"));
	}

	if let Some(commit) = context
		.introduced
		.as_ref()
		.and_then(|revision| revision.commit.as_ref())
	{
		let label = commit.short_sha.clone();
		let link = render_markdown_link(&format!("`{label}`"), commit.url.as_deref());
		rendered.introduced_commit = Some(label);
		rendered.introduced_commit_link = Some(link.clone());
		lines.push(format!("> _Introduced in:_ {link}"));
	}

	let introduced_sha = context
		.introduced
		.as_ref()
		.and_then(|revision| revision.commit.as_ref())
		.map(|commit| commit.sha.as_str());
	if let Some(commit) = context
		.last_updated
		.as_ref()
		.and_then(|revision| revision.commit.as_ref())
		.filter(|commit| Some(commit.sha.as_str()) != introduced_sha)
	{
		let label = commit.short_sha.clone();
		let link = render_markdown_link(&format!("`{label}`"), commit.url.as_deref());
		rendered.last_updated_commit = Some(label);
		rendered.last_updated_commit_link = Some(link.clone());
		lines.push(format!("> _Last updated in:_ {link}"));
	}

	let closed_issues = context
		.related_issues
		.iter()
		.filter(|issue| issue.relationship == HostedIssueRelationshipKind::ClosedByReviewRequest)
		.collect::<Vec<_>>();
	if !closed_issues.is_empty() {
		let labels = render_issue_labels(&closed_issues);
		let issue_links = render_issue_links(&closed_issues);
		rendered.closed_issues = Some(labels.clone());
		rendered.closed_issue_links = Some(issue_links.clone());
		lines.push(format!("> _Closed issues:_ {issue_links}"));
	}

	let related_issues = context
		.related_issues
		.iter()
		.filter(|issue| issue.relationship != HostedIssueRelationshipKind::ClosedByReviewRequest)
		.collect::<Vec<_>>();
	if !related_issues.is_empty() {
		let labels = render_issue_labels(&related_issues);
		let issue_links = render_issue_links(&related_issues);
		rendered.related_issues = Some(labels.clone());
		rendered.related_issue_links = Some(issue_links.clone());
		lines.push(format!("> _Related issues:_ {issue_links}"));
	}

	rendered.context = lines.join("\n");
	rendered
}

fn render_actor_label(actor: &HostedActorRef) -> String {
	if let Some(login) = actor.login.as_deref() {
		format!("@{login}")
	} else if let Some(display_name) = actor.display_name.as_deref() {
		display_name.to_string()
	} else {
		"unknown".to_string()
	}
}

fn render_review_request_label(review_request: &HostedReviewRequestRef) -> String {
	match review_request.kind {
		monochange_core::HostedReviewRequestKind::PullRequest => {
			format!("PR {}", review_request.id)
		}
		monochange_core::HostedReviewRequestKind::MergeRequest => {
			format!("MR {}", review_request.id)
		}
	}
}

fn render_markdown_link(label: &str, url: Option<&str>) -> String {
	url.map_or_else(|| label.to_string(), |url| format!("[{label}]({url})"))
}

fn render_issue_labels(issues: &[&HostedIssueRef]) -> String {
	issues
		.iter()
		.map(|issue| issue.id.clone())
		.collect::<Vec<_>>()
		.join(", ")
}

fn render_issue_links(issues: &[&HostedIssueRef]) -> String {
	issues
		.iter()
		.map(|issue| render_markdown_link(&issue.id, issue.url.as_deref()))
		.collect::<Vec<_>>()
		.join(", ")
}

fn render_package_empty_update_message(
	configuration: &monochange_core::WorkspaceConfiguration,
	package_definition: Option<&monochange_core::PackageDefinition>,
	group_definition: Option<&monochange_core::GroupDefinition>,
	package: &PackageRecord,
	decision: &monochange_core::ReleaseDecision,
	planned_version: &str,
) -> String {
	let template = select_empty_update_message(
		package_definition.and_then(|definition| definition.empty_update_message.as_deref()),
		group_definition.and_then(|definition| definition.empty_update_message.as_deref()),
		configuration.defaults.empty_update_message.as_deref(),
		if group_definition.is_some() {
			"No package-specific changes were recorded; `{{ package }}` was updated to {{ version }} as part of group `{{ group }}`."
		} else {
			"No package-specific changes were recorded; `{{ package }}` was updated to {{ version }}."
		},
	);
	let mut metadata = BTreeMap::new();
	metadata.insert("package", package.name.clone());
	metadata.insert("package_name", package.name.clone());
	metadata.insert("package_id", decision.package_id.clone());
	metadata.insert("group", decision.group_id.clone().unwrap_or_default());
	metadata.insert("group_name", decision.group_id.clone().unwrap_or_default());
	metadata.insert("group_id", decision.group_id.clone().unwrap_or_default());
	metadata.insert("version", planned_version.to_string());
	metadata.insert("new_version", planned_version.to_string());
	metadata.insert(
		"previous_version",
		package
			.current_version
			.as_ref()
			.map_or_else(String::new, ToString::to_string),
	);
	metadata.insert(
		"current_version",
		package
			.current_version
			.as_ref()
			.map_or_else(String::new, ToString::to_string),
	);
	metadata.insert("bump", decision.recommended_bump.to_string());
	metadata.insert("trigger", decision.trigger_type.clone());
	metadata.insert("ecosystem", package.ecosystem.to_string());
	metadata.insert(
		"release_owner",
		decision
			.group_id
			.clone()
			.unwrap_or_else(|| decision.package_id.clone()),
	);
	metadata.insert(
		"release_owner_kind",
		if decision.group_id.is_some() {
			"group".to_string()
		} else {
			"package".to_string()
		},
	);
	metadata.insert("reasons", decision.reasons.join("; "));
	render_message_template(template, &metadata)
}

fn render_group_empty_update_message(
	configuration: &monochange_core::WorkspaceConfiguration,
	group_definition: Option<&monochange_core::GroupDefinition>,
	planned_group: &monochange_core::PlannedVersionGroup,
	planned_version: &str,
	packages: &[PackageRecord],
) -> String {
	let template = select_empty_update_message(
		group_definition.and_then(|definition| definition.empty_update_message.as_deref()),
		None,
		configuration.defaults.empty_update_message.as_deref(),
		"No package-specific changes were recorded; group `{{ group }}` was updated to {{ version }}.",
	);
	let previous_version = planned_group.members.iter().find_map(|member_id| {
		packages
			.iter()
			.find(|package| package.id == *member_id)
			.and_then(|package| package.current_version.as_ref())
			.map(ToString::to_string)
	});
	let mut metadata = BTreeMap::new();
	metadata.insert("group", planned_group.group_id.clone());
	metadata.insert("group_name", planned_group.group_id.clone());
	metadata.insert("group_id", planned_group.group_id.clone());
	metadata.insert("version", planned_version.to_string());
	metadata.insert("new_version", planned_version.to_string());
	metadata.insert(
		"previous_version",
		previous_version.clone().unwrap_or_default(),
	);
	metadata.insert("current_version", previous_version.unwrap_or_default());
	metadata.insert("bump", planned_group.recommended_bump.to_string());
	metadata.insert("members", planned_group.members.join(", "));
	metadata.insert("member_count", planned_group.members.len().to_string());
	metadata.insert("release_owner", planned_group.group_id.clone());
	metadata.insert("release_owner_kind", "group".to_string());
	render_message_template(template, &metadata)
}

fn select_empty_update_message<'value>(
	primary: Option<&'value str>,
	secondary: Option<&'value str>,
	default_value: Option<&'value str>,
	built_in_default: &'value str,
) -> &'value str {
	primary
		.filter(|message| !message.trim().is_empty())
		.or_else(|| secondary.filter(|message| !message.trim().is_empty()))
		.or_else(|| default_value.filter(|message| !message.trim().is_empty()))
		.unwrap_or(built_in_default)
}

pub fn render_jinja_template(
	template: &str,
	context: &minijinja::Value,
) -> MonochangeResult<String> {
	render_jinja_template_with_behavior(template, context, UndefinedBehavior::Lenient)
}

fn render_jinja_template_strict(
	template: &str,
	context: &minijinja::Value,
) -> MonochangeResult<String> {
	render_jinja_template_with_behavior(template, context, UndefinedBehavior::Strict)
}

fn render_jinja_template_with_behavior(
	template_source: &str,
	context: &minijinja::Value,
	undefined_behavior: UndefinedBehavior,
) -> MonochangeResult<String> {
	use std::collections::HashMap;

	thread_local! {
		static LENIENT_CACHE: std::cell::RefCell<HashMap<String, Environment<'static>>> =
			std::cell::RefCell::new(HashMap::new());
		static STRICT_CACHE: std::cell::RefCell<HashMap<String, Environment<'static>>> =
			std::cell::RefCell::new(HashMap::new());
	}

	let render_with_cache =
		|cache: &std::cell::RefCell<HashMap<String, Environment<'static>>>| -> MonochangeResult<String> {
			cache.borrow_mut().entry(template_source.to_owned()).or_insert_with(|| {
				let mut env = Environment::new();
				env.set_undefined_behavior(undefined_behavior);
				let _ = env.add_template_owned("t", template_source.to_owned());
				env
			});
			let cache_ref = cache.borrow();
			let env = cache_ref.get(template_source).unwrap_or_else(|| unreachable!("just inserted"));
			match env.get_template("t") {
				Ok(tmpl) => tmpl
					.render(context)
					.map_err(|error| MonochangeError::Config(format!("template rendering failed: {error}"))),
				Err(_) => {
					// Template failed to compile on insert; fall back to render_str.
					env.render_str(template_source, context)
						.map_err(|error| MonochangeError::Config(format!("template rendering failed: {error}")))
				}
			}
		};

	match undefined_behavior {
		UndefinedBehavior::Lenient => LENIENT_CACHE.with(render_with_cache),
		UndefinedBehavior::Strict => STRICT_CACHE.with(render_with_cache),
		_ => {
			let mut env = Environment::new();
			env.set_undefined_behavior(undefined_behavior);
			env.render_str(template_source, context).map_err(|error| {
				MonochangeError::Config(format!("template rendering failed: {error}"))
			})
		}
	}
}

pub fn render_message_template(template: &str, metadata: &BTreeMap<&str, String>) -> String {
	let context = minijinja::Value::from_serialize(metadata);
	render_jinja_template(template, &context).unwrap_or_else(|_| template.to_string())
}

fn package_release_note_changes(
	configuration: &monochange_core::WorkspaceConfiguration,
	package_definition: Option<&monochange_core::PackageDefinition>,
	group_definition: Option<&monochange_core::GroupDefinition>,
	decision: &monochange_core::ReleaseDecision,
	package: &PackageRecord,
	direct_changes: Option<&Vec<ReleaseNoteChange>>,
	planned_version: &str,
) -> Vec<ReleaseNoteChange> {
	let mut changes = direct_changes.cloned().unwrap_or_default();
	if changes.is_empty() {
		changes.push(ReleaseNoteChange {
			package_id: decision.package_id.clone(),
			package_name: config_package_id(package),
			package_labels: Vec::new(),
			source_path: None,
			summary: render_package_empty_update_message(
				configuration,
				package_definition,
				group_definition,
				package,
				decision,
				planned_version,
			),
			details: None,
			bump: decision.recommended_bump,
			change_type: None,
			context: None,
			changeset_path: None,
			change_owner: None,
			change_owner_link: None,
			review_request: None,
			review_request_link: None,
			introduced_commit: None,
			introduced_commit_link: None,
			last_updated_commit: None,
			last_updated_commit_link: None,
			related_issues: None,
			related_issue_links: None,
			closed_issues: None,
			closed_issue_links: None,
		});
	}
	changes
}

fn group_release_note_changes(
	configuration: &monochange_core::WorkspaceConfiguration,
	group_definition: Option<&monochange_core::GroupDefinition>,
	planned_group: &monochange_core::PlannedVersionGroup,
	release_note_changes: &BTreeMap<String, Vec<ReleaseNoteChange>>,
	changeset_targets_by_path: &BTreeMap<PathBuf, Vec<PreparedChangesetTarget>>,
	packages: &[PackageRecord],
	planned_version: &str,
) -> Vec<ReleaseNoteChange> {
	let unfiltered_changes = planned_group
		.members
		.iter()
		.flat_map(|member_id| {
			release_note_changes
				.get(member_id)
				.into_iter()
				.flatten()
				.cloned()
		})
		.collect::<Vec<_>>();
	let mut changes = unfiltered_changes
		.iter()
		.filter_map(|change| {
			filter_group_release_note_change(
				change,
				group_definition,
				planned_group,
				changeset_targets_by_path,
			)
		})
		.collect::<Vec<_>>();
	if changes.is_empty() {
		let summary = if unfiltered_changes.is_empty() {
			render_group_empty_update_message(
				configuration,
				group_definition,
				planned_group,
				planned_version,
				packages,
			)
		} else {
			render_group_filtered_update_message(&planned_group.group_id)
		};
		changes.push(ReleaseNoteChange {
			package_id: planned_group.group_id.clone(),
			package_name: planned_group.group_id.clone(),
			package_labels: Vec::new(),
			source_path: None,
			summary,
			details: None,
			bump: planned_group.recommended_bump,
			change_type: None,
			context: None,
			changeset_path: None,
			change_owner: None,
			change_owner_link: None,
			review_request: None,
			review_request_link: None,
			introduced_commit: None,
			introduced_commit_link: None,
			last_updated_commit: None,
			last_updated_commit_link: None,
			related_issues: None,
			related_issue_links: None,
			closed_issues: None,
			closed_issue_links: None,
		});
	} else {
		changes = aggregate_group_release_note_changes(changes);
	}
	changes
}

pub fn filter_group_release_note_change(
	change: &ReleaseNoteChange,
	group_definition: Option<&monochange_core::GroupDefinition>,
	planned_group: &monochange_core::PlannedVersionGroup,
	changeset_targets_by_path: &BTreeMap<PathBuf, Vec<PreparedChangesetTarget>>,
) -> Option<ReleaseNoteChange> {
	let source_path = change.source_path.as_ref().map(PathBuf::from)?;
	let targets = changeset_targets_by_path.get(&source_path)?;
	if targets.iter().any(|target| {
		target.kind == ChangesetTargetKind::Group && target.id == planned_group.group_id
	}) {
		let mut change = change.clone();
		change.package_name.clone_from(&planned_group.group_id);
		return Some(change);
	}
	let in_group_targets = targets
		.iter()
		.filter(|target| {
			target.kind == ChangesetTargetKind::Package
				&& group_definition
					.is_some_and(|group| group.packages.iter().any(|member| member == &target.id))
		})
		.map(|target| target.id.clone())
		.collect::<BTreeSet<_>>();
	if in_group_targets.is_empty() {
		return None;
	}
	let default_include = GroupChangelogInclude::All;
	let include = group_definition.map_or(&default_include, |group| &group.changelog_include);
	if group_changelog_include_allows(include, &in_group_targets) {
		Some(change.clone())
	} else {
		None
	}
}

pub fn group_changelog_include_allows(
	include: &GroupChangelogInclude,
	in_group_targets: &BTreeSet<String>,
) -> bool {
	match include {
		GroupChangelogInclude::All => true,
		GroupChangelogInclude::GroupOnly => false,
		GroupChangelogInclude::Selected(selected) => {
			in_group_targets
				.iter()
				.all(|package_id| selected.contains(package_id))
		}
	}
}

pub fn render_group_filtered_update_message(group_id: &str) -> String {
	format!(
		"No group-facing notes were recorded for this release. Member packages were updated as part of the synchronized group `{group_id}` version, but their changes are not configured for inclusion in this changelog."
	)
}

fn aggregate_group_release_note_changes(changes: Vec<ReleaseNoteChange>) -> Vec<ReleaseNoteChange> {
	let mut aggregated = Vec::<ReleaseNoteChange>::new();
	let mut indexes = BTreeMap::<GroupReleaseNoteKey, usize>::new();
	for change in changes {
		let key = GroupReleaseNoteKey {
			source_path: change.source_path.clone(),
			summary: change.summary.clone(),
			details: change.details.clone(),
			bump: change.bump,
			change_type: change.change_type.clone(),
			context: change.context.clone(),
		};
		if let Some(index) = indexes.get(&key).copied() {
			if let Some(entry) = aggregated.get_mut(index)
				&& !entry
					.package_labels
					.iter()
					.any(|label| label == &change.package_name)
			{
				entry.package_labels.push(change.package_name.clone());
				entry.package_name = entry.package_labels.join(", ");
			}
			continue;
		}
		let mut change = change;
		change.package_labels = vec![change.package_name.clone()];
		change.package_name = change.package_labels.join(", ");
		indexes.insert(key, aggregated.len());
		aggregated.push(change);
	}
	aggregated
}

fn group_release_summary(group_name: &str) -> Vec<String> {
	vec![format!("Grouped release for `{group_name}`.")]
}

fn build_release_notes_document(
	target_id: &str,
	version: &str,
	summary: Vec<String>,
	changelog: &ChangelogSettings,
	changes: &[ReleaseNoteChange],
) -> ReleaseNotesDocument {
	ReleaseNotesDocument {
		title: version.to_string(),
		summary,
		sections: render_release_note_sections(target_id, version, changelog, changes),
	}
}

fn render_release_note_sections(
	target_id: &str,
	version: &str,
	changelog: &ChangelogSettings,
	changes: &[ReleaseNoteChange],
) -> Vec<ReleaseNotesSection> {
	// Sort sections by priority (lower = earlier)
	let mut sorted_sections: Vec<(&str, &str, i8)> = changelog
		.sections
		.iter()
		.map(|(key, def)| (key.as_str(), def.heading.as_str(), def.priority))
		.collect::<Vec<_>>();
	sorted_sections.sort_by_key(|(_, _, priority)| *priority);

	let collapse_threshold = changelog.section_thresholds.collapse;
	let ignored_threshold = changelog.section_thresholds.ignored;

	let mut section_entries: BTreeMap<&str, Vec<String>> = BTreeMap::new();
	let mut uncategorized = Vec::<String>::new();

	for change in changes {
		let rendered = render_change_entry(change, target_id, version, &changelog.templates);
		let change_type = change.change_type.as_deref().unwrap_or("");
		if let Some(typ) = changelog.types.get(change_type) {
			let section_key = typ.section.as_str();
			let entries = section_entries.entry(section_key).or_default();
			push_unique_release_note_entry(entries, rendered);
		} else {
			push_unique_release_note_entry(&mut uncategorized, rendered);
		}
	}

	// Build sections in priority order
	let mut sections = Vec::new();
	for (section_key, heading, priority) in &sorted_sections {
		if *priority > ignored_threshold {
			section_entries.remove(*section_key);
			continue;
		}
		if let Some(entries) = section_entries.remove(*section_key)
			&& !entries.is_empty()
		{
			sections.push(ReleaseNotesSection {
				title: heading.to_string(),
				collapsed: *priority >= collapse_threshold,
				entries,
			});
		}
	}
	if !uncategorized.is_empty() {
		sections.push(ReleaseNotesSection {
			title: "Changed".to_string(),
			collapsed: false,
			entries: uncategorized,
		});
	}
	if sections.is_empty() {
		sections.push(ReleaseNotesSection {
			title: "Changed".to_string(),
			collapsed: false,
			entries: vec!["- prepare release".to_string()],
		});
		return sections;
	}
	sections
}

fn config_package_id(package: &PackageRecord) -> String {
	package
		.metadata
		.get("config_id")
		.cloned()
		.unwrap_or_else(|| package.name.clone())
}

fn render_change_entry(
	change: &ReleaseNoteChange,
	target_id: &str,
	version: &str,
	change_templates: &[String],
) -> String {
	for template in change_templates
		.iter()
		.map(String::as_str)
		.chain(DEFAULT_CHANGE_TEMPLATES)
	{
		if let Some(rendered) = apply_change_template(template, change, target_id, version) {
			return format_group_labeled_entry(change, &rendered);
		}
	}
	format_group_labeled_entry(change, &format!("- {}", change.summary))
}

fn format_group_labeled_entry(change: &ReleaseNoteChange, rendered: &str) -> String {
	if change.package_labels.is_empty() {
		return rendered.to_string();
	}
	if change.package_labels.len() == 1
		&& !rendered.contains('\n')
		&& let Some(entry) = rendered.strip_prefix("- ")
		&& let Some(package_label) = change.package_labels.first()
	{
		return format!("- **{package_label}**: {entry}");
	}
	let labels = change
		.package_labels
		.iter()
		.map(|package| format!("*{package}*"))
		.collect::<Vec<_>>()
		.join(", ");
	format!("> [!NOTE]\n> {labels}\n\n{rendered}")
}

const DEFAULT_CHANGE_TEMPLATES: [&str; 3] = [
	"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ details }}",
	"- {{ summary }}",
];

fn apply_change_template(
	template: &str,
	change: &ReleaseNoteChange,
	target_id: &str,
	version: &str,
) -> Option<String> {
	let bump = change.bump.to_string();
	let mut context = BTreeMap::<&str, &str>::new();
	context.insert("summary", &change.summary);
	context.insert("package", &change.package_name);
	context.insert("version", version);
	context.insert("target_id", target_id);
	context.insert("bump", &bump);
	if let Some(value) = change.details.as_deref() {
		context.insert("details", value);
	}
	if let Some(value) = change.change_type.as_deref() {
		context.insert("type", value);
	}
	if let Some(value) = change.context.as_deref() {
		context.insert("context", value);
	}
	if let Some(value) = change.changeset_path.as_deref() {
		context.insert("changeset_path", value);
	}
	if let Some(value) = change.change_owner.as_deref() {
		context.insert("change_owner", value);
	}
	if let Some(value) = change.change_owner_link.as_deref() {
		context.insert("change_owner_link", value);
	}
	if let Some(value) = change.review_request.as_deref() {
		context.insert("review_request", value);
	}
	if let Some(value) = change.review_request_link.as_deref() {
		context.insert("review_request_link", value);
	}
	if let Some(value) = change.introduced_commit.as_deref() {
		context.insert("introduced_commit", value);
	}
	if let Some(value) = change.introduced_commit_link.as_deref() {
		context.insert("introduced_commit_link", value);
	}
	if let Some(value) = change.last_updated_commit.as_deref() {
		context.insert("last_updated_commit", value);
	}
	if let Some(value) = change.last_updated_commit_link.as_deref() {
		context.insert("last_updated_commit_link", value);
	}
	if let Some(value) = change.related_issues.as_deref() {
		context.insert("related_issues", value);
	}
	if let Some(value) = change.related_issue_links.as_deref() {
		context.insert("related_issue_links", value);
	}
	if let Some(value) = change.closed_issues.as_deref() {
		context.insert("closed_issues", value);
	}
	if let Some(value) = change.closed_issue_links.as_deref() {
		context.insert("closed_issue_links", value);
	}
	let jinja_context = minijinja::Value::from_serialize(&context);
	let rendered = render_jinja_template_strict(template, &jinja_context).ok()?;
	let rendered = rendered.trim().to_string();
	if rendered.is_empty() {
		None
	} else {
		Some(rendered)
	}
}

fn push_unique_release_note_entry(entries: &mut Vec<String>, entry: String) {
	if !entries.iter().any(|existing| existing == &entry) {
		entries.push(entry);
	}
}

#[cfg(test)]
#[path = "__tests__/changelog_tests.rs"]
mod tests;
