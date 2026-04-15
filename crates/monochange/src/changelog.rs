use typed_builder::TypedBuilder;

use super::*;

#[derive(Clone, Copy, Debug, TypedBuilder)]
pub(crate) struct ChangelogBuildContext<'a> {
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
pub(crate) fn build_changelog_updates(
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
			package_definition.map_or(&[][..], |package| {
				package.extra_changelog_sections.as_slice()
			}),
			&context.configuration.release_notes.change_templates,
			&changes,
		);
		let rendered = render_release_notes(changelog_target.format, &document);
		updates.push(ChangelogUpdate {
			file: FileUpdate {
				path: changelog_target.path.clone(),
				content: append_changelog_section(&changelog_target.path, &rendered)?.into_bytes(),
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
		let changed_members =
			group_changed_members(planned_group, &release_note_changes, context.packages);
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
			group_release_summary(&planned_group.group_id, &member_ids, &changed_members),
			group_definition.map_or(&[][..], |group| group.extra_changelog_sections.as_slice()),
			&context.configuration.release_notes.change_templates,
			&changes,
		);
		let rendered = render_release_notes(changelog_target.format, &document);
		updates.push(ChangelogUpdate {
			file: FileUpdate {
				path: changelog_target.path.clone(),
				content: append_changelog_section(&changelog_target.path, &rendered)?.into_bytes(),
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

fn append_changelog_section(path: &Path, section: &str) -> MonochangeResult<String> {
	let current = if path.exists() {
		fs::read_to_string(path).map_err(|error| {
			MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
		})?
	} else {
		String::new()
	};

	let mut content = current.trim_end().to_string();

	if !content.is_empty() {
		content.push_str("\n\n");
	}

	content.push_str(section);
	content.push('\n');

	Ok(content)
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

pub(crate) fn render_jinja_template(
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

pub(crate) fn render_message_template(template: &str, metadata: &BTreeMap<&str, String>) -> String {
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

pub(crate) fn filter_group_release_note_change(
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

pub(crate) fn group_changelog_include_allows(
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

pub(crate) fn render_group_filtered_update_message(group_id: &str) -> String {
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
			let entry = &mut aggregated[index];
			if !entry
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

fn group_changed_members(
	planned_group: &monochange_core::PlannedVersionGroup,
	release_note_changes: &BTreeMap<String, Vec<ReleaseNoteChange>>,
	packages: &[PackageRecord],
) -> BTreeSet<String> {
	planned_group
		.members
		.iter()
		.filter(|member_id| {
			release_note_changes
				.get(*member_id)
				.is_some_and(|changes| !changes.is_empty())
		})
		.filter_map(|member_id| {
			packages
				.iter()
				.find(|package| package.id == *member_id)
				.map(config_package_id)
		})
		.collect()
}

fn group_release_summary(
	group_name: &str,
	members: &[String],
	changed_members: &BTreeSet<String>,
) -> Vec<String> {
	let mut summary = vec![format!("Grouped release for `{group_name}`.")];
	if members.is_empty() {
		return summary;
	}
	if changed_members.is_empty() {
		summary.push(format!("Members: {}", members.join(", ")));
		return summary;
	}
	let changed = members
		.iter()
		.filter(|member| changed_members.contains(member.as_str()))
		.cloned()
		.collect::<Vec<_>>();
	let synchronized = members
		.iter()
		.filter(|member| !changed_members.contains(member.as_str()))
		.cloned()
		.collect::<Vec<_>>();
	if !changed.is_empty() {
		summary.push(format!("Changed members: {}", changed.join(", ")));
	}
	if !synchronized.is_empty() {
		summary.push(format!("Synchronized members: {}", synchronized.join(", ")));
	}
	summary
}

fn build_release_notes_document(
	target_id: &str,
	version: &str,
	summary: Vec<String>,
	extra_sections: &[ExtraChangelogSection],
	change_templates: &[String],
	changes: &[ReleaseNoteChange],
) -> ReleaseNotesDocument {
	ReleaseNotesDocument {
		title: version.to_string(),
		summary,
		sections: render_release_note_sections(
			target_id,
			version,
			extra_sections,
			change_templates,
			changes,
		),
	}
}

fn render_release_note_sections(
	target_id: &str,
	version: &str,
	extra_sections: &[ExtraChangelogSection],
	change_templates: &[String],
	changes: &[ReleaseNoteChange],
) -> Vec<ReleaseNotesSection> {
	let overridden_builtins = extra_sections
		.iter()
		.flat_map(|section| {
			section
				.types
				.iter()
				.map(|change_type| change_type.trim().to_string())
		})
		.collect::<BTreeSet<_>>();
	let resolved_extra_sections = extra_sections
		.iter()
		.map(|section| {
			ResolvedSectionDefinition {
				title: section.name.clone(),
				types: section.types.clone(),
			}
		})
		.collect::<Vec<_>>();
	let mut builtin_entries = BTreeMap::<BuiltinReleaseSection, Vec<String>>::new();
	let mut extra_entries = vec![Vec::<String>::new(); resolved_extra_sections.len()];

	for change in changes {
		let rendered = render_change_entry(change, target_id, version, change_templates);
		match classify_release_note_change(change, &resolved_extra_sections) {
			ResolvedReleaseSectionTarget::Builtin(section) => {
				push_unique_release_note_entry(
					builtin_entries.entry(section).or_default(),
					rendered,
				);
			}
			ResolvedReleaseSectionTarget::Extra(index) => {
				push_unique_release_note_entry(&mut extra_entries[index], rendered);
			}
		}
	}

	let mut sections = Vec::new();
	for builtin in builtin_release_sections() {
		if overridden_builtins.contains(builtin.selector()) {
			continue;
		}
		if let Some(entries) = builtin_entries
			.remove(&builtin)
			.filter(|entries| !entries.is_empty())
		{
			sections.push(ReleaseNotesSection {
				title: builtin.title().to_string(),
				entries,
			});
		}
	}
	for (index, section) in resolved_extra_sections.iter().enumerate() {
		if extra_entries[index].is_empty() {
			continue;
		}
		sections.push(ReleaseNotesSection {
			title: section.title.clone(),
			entries: extra_entries[index].clone(),
		});
	}
	if sections.is_empty() {
		sections.push(ReleaseNotesSection {
			title: "Changed".to_string(),
			entries: vec!["- prepare release".to_string()],
		});
	}
	sections
}

#[allow(variant_size_differences)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ResolvedReleaseSectionTarget {
	Builtin(BuiltinReleaseSection),
	Extra(usize),
}

fn classify_release_note_change(
	change: &ReleaseNoteChange,
	extra_sections: &[ResolvedSectionDefinition],
) -> ResolvedReleaseSectionTarget {
	if let Some(change_type) = change.change_type.as_deref()
		&& let Some(index) = extra_sections
			.iter()
			.position(|section| section_matches_resolved_type(section, change_type))
	{
		return ResolvedReleaseSectionTarget::Extra(index);
	}

	if change.change_type.as_deref() == Some(BuiltinReleaseSection::Note.selector()) {
		return ResolvedReleaseSectionTarget::Builtin(BuiltinReleaseSection::Note);
	}
	let builtin = BuiltinReleaseSection::from_bump(change.bump);
	if let Some(index) = extra_sections
		.iter()
		.position(|section| section_matches_resolved_type(section, builtin.selector()))
	{
		return ResolvedReleaseSectionTarget::Extra(index);
	}
	ResolvedReleaseSectionTarget::Builtin(builtin)
}

fn section_matches_resolved_type(section: &ResolvedSectionDefinition, change_type: &str) -> bool {
	section
		.types
		.iter()
		.any(|candidate| candidate.trim() == change_type)
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
	{
		return format!("- **{}**: {}", change.package_labels[0], entry);
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

fn config_package_id(package: &PackageRecord) -> String {
	package
		.metadata
		.get("config_id")
		.cloned()
		.unwrap_or_else(|| package.name.clone())
}

impl BuiltinReleaseSection {
	#[allow(clippy::match_same_arms)]
	fn from_bump(bump: BumpSeverity) -> Self {
		match bump {
			BumpSeverity::Major => Self::Major,
			BumpSeverity::Minor => Self::Minor,
			BumpSeverity::None | BumpSeverity::Patch => Self::Patch,
			_ => Self::Patch,
		}
	}

	fn selector(self) -> &'static str {
		match self {
			Self::Major => "major",
			Self::Minor => "minor",
			Self::Patch => "patch",
			Self::Note => "note",
		}
	}

	fn title(self) -> &'static str {
		match self {
			Self::Major => "Breaking changes",
			Self::Minor => "Features",
			Self::Patch => "Fixes",
			Self::Note => "Notes",
		}
	}
}

fn builtin_release_sections() -> [BuiltinReleaseSection; 4] {
	[
		BuiltinReleaseSection::Major,
		BuiltinReleaseSection::Minor,
		BuiltinReleaseSection::Patch,
		BuiltinReleaseSection::Note,
	]
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::collections::BTreeSet;
	use std::fs;

	use monochange_core::ChangeSignal;
	use monochange_core::ChangelogFormat;
	use monochange_core::ChangelogTarget;
	use monochange_core::ChangesetContext;
	use monochange_core::ChangesetRevision;
	use monochange_core::ChangesetTargetKind;
	use monochange_core::Ecosystem;
	use monochange_core::ExtraChangelogSection;
	use monochange_core::GroupChangelogInclude;
	use monochange_core::GroupDefinition;
	use monochange_core::HostedActorRef;
	use monochange_core::HostedActorSourceKind;
	use monochange_core::HostedCommitRef;
	use monochange_core::HostedIssueRef;
	use monochange_core::HostedIssueRelationshipKind;
	use monochange_core::HostedReviewRequestKind;
	use monochange_core::HostedReviewRequestRef;
	use monochange_core::HostingProviderKind;
	use monochange_core::PackageDefinition;
	use monochange_core::PackageRecord;
	use monochange_core::PackageType;
	use monochange_core::PlannedVersionGroup;
	use monochange_core::PreparedChangeset;
	use monochange_core::PreparedChangesetTarget;
	use monochange_core::PublishState;
	use monochange_core::ReleaseDecision;
	use monochange_core::ReleaseNotesSettings;
	use monochange_core::VersionFormat;
	use monochange_core::WorkspaceConfiguration;
	use monochange_core::WorkspaceDefaults;
	use semver::Version;
	use tempfile::tempdir;

	use super::*;

	fn empty_configuration(root: &Path) -> WorkspaceConfiguration {
		WorkspaceConfiguration {
			root_path: root.to_path_buf(),
			defaults: WorkspaceDefaults::default(),
			release_notes: ReleaseNotesSettings::default(),
			packages: Vec::new(),
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
			cargo: monochange_core::EcosystemSettings::default(),
			npm: monochange_core::EcosystemSettings::default(),
			deno: monochange_core::EcosystemSettings::default(),
			dart: monochange_core::EcosystemSettings::default(),
		}
	}

	fn sample_package_record(root: &Path, config_id: &str, name: &str) -> PackageRecord {
		let manifest_dir = root.join("packages").join(config_id);
		fs::create_dir_all(&manifest_dir)
			.unwrap_or_else(|error| panic!("create manifest dir: {error}"));
		let manifest_path = manifest_dir.join("package.json");
		fs::write(&manifest_path, "{}\n")
			.unwrap_or_else(|error| panic!("write manifest file: {error}"));

		let mut package = PackageRecord::new(
			Ecosystem::Npm,
			name,
			manifest_path,
			root.to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		);
		package
			.metadata
			.insert("config_id".to_string(), config_id.to_string());
		package
	}

	fn sample_package_definition(config_id: &str) -> PackageDefinition {
		PackageDefinition {
			id: config_id.to_string(),
			path: PathBuf::from(format!("packages/{config_id}")),
			package_type: PackageType::Npm,
			changelog: Some(ChangelogTarget {
				path: PathBuf::from(format!("packages/{config_id}/CHANGELOG.md")),
				format: ChangelogFormat::Monochange,
			}),
			extra_changelog_sections: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			ignore_ecosystem_versioned_files: false,
			ignored_paths: Vec::new(),
			additional_paths: Vec::new(),
			tag: true,
			release: true,
			publish: monochange_core::PublishSettings::default(),
			version_format: VersionFormat::Namespaced,
		}
	}

	fn sample_group_definition(include: GroupChangelogInclude) -> GroupDefinition {
		GroupDefinition {
			id: "sdk".to_string(),
			packages: vec!["pkg-a".to_string(), "pkg-b".to_string()],
			changelog: Some(ChangelogTarget {
				path: PathBuf::from("groups/sdk/CHANGELOG.md"),
				format: ChangelogFormat::Monochange,
			}),
			changelog_include: include,
			extra_changelog_sections: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Namespaced,
		}
	}

	fn sample_decision(package_id: &str, group_id: Option<&str>) -> ReleaseDecision {
		ReleaseDecision {
			package_id: package_id.to_string(),
			trigger_type: "changeset".to_string(),
			recommended_bump: BumpSeverity::Patch,
			planned_version: Some(Version::new(1, 2, 3)),
			group_id: group_id.map(ToString::to_string),
			reasons: vec!["covered".to_string()],
			upstream_sources: Vec::new(),
			warnings: Vec::new(),
		}
	}

	fn sample_group(member_ids: Vec<String>) -> PlannedVersionGroup {
		PlannedVersionGroup {
			group_id: "sdk".to_string(),
			display_name: "SDK".to_string(),
			members: member_ids,
			mismatch_detected: false,
			planned_version: Some(Version::new(2, 0, 0)),
			recommended_bump: BumpSeverity::Minor,
		}
	}

	fn sample_change(package_id: &str, package_name: &str, source_path: &str) -> ReleaseNoteChange {
		ReleaseNoteChange {
			package_id: package_id.to_string(),
			package_name: package_name.to_string(),
			package_labels: Vec::new(),
			source_path: Some(source_path.to_string()),
			summary: "Added release note support".to_string(),
			details: Some("Detailed explanation".to_string()),
			bump: BumpSeverity::Minor,
			change_type: Some("note".to_string()),
			context: Some("> _Owner:_ @octocat".to_string()),
			changeset_path: Some(source_path.to_string()),
			change_owner: Some("@octocat".to_string()),
			change_owner_link: Some("[@octocat](https://example.com/octocat)".to_string()),
			review_request: Some("PR 42".to_string()),
			review_request_link: Some("[PR 42](https://example.com/pr/42)".to_string()),
			introduced_commit: Some("abc1234".to_string()),
			introduced_commit_link: Some(
				"[`abc1234`](https://example.com/commit/abc1234)".to_string(),
			),
			last_updated_commit: Some("def5678".to_string()),
			last_updated_commit_link: Some(
				"[`def5678`](https://example.com/commit/def5678)".to_string(),
			),
			related_issues: Some("#10".to_string()),
			related_issue_links: Some("[#10](https://example.com/issues/10)".to_string()),
			closed_issues: Some("#20".to_string()),
			closed_issue_links: Some("[#20](https://example.com/issues/20)".to_string()),
		}
	}

	#[test]
	fn changelog_file_helpers_append_and_deduplicate_updates() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let changelog_path = tempdir.path().join("CHANGELOG.md");

		let first = append_changelog_section(&changelog_path, "## 1.0.0\n- initial")
			.unwrap_or_else(|error| panic!("append first changelog section: {error}"));
		assert_eq!(first, "## 1.0.0\n- initial\n");

		fs::write(&changelog_path, "# Changelog\n\n## 0.9.0\n- older\n")
			.unwrap_or_else(|error| panic!("write existing changelog: {error}"));
		let appended = append_changelog_section(&changelog_path, "## 1.0.0\n- latest")
			.unwrap_or_else(|error| panic!("append second changelog section: {error}"));
		assert_eq!(
			appended,
			"# Changelog\n\n## 0.9.0\n- older\n\n## 1.0.0\n- latest\n"
		);

		let earlier = ChangelogUpdate {
			file: FileUpdate {
				path: changelog_path.clone(),
				content: b"old".to_vec(),
			},
			owner_id: "pkg-a".to_string(),
			owner_kind: ReleaseOwnerKind::Package,
			format: ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "0.9.0".to_string(),
				summary: Vec::new(),
				sections: Vec::new(),
			},
			rendered: "old".to_string(),
		};
		let latest = ChangelogUpdate {
			file: FileUpdate {
				path: changelog_path.clone(),
				content: b"new".to_vec(),
			},
			owner_id: "pkg-a".to_string(),
			owner_kind: ReleaseOwnerKind::Package,
			format: ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "1.0.0".to_string(),
				summary: Vec::new(),
				sections: Vec::new(),
			},
			rendered: "new".to_string(),
		};
		let unique_path = tempdir.path().join("OTHER.md");
		let unique = ChangelogUpdate {
			file: FileUpdate {
				path: unique_path.clone(),
				content: b"other".to_vec(),
			},
			owner_id: "pkg-b".to_string(),
			owner_kind: ReleaseOwnerKind::Package,
			format: ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "1.0.0".to_string(),
				summary: Vec::new(),
				sections: Vec::new(),
			},
			rendered: "other".to_string(),
		};

		let deduped = dedup_changelog_updates(vec![earlier, latest.clone(), unique]);
		assert_eq!(deduped.len(), 2);
		assert!(deduped.iter().any(|update| update.file.path == unique_path));
		assert!(deduped.iter().any(|update| {
			update.file.path == changelog_path && update.file.content == latest.file.content
		}));
	}

	#[test]
	fn rendered_changeset_context_and_signal_mapping_cover_hosted_metadata() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let changeset_path = tempdir.path().join(".changeset/feature.md");
		fs::create_dir_all(changeset_path.parent().unwrap_or_else(|| Path::new(".")))
			.unwrap_or_else(|error| panic!("create changeset dir: {error}"));
		fs::write(&changeset_path, "feature\n")
			.unwrap_or_else(|error| panic!("write changeset file: {error}"));

		let changeset = PreparedChangeset {
			path: changeset_path.clone(),
			summary: Some("summary".to_string()),
			details: Some("details".to_string()),
			targets: vec![PreparedChangesetTarget {
				id: "pkg-a".to_string(),
				kind: ChangesetTargetKind::Package,
				bump: Some(BumpSeverity::Minor),
				origin: "manual".to_string(),
				evidence_refs: Vec::new(),
				change_type: Some("note".to_string()),
			}],
			context: Some(ChangesetContext {
				provider: HostingProviderKind::GitHub,
				host: Some("github.com".to_string()),
				capabilities: HostingCapabilities::default(),
				introduced: Some(ChangesetRevision {
					actor: Some(HostedActorRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						id: Some("1".to_string()),
						login: Some("octocat".to_string()),
						display_name: Some("Octo Cat".to_string()),
						url: Some("https://example.com/octocat".to_string()),
						source: HostedActorSourceKind::ReviewRequestAuthor,
					}),
					commit: Some(HostedCommitRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						sha: "abcdef123456".to_string(),
						short_sha: "abcdef1".to_string(),
						url: Some("https://example.com/commit/abcdef1".to_string()),
						authored_at: None,
						committed_at: None,
						author_name: None,
						author_email: None,
					}),
					review_request: Some(HostedReviewRequestRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						kind: HostedReviewRequestKind::PullRequest,
						id: "42".to_string(),
						title: Some("release notes".to_string()),
						url: Some("https://example.com/pull/42".to_string()),
						author: None,
					}),
				}),
				last_updated: Some(ChangesetRevision {
					actor: Some(HostedActorRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						id: None,
						login: None,
						display_name: Some("Release Bot".to_string()),
						url: None,
						source: HostedActorSourceKind::CommitAuthor,
					}),
					commit: Some(HostedCommitRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						sha: "9876543210ab".to_string(),
						short_sha: "9876543".to_string(),
						url: Some("https://example.com/commit/9876543".to_string()),
						authored_at: None,
						committed_at: None,
						author_name: None,
						author_email: None,
					}),
					review_request: Some(HostedReviewRequestRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						kind: HostedReviewRequestKind::MergeRequest,
						id: "77".to_string(),
						title: None,
						url: None,
						author: None,
					}),
				}),
				related_issues: vec![
					HostedIssueRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						id: "#123".to_string(),
						title: Some("closed".to_string()),
						url: Some("https://example.com/issues/123".to_string()),
						relationship: HostedIssueRelationshipKind::ClosedByReviewRequest,
					},
					HostedIssueRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						id: "#456".to_string(),
						title: Some("related".to_string()),
						url: None,
						relationship: HostedIssueRelationshipKind::ReferencedByReviewRequest,
					},
				],
			}),
		};

		let rendered = build_rendered_changeset_context(tempdir.path(), &changeset);
		assert!(
			rendered
				.context
				.contains("> _Owner:_ [@octocat](https://example.com/octocat)")
		);
		assert!(
			rendered
				.context
				.contains("> _Review:_ [PR 42](https://example.com/pull/42)")
		);
		assert!(
			rendered
				.context
				.contains("> _Introduced in:_ [`abcdef1`](https://example.com/commit/abcdef1)")
		);
		assert!(
			rendered
				.context
				.contains("> _Last updated in:_ [`9876543`](https://example.com/commit/9876543)")
		);
		assert!(
			rendered
				.context
				.contains("> _Closed issues:_ [#123](https://example.com/issues/123)")
		);
		assert!(rendered.context.contains("> _Related issues:_ #456"));
		assert_eq!(rendered.change_owner.as_deref(), Some("@octocat"));
		assert_eq!(rendered.review_request.as_deref(), Some("PR 42"));
		assert_eq!(rendered.closed_issues.as_deref(), Some("#123"));
		assert_eq!(rendered.related_issues.as_deref(), Some("#456"));

		let package = sample_package_record(tempdir.path(), "pkg-a", "package-a");
		let signal = ChangeSignal {
			package_id: package.id.clone(),
			requested_bump: Some(BumpSeverity::Minor),
			explicit_version: None,
			change_origin: "changeset".to_string(),
			evidence_refs: vec!["manual".to_string()],
			notes: Some("Added release note support".to_string()),
			details: Some("Detailed explanation".to_string()),
			change_type: Some("note".to_string()),
			source_path: changeset_path.clone(),
		};
		let source_path = root_relative(tempdir.path(), &changeset.path);
		let mapped = build_release_note_change(
			&signal,
			std::slice::from_ref(&package),
			tempdir.path(),
			&BTreeMap::from([(source_path, rendered.clone())]),
		)
		.unwrap_or_else(|| panic!("expected mapped release note change"));
		assert_eq!(mapped.package_name, "pkg-a");
		assert_eq!(mapped.change_owner.as_deref(), Some("@octocat"));
		assert_eq!(
			mapped.review_request_link.as_deref(),
			Some("[PR 42](https://example.com/pull/42)")
		);
		assert_eq!(mapped.related_issue_links.as_deref(), Some("#456"));

		let no_notes = ChangeSignal {
			notes: None,
			..signal
		};
		assert!(
			build_release_note_change(&no_notes, &[package], tempdir.path(), &BTreeMap::new())
				.is_none()
		);
	}

	#[test]
	fn render_helpers_cover_actor_labels_links_sections_and_templates() {
		assert_eq!(
			render_actor_label(&HostedActorRef {
				login: Some("octocat".to_string()),
				source: HostedActorSourceKind::CommitAuthor,
				..HostedActorRef::default()
			}),
			"@octocat"
		);
		assert_eq!(
			render_actor_label(&HostedActorRef {
				display_name: Some("Release Bot".to_string()),
				source: HostedActorSourceKind::CommitAuthor,
				..HostedActorRef::default()
			}),
			"Release Bot"
		);
		assert_eq!(
			render_actor_label(&HostedActorRef {
				source: HostedActorSourceKind::CommitAuthor,
				..HostedActorRef::default()
			}),
			"unknown"
		);
		assert_eq!(
			render_review_request_label(&HostedReviewRequestRef {
				kind: HostedReviewRequestKind::PullRequest,
				id: "12".to_string(),
				..HostedReviewRequestRef::default()
			}),
			"PR 12"
		);
		assert_eq!(
			render_review_request_label(&HostedReviewRequestRef {
				kind: HostedReviewRequestKind::MergeRequest,
				id: "9".to_string(),
				..HostedReviewRequestRef::default()
			}),
			"MR 9"
		);
		assert_eq!(render_markdown_link("plain", None), "plain");
		assert_eq!(
			render_markdown_link("linked", Some("https://example.com")),
			"[linked](https://example.com)"
		);
		let issues = [
			HostedIssueRef {
				id: "#1".to_string(),
				url: Some("https://example.com/issues/1".to_string()),
				relationship: HostedIssueRelationshipKind::Mentioned,
				..HostedIssueRef::default()
			},
			HostedIssueRef {
				id: "#2".to_string(),
				url: None,
				relationship: HostedIssueRelationshipKind::Manual,
				..HostedIssueRef::default()
			},
		];
		let issue_refs = issues.iter().collect::<Vec<_>>();
		assert_eq!(render_issue_labels(&issue_refs), "#1, #2");
		assert_eq!(
			render_issue_links(&issue_refs),
			"[#1](https://example.com/issues/1), #2"
		);

		let mut multi_label_change = sample_change("pkg-a", "pkg-a", ".changeset/a.md");
		multi_label_change.package_labels = vec!["pkg-a".to_string(), "pkg-b".to_string()];
		let block = format_group_labeled_entry(&multi_label_change, "#### Summary\n\nMore");
		assert!(block.contains("> [!NOTE]"));
		assert!(block.contains("*pkg-a*, *pkg-b*"));

		let mut single_label_change = sample_change("pkg-a", "pkg-a", ".changeset/a.md");
		single_label_change.package_labels = vec!["pkg-a".to_string()];
		assert_eq!(
			format_group_labeled_entry(&single_label_change, "- Added release note support"),
			"- **pkg-a**: Added release note support"
		);

		let rendered = apply_change_template(
			"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}\n\n{{ change_owner_link }}\n\n{{ review_request_link }}\n\n{{ introduced_commit_link }}\n\n{{ last_updated_commit_link }}\n\n{{ related_issue_links }}\n\n{{ closed_issue_links }}",
			&sample_change("pkg-a", "pkg-a", ".changeset/a.md"),
			"sdk",
			"1.2.3",
		)
		.unwrap_or_else(|| panic!("expected template to render"));
		assert!(rendered.contains("Detailed explanation"));
		assert!(rendered.contains("[@octocat](https://example.com/octocat)"));
		assert!(rendered.contains("[PR 42](https://example.com/pr/42)"));
		assert!(rendered.contains("[#10](https://example.com/issues/10)"));
		assert!(rendered.contains("[#20](https://example.com/issues/20)"));
		assert!(
			apply_change_template(
				"{{ missing_value }}",
				&sample_change("pkg-a", "pkg-a", ".changeset/a.md"),
				"sdk",
				"1.2.3"
			)
			.is_none()
		);
		assert!(
			apply_change_template(
				"   ",
				&sample_change("pkg-a", "pkg-a", ".changeset/a.md"),
				"sdk",
				"1.2.3"
			)
			.is_none()
		);

		let extra_sections = vec![
			ExtraChangelogSection {
				name: "Highlights".to_string(),
				types: vec!["minor".to_string()],
				default_bump: None,
				description: None,
			},
			ExtraChangelogSection {
				name: "Notes".to_string(),
				types: vec!["note".to_string()],
				default_bump: None,
				description: None,
			},
		];
		let sections = render_release_note_sections(
			"sdk",
			"1.2.3",
			&extra_sections,
			&["- {{ summary }}".to_string()],
			&[
				ReleaseNoteChange {
					change_type: Some("note".to_string()),
					..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
				},
				ReleaseNoteChange {
					change_type: Some("minor".to_string()),
					bump: BumpSeverity::Minor,
					summary: "Added group support".to_string(),
					..sample_change("pkg-b", "pkg-b", ".changeset/b.md")
				},
				ReleaseNoteChange {
					change_type: None,
					bump: BumpSeverity::Major,
					summary: "Breaking API".to_string(),
					..sample_change("pkg-c", "pkg-c", ".changeset/c.md")
				},
				ReleaseNoteChange {
					change_type: None,
					bump: BumpSeverity::Patch,
					summary: "Bug fix".to_string(),
					..sample_change("pkg-d", "pkg-d", ".changeset/d.md")
				},
				ReleaseNoteChange {
					change_type: None,
					bump: BumpSeverity::Patch,
					summary: "Bug fix".to_string(),
					..sample_change("pkg-d", "pkg-d", ".changeset/d.md")
				},
			],
		);
		assert_eq!(sections[0].title, "Breaking changes");
		assert_eq!(sections[1].title, "Fixes");
		assert_eq!(sections[2].title, "Highlights");
		assert_eq!(sections[3].title, "Notes");
		assert_eq!(sections[1].entries, vec!["- Bug fix".to_string()]);
		assert_eq!(
			sections[2].entries,
			vec!["- Added group support".to_string()]
		);

		let fallback = render_release_note_sections("sdk", "1.2.3", &[], &[], &[]);
		assert_eq!(fallback[0].title, "Changed");
		assert_eq!(fallback[0].entries, vec!["- prepare release".to_string()]);

		let document = build_release_notes_document(
			"sdk",
			"1.2.3",
			vec!["Summary".to_string()],
			&extra_sections,
			&["- {{ summary }}".to_string()],
			&[sample_change("pkg-a", "pkg-a", ".changeset/a.md")],
		);
		assert_eq!(document.title, "1.2.3");
		assert_eq!(document.summary, vec!["Summary".to_string()]);
		assert!(!document.sections.is_empty());
	}

	#[test]
	fn package_and_group_release_note_helpers_cover_empty_filtered_and_aggregated_paths() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		let package_a = sample_package_record(root, "pkg-a", "package-a");
		let package_b = sample_package_record(root, "pkg-b", "package-b");
		let mut configuration = empty_configuration(root);
		configuration.defaults.empty_update_message =
			Some("Default release for {{ package }} {{ version }}".to_string());
		let mut package_definition = sample_package_definition("pkg-a");
		package_definition.empty_update_message =
			Some("Package release for {{ package }} in {{ group }} -> {{ version }}".to_string());
		let mut group_definition =
			sample_group_definition(GroupChangelogInclude::Selected(BTreeSet::from([
				"pkg-a".to_string()
			])));
		group_definition.empty_update_message = Some(
			"Group {{ group }} now at {{ version }} with {{ member_count }} members".to_string(),
		);
		configuration.packages = vec![package_definition.clone()];
		configuration.groups = vec![group_definition.clone()];

		let package_changes = package_release_note_changes(
			&configuration,
			Some(&package_definition),
			Some(&group_definition),
			&sample_decision("pkg-a", Some("sdk")),
			&package_a,
			None,
			"1.2.3",
		);
		assert_eq!(package_changes.len(), 1);
		assert!(
			package_changes[0]
				.summary
				.contains("Package release for package-a in sdk -> 1.2.3")
		);

		let direct_change = sample_change("pkg-a", "pkg-a", ".changeset/a.md");
		let direct = package_release_note_changes(
			&configuration,
			Some(&package_definition),
			Some(&group_definition),
			&sample_decision("pkg-a", Some("sdk")),
			&package_a,
			Some(&vec![direct_change.clone()]),
			"1.2.3",
		);
		assert_eq!(direct, vec![direct_change.clone()]);

		let group = sample_group(vec![package_a.id.clone(), package_b.id.clone()]);
		let group_empty = group_release_note_changes(
			&configuration,
			Some(&group_definition),
			&group,
			&BTreeMap::new(),
			&BTreeMap::new(),
			&[package_a.clone(), package_b.clone()],
			"2.0.0",
		);
		assert_eq!(group_empty.len(), 1);
		assert!(
			group_empty[0]
				.summary
				.contains("Group sdk now at 2.0.0 with 2 members")
		);

		let changes_by_package = BTreeMap::from([
			(
				package_a.id.clone(),
				vec![
					sample_change(&package_a.id, "pkg-a", ".changeset/shared.md"),
					sample_change(&package_a.id, "pkg-a", ".changeset/shared.md"),
				],
			),
			(
				package_b.id.clone(),
				vec![sample_change(
					&package_b.id,
					"pkg-b",
					".changeset/shared.md",
				)],
			),
		]);
		let targets_by_path = BTreeMap::from([(
			PathBuf::from(".changeset/shared.md"),
			vec![
				PreparedChangesetTarget {
					id: "pkg-a".to_string(),
					kind: ChangesetTargetKind::Package,
					bump: Some(BumpSeverity::Minor),
					origin: "changeset".to_string(),
					evidence_refs: Vec::new(),
					change_type: None,
				},
				PreparedChangesetTarget {
					id: "pkg-b".to_string(),
					kind: ChangesetTargetKind::Package,
					bump: Some(BumpSeverity::Minor),
					origin: "changeset".to_string(),
					evidence_refs: Vec::new(),
					change_type: None,
				},
			],
		)]);
		let aggregate_group_definition = sample_group_definition(GroupChangelogInclude::All);
		let grouped = group_release_note_changes(
			&configuration,
			Some(&aggregate_group_definition),
			&group,
			&changes_by_package,
			&targets_by_path,
			&[package_a.clone(), package_b.clone()],
			"2.0.0",
		);
		assert_eq!(grouped.len(), 1);
		assert_eq!(
			grouped[0].package_labels,
			vec!["pkg-a".to_string(), "pkg-b".to_string()]
		);
		assert_eq!(grouped[0].package_name, "pkg-a, pkg-b");

		let selected_filtered = group_release_note_changes(
			&configuration,
			Some(&group_definition),
			&group,
			&changes_by_package,
			&targets_by_path,
			&[package_a.clone(), package_b.clone()],
			"2.0.0",
		);
		assert_eq!(selected_filtered.len(), 1);
		assert!(
			selected_filtered[0]
				.summary
				.contains("No group-facing notes were recorded")
		);

		let group_only_definition = sample_group_definition(GroupChangelogInclude::GroupOnly);
		let filtered = group_release_note_changes(
			&configuration,
			Some(&group_only_definition),
			&group,
			&changes_by_package,
			&targets_by_path,
			&[package_a.clone(), package_b.clone()],
			"2.0.0",
		);
		assert_eq!(filtered.len(), 1);
		assert!(
			filtered[0]
				.summary
				.contains("No group-facing notes were recorded")
		);

		let mut include_targets = BTreeSet::new();
		include_targets.insert("pkg-a".to_string());
		assert!(group_changelog_include_allows(
			&GroupChangelogInclude::All,
			&include_targets
		));
		assert!(!group_changelog_include_allows(
			&GroupChangelogInclude::GroupOnly,
			&include_targets,
		));
		assert!(group_changelog_include_allows(
			&GroupChangelogInclude::Selected(BTreeSet::from(["pkg-a".to_string()])),
			&include_targets,
		));
		assert!(!group_changelog_include_allows(
			&GroupChangelogInclude::Selected(BTreeSet::from(["pkg-b".to_string()])),
			&include_targets,
		));

		let group_target_map = BTreeMap::from([(
			PathBuf::from(".changeset/group.md"),
			vec![PreparedChangesetTarget {
				id: "sdk".to_string(),
				kind: ChangesetTargetKind::Group,
				bump: Some(BumpSeverity::Minor),
				origin: "changeset".to_string(),
				evidence_refs: Vec::new(),
				change_type: None,
			}],
		)]);
		let group_target_change = sample_change("pkg-a", "pkg-a", ".changeset/group.md");
		let filtered_group_target = filter_group_release_note_change(
			&group_target_change,
			Some(&group_definition),
			&group,
			&group_target_map,
		)
		.unwrap_or_else(|| panic!("expected group target to be included"));
		assert_eq!(filtered_group_target.package_name, "sdk");
		assert!(
			filter_group_release_note_change(
				&ReleaseNoteChange {
					source_path: Some(".changeset/unknown.md".to_string()),
					..sample_change("pkg-a", "pkg-a", ".changeset/group.md")
				},
				Some(&group_definition),
				&group,
				&group_target_map,
			)
			.is_none()
		);

		assert_eq!(
			group_release_summary("sdk", &[], &BTreeSet::new()),
			vec!["Grouped release for `sdk`.".to_string()]
		);
		assert_eq!(
			group_release_summary(
				"sdk",
				&["pkg-a".to_string(), "pkg-b".to_string()],
				&BTreeSet::new(),
			),
			vec![
				"Grouped release for `sdk`.".to_string(),
				"Members: pkg-a, pkg-b".to_string()
			]
		);
		assert_eq!(
			group_release_summary(
				"sdk",
				&["pkg-a".to_string(), "pkg-b".to_string()],
				&BTreeSet::from(["pkg-a".to_string()]),
			),
			vec![
				"Grouped release for `sdk`.".to_string(),
				"Changed members: pkg-a".to_string(),
				"Synchronized members: pkg-b".to_string()
			]
		);

		assert_eq!(
			group_changed_members(
				&group,
				&BTreeMap::from([(package_a.id.clone(), vec![direct_change])]),
				&[package_a.clone(), package_b.clone()],
			),
			BTreeSet::from(["pkg-a".to_string()])
		);
		assert_eq!(
			render_group_filtered_update_message("sdk"),
			"No group-facing notes were recorded for this release. Member packages were updated as part of the synchronized group `sdk` version, but their changes are not configured for inclusion in this changelog."
		);
	}

	#[test]
	fn config_and_section_helpers_cover_package_ids_and_section_resolution() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let mut configured = sample_package_record(tempdir.path(), "pkg-a", "package-a");
		assert_eq!(config_package_id(&configured), "pkg-a");
		configured.metadata.clear();
		assert_eq!(config_package_id(&configured), "package-a");
		assert_eq!(
			BuiltinReleaseSection::from_bump(BumpSeverity::Major),
			BuiltinReleaseSection::Major
		);
		assert_eq!(
			BuiltinReleaseSection::from_bump(BumpSeverity::Minor),
			BuiltinReleaseSection::Minor
		);
		assert_eq!(
			BuiltinReleaseSection::from_bump(BumpSeverity::Patch),
			BuiltinReleaseSection::Patch
		);
		assert_eq!(
			BuiltinReleaseSection::from_bump(BumpSeverity::None),
			BuiltinReleaseSection::Patch
		);
		assert_eq!(BuiltinReleaseSection::Note.title(), "Notes");
		assert_eq!(builtin_release_sections().len(), 4);

		let selected = ResolvedSectionDefinition {
			title: "Selected".to_string(),
			types: vec!["custom".to_string(), " minor ".to_string()],
		};
		assert!(section_matches_resolved_type(&selected, "custom"));
		assert!(section_matches_resolved_type(&selected, "minor"));
		assert_eq!(
			classify_release_note_change(
				&ReleaseNoteChange {
					change_type: Some("note".to_string()),
					..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
				},
				std::slice::from_ref(&selected),
			),
			ResolvedReleaseSectionTarget::Builtin(BuiltinReleaseSection::Note)
		);
		assert_eq!(
			classify_release_note_change(
				&ReleaseNoteChange {
					change_type: Some("custom".to_string()),
					..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
				},
				std::slice::from_ref(&selected),
			),
			ResolvedReleaseSectionTarget::Extra(0)
		);
		assert_eq!(
			classify_release_note_change(
				&ReleaseNoteChange {
					change_type: None,
					bump: BumpSeverity::Minor,
					..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
				},
				&[selected],
			),
			ResolvedReleaseSectionTarget::Extra(0)
		);
	}
}
