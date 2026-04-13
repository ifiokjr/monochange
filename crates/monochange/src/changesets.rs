use std::collections::BTreeMap;

use super::*;

pub(crate) fn diagnose_changesets(
	root: &Path,
	requested: &[String],
) -> MonochangeResult<ChangesetDiagnosticsReport> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;

	let changeset_paths = if requested.is_empty() {
		discover_changeset_paths(root)?
			.into_iter()
			.map(|path| root.join(path))
			.collect::<Vec<_>>()
	} else {
		let mut resolved = Vec::new();

		for path in requested {
			resolved.push(resolve_changeset_path(root, path)?);
		}

		resolved.sort();
		resolved.dedup();
		resolved
	};

	let loaded_changesets = changeset_paths
		.iter()
		.map(|path| load_changeset_file(path, &configuration, &discovery.packages))
		.collect::<MonochangeResult<Vec<_>>>()?;

	let mut changesets = build_prepared_changesets(root, &loaded_changesets);

	if let Some(source) = configuration.source.as_ref() {
		hosted_sources::configured_hosted_source_adapter(source)
			.enrich_changeset_context(source, &mut changesets);
	}

	let requested_changesets = changeset_paths
		.iter()
		.map(|path| root_relative(root, path))
		.collect();

	Ok(ChangesetDiagnosticsReport {
		requested_changesets,
		changesets,
	})
}

#[must_use = "the changeset path result must be checked"]
pub(crate) fn resolve_changeset_path(root: &Path, requested: &str) -> MonochangeResult<PathBuf> {
	let requested_is_absolute = Path::new(requested).is_absolute();

	let normalized = if requested_is_absolute {
		requested.to_string()
	} else {
		normalize_changed_path(requested)
	};

	if normalized.is_empty() {
		return Err(MonochangeError::Config(
			"changeset path cannot be empty".to_string(),
		));
	}

	let candidate = if requested_is_absolute {
		Path::new(requested)
	} else {
		Path::new(&normalized)
	};

	let candidates = if candidate.is_absolute() {
		vec![candidate.to_path_buf()]
	} else {
		let mut candidates = vec![root.join(candidate)];

		if !normalized.starts_with(CHANGESET_DIR) {
			candidates.push(root.join(CHANGESET_DIR).join(candidate));
		}

		candidates
	};

	for candidate in candidates {
		let Some(relative_candidate) = relative_to_root(root, &candidate) else {
			continue;
		};

		if !is_changeset_markdown_path(&relative_candidate.to_string_lossy()) {
			continue;
		}

		if candidate.exists() {
			return Ok(candidate);
		}
	}

	Err(MonochangeError::Config(format!(
		"requested changeset `{requested}` does not exist"
	)))
}

pub(crate) fn render_changeset_diagnostics(report: &ChangesetDiagnosticsReport) -> String {
	if report.changesets.is_empty() {
		return "no matching changesets found".to_string();
	}

	let mut lines = Vec::new();

	for changeset in &report.changesets {
		let change_summary = changeset.summary.as_deref().unwrap_or("<missing summary>");

		lines.push(format!("changeset: {}", changeset.path.display()));
		lines.push(format!("  summary: {change_summary}"));

		if let Some(details) = &changeset.details {
			lines.push(format!("  details: {details}"));
		}

		if !changeset.targets.is_empty() {
			lines.push("  targets:".to_string());

			for target in &changeset.targets {
				let bump = target
					.bump
					.map_or_else(|| "auto".to_string(), |bump| bump.to_string());
				lines.push(format!(
					"  - {} {} (bump: {}, origin: {})",
					target.kind, target.id, bump, target.origin,
				));

				if !target.evidence_refs.is_empty() {
					lines.push(format!("    evidence: {}", target.evidence_refs.join(", ")));
				}
			}
		}

		if let Some(context) = &changeset.context {
			if let Some(introduced) = context
				.introduced
				.as_ref()
				.and_then(|revision| revision.commit.as_ref())
			{
				lines.push(format!("  introduced: {}", introduced.short_sha));
			}

			if let Some(last_updated) = context
				.last_updated
				.as_ref()
				.and_then(|revision| revision.commit.as_ref())
			{
				lines.push(format!("  last-updated: {}", last_updated.short_sha));
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
				if let Some(url) = &review_request.url {
					lines.push(format!("  review request: {} ({})", review_request.id, url));
				} else {
					lines.push(format!("  review request: {}", review_request.id));
				}
			}

			if !context.related_issues.is_empty() {
				let issues = context
					.related_issues
					.iter()
					.map(|issue| issue.id.as_str())
					.collect::<Vec<_>>()
					.join(", ");
				lines.push(format!("  related issues: {issues}"));
			}
		}

		lines.push(String::new());
	}

	lines.pop();
	lines.join("\n")
}

#[must_use = "the discovery result must be checked"]
pub(crate) fn discover_changeset_paths(root: &Path) -> MonochangeResult<Vec<PathBuf>> {
	let changeset_dir = root.join(CHANGESET_DIR);

	if !changeset_dir.exists() {
		return Err(MonochangeError::Config(format!(
			"no markdown changesets found under {CHANGESET_DIR}"
		)));
	}

	let mut changeset_paths = fs::read_dir(&changeset_dir)
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read {}: {error}",
				changeset_dir.display()
			))
		})?
		.filter_map(Result::ok)
		.map(|entry| entry.path())
		.filter(|path| path.extension().and_then(|value| value.to_str()) == Some("md"))
		.collect::<Vec<_>>();

	changeset_paths.sort();

	if changeset_paths.is_empty() {
		return Err(MonochangeError::Config(format!(
			"no markdown changesets found under {CHANGESET_DIR}"
		)));
	}

	Ok(changeset_paths)
}

pub(crate) fn build_prepared_changesets(
	root: &Path,
	loaded_changesets: &[monochange_config::LoadedChangesetFile],
) -> Vec<PreparedChangeset> {
	// Batch-load all changeset git context in a single pass instead of
	// spawning two git-log subprocesses per changeset (which was O(2N)
	// subprocess spawns and dominated release planning time).
	let git_contexts = batch_load_changeset_contexts(root, loaded_changesets);

	loaded_changesets
		.iter()
		.enumerate()
		.map(|(index, changeset)| {
			PreparedChangeset {
				path: root_relative(root, &changeset.path),
				summary: changeset.summary.clone(),
				details: changeset.details.clone(),
				targets: changeset
					.targets
					.iter()
					.map(|target| {
						PreparedChangesetTarget {
							id: target.id.clone(),
							kind: target.kind,
							bump: target.bump,
							origin: target.origin.clone(),
							evidence_refs: target.evidence_refs.clone(),
							change_type: target.change_type.clone(),
						}
					})
					.collect(),
				context: git_contexts.get(index).cloned(),
			}
		})
		.collect()
}

/// Load git context for all changesets in two batched git-log calls instead
/// of 2*N individual subprocess spawns.
#[tracing::instrument(skip_all, fields(count = loaded_changesets.len()))]
fn batch_load_changeset_contexts(
	root: &Path,
	loaded_changesets: &[monochange_config::LoadedChangesetFile],
) -> Vec<ChangesetContext> {
	if loaded_changesets.is_empty() {
		return Vec::new();
	}

	// Skip git operations entirely if there's no git repository.
	if !root.join(".git").exists() {
		return loaded_changesets
			.iter()
			.map(|_| {
				ChangesetContext {
					provider: HostingProviderKind::GenericGit,
					host: None,
					capabilities: HostingCapabilities::default(),
					introduced: None,
					last_updated: None,
					related_issues: Vec::new(),
				}
			})
			.collect();
	}

	// Build file list for git log.
	let relative_paths: Vec<_> = loaded_changesets
		.iter()
		.map(|cs| root_relative(root, &cs.path))
		.collect();

	let (introduced_map, last_updated_map) = batch_git_log(root, &relative_paths);

	relative_paths
		.iter()
		.map(|path| {
			let path_str = path.to_string_lossy();

			ChangesetContext {
				provider: HostingProviderKind::GenericGit,
				host: None,
				capabilities: HostingCapabilities::default(),
				introduced: introduced_map.get(path_str.as_ref()).cloned(),
				last_updated: last_updated_map.get(path_str.as_ref()).cloned(),
				related_issues: Vec::new(),
			}
		})
		.collect()
}

/// Run a single `git log` command covering the entire `.changeset/` directory
/// and return both the introduced and last-updated revision maps.
///
/// This replaces two independent history walks with one pass over the same git
/// log stream. The first time we see a file is its latest update; the last
/// added entry we see for the file is the commit that introduced it.
#[tracing::instrument(skip_all, fields(count = paths.len()))]
fn batch_git_log(
	root: &Path,
	paths: &[PathBuf],
) -> (
	std::collections::HashMap<String, ChangesetRevision>,
	std::collections::HashMap<String, ChangesetRevision>,
) {
	use std::collections::HashMap;

	if paths.is_empty() {
		return (HashMap::new(), HashMap::new());
	}

	let mut command = ProcessCommand::new("git");
	command
		.current_dir(root)
		.arg("log")
		.arg("--format=%H%x1f%an%x1f%ae%x1f%aI%x1f%cI")
		.arg("--name-status");
	command.arg("--").arg(".changeset/");

	let output = match command.output() {
		Ok(output) if output.status.success() => output,
		_ => return (HashMap::new(), HashMap::new()),
	};
	parse_batch_git_log_bytes(&output.stdout, paths)
}

fn parse_batch_git_log_bytes(
	stdout: &[u8],
	paths: &[PathBuf],
) -> (
	std::collections::HashMap<String, ChangesetRevision>,
	std::collections::HashMap<String, ChangesetRevision>,
) {
	use std::collections::HashMap;

	let Ok(stdout) = std::str::from_utf8(stdout) else {
		return (HashMap::new(), HashMap::new());
	};
	parse_batch_git_log_output(stdout, paths)
}

fn parse_batch_git_log_output(
	stdout: &str,
	paths: &[PathBuf],
) -> (
	std::collections::HashMap<String, ChangesetRevision>,
	std::collections::HashMap<String, ChangesetRevision>,
) {
	use std::collections::HashMap;

	let wanted_paths: std::collections::HashSet<String> = paths
		.iter()
		.map(|p| p.to_string_lossy().into_owned())
		.collect();

	// Parse the git log output. Format is blocks separated by blank lines:
	// <sha>\x1f<name>\x1f<email>\x1f<author_date>\x1f<commit_date>
	// <status>\t<filename>
	//
	// (blank line between commits)
	let mut introduced = HashMap::new();
	let mut last_updated = HashMap::new();

	let mut current_fields: Option<Vec<&str>> = None;
	for line in stdout.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty() {
			// Blank lines separate the header from filenames AND separate
			// commits from each other. Don't reset current_fields here —
			// a new header line (containing \x1f) will replace it.
			continue;
		}
		if trimmed.contains('\u{1f}') {
			// This is a commit header line — start a new commit block.
			current_fields = Some(trimmed.split('\u{1f}').collect());
			continue;
		}
		// This is a filename line associated with the current commit.
		let Some(ref fields) = current_fields else {
			continue;
		};
		if fields.len() != 5 {
			continue;
		}
		let mut parts = trimmed.splitn(2, '\t');
		let Some(status) = parts.next() else {
			continue;
		};
		let Some(file_path) = parts.next() else {
			continue;
		};
		if !wanted_paths.contains(file_path) {
			continue;
		}
		let [sha, author_name, author_email, authored_at, committed_at] = fields.as_slice() else {
			continue;
		};
		let revision = ChangesetRevision {
			actor: Some(HostedActorRef {
				provider: HostingProviderKind::GenericGit,
				host: None,
				id: None,
				login: None,
				display_name: Some((*author_name).to_string()),
				url: None,
				source: HostedActorSourceKind::CommitAuthor,
			}),
			commit: Some(HostedCommitRef {
				provider: HostingProviderKind::GenericGit,
				host: None,
				sha: (*sha).to_string(),
				short_sha: short_commit_sha(sha),
				url: None,
				author_name: Some((*author_name).to_string()),
				author_email: Some((*author_email).to_string()),
				authored_at: Some((*authored_at).to_string()),
				committed_at: Some((*committed_at).to_string()),
			}),
			review_request: None,
		};
		last_updated
			.entry(file_path.to_string())
			.or_insert_with(|| revision.clone());
		if status == "A" {
			introduced.insert(file_path.to_string(), revision);
		}
	}

	(introduced, last_updated)
}

pub(crate) fn short_commit_sha(sha: &str) -> String {
	sha.chars().take(7).collect()
}

#[tracing::instrument(skip_all)]
pub(crate) fn build_release_plan_from_signals(
	configuration: &monochange_core::WorkspaceConfiguration,
	discovery: &DiscoveryReport,
	change_signals: &[ChangeSignal],
) -> MonochangeResult<ReleasePlan> {
	#[cfg(feature = "cargo")]
	let rust_provider = RustSemverProvider;
	#[cfg(feature = "cargo")]
	let providers: [&dyn CompatibilityProvider; 1] = [&rust_provider];
	#[cfg(feature = "cargo")]
	let compatibility_evidence = collect_assessments(&providers, &discovery.packages, change_signals);
	#[cfg(not(feature = "cargo"))]
	let compatibility_evidence = Vec::new();

	build_release_plan(
		&discovery.workspace_root,
		&discovery.packages,
		&discovery.dependencies,
		&discovery.version_groups,
		change_signals,
		&compatibility_evidence,
		configuration.defaults.parent_bump,
		configuration.defaults.strict_version_conflicts,
	)
}

pub(crate) fn canonical_change_packages(
	root: &Path,
	package_refs: &[String],
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> MonochangeResult<Vec<String>> {
	let mut canonical_packages = Vec::new();
	for package_ref in package_refs {
		let canonical_key = if configuration
			.groups
			.iter()
			.any(|group| group.id == *package_ref)
			|| configuration
				.packages
				.iter()
				.any(|package| package.id == *package_ref)
		{
			package_ref.clone()
		} else {
			let package_id = resolve_package_reference(package_ref, root, packages)?;
			let package = packages
				.iter()
				.find(|package| package.id == package_id)
				.ok_or_else(|| {
					MonochangeError::Config(format!("failed to resolve package `{package_ref}`"))
				})?;
			package
				.metadata
				.get("config_id")
				.cloned()
				.unwrap_or_else(|| package.name.clone())
		};
		if !canonical_packages.contains(&canonical_key) {
			canonical_packages.push(canonical_key);
		}
	}
	Ok(canonical_packages)
}

pub(crate) fn released_package_names(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> Vec<String> {
	let package_by_id = packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<BTreeMap<_, _>>();
	let mut released_packages = plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
		.filter_map(|decision| {
			package_by_id
				.get(decision.package_id.as_str())
				.map(|package| package.name.clone())
		})
		.collect::<Vec<_>>();
	released_packages.sort();
	released_packages.dedup();
	released_packages
}

pub(crate) type PackageChangelogTargets = BTreeMap<String, ChangelogTarget>;
pub(crate) type GroupChangelogTargets = BTreeMap<String, ChangelogTarget>;

pub(crate) fn resolve_changelog_targets(
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> MonochangeResult<(PackageChangelogTargets, GroupChangelogTargets)> {
	let mut package_targets = BTreeMap::new();
	let mut group_targets = BTreeMap::new();

	for package_definition in &configuration.packages {
		let Some(changelog_path) = &package_definition.changelog else {
			continue;
		};
		let package_id =
			resolve_package_reference(&package_definition.id, &configuration.root_path, packages)?;
		package_targets.insert(
			package_id,
			ChangelogTarget {
				path: resolve_config_path(&configuration.root_path, &changelog_path.path),
				format: changelog_path.format,
			},
		);
	}
	for group_definition in &configuration.groups {
		let Some(changelog_path) = &group_definition.changelog else {
			continue;
		};
		group_targets.insert(
			group_definition.id.clone(),
			ChangelogTarget {
				path: resolve_config_path(&configuration.root_path, &changelog_path.path),
				format: changelog_path.format,
			},
		);
	}

	Ok((package_targets, group_targets))
}

pub(crate) fn resolve_config_path(root: &Path, path: &Path) -> PathBuf {
	if path.is_absolute() {
		path.to_path_buf()
	} else {
		root.join(path)
	}
}

pub(crate) fn default_change_path(root: &Path, package_refs: &[String]) -> PathBuf {
	let timestamp = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |duration| duration.as_secs());
	let slug_source = package_refs.first().map_or("change", String::as_str);
	let slug = slug_source
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
	let slug = if slug.is_empty() {
		"change".to_string()
	} else {
		slug
	};
	root.join(CHANGESET_DIR)
		.join(format!("{timestamp}-{slug}.md"))
}

pub(crate) fn render_changeset_markdown(
	configuration: &monochange_core::WorkspaceConfiguration,
	package_refs: &[String],
	bump: BumpSeverity,
	version: Option<&str>,
	reason: &str,
	change_type: Option<&str>,
	details: Option<&str>,
) -> MonochangeResult<String> {
	let mut lines = vec!["---".to_string()];
	for package in package_refs {
		lines.extend(render_change_target_markdown(
			configuration,
			package,
			bump,
			version,
			change_type,
		)?);
	}
	lines.push("---".to_string());
	lines.push(String::new());
	lines.push(format!("# {reason}"));
	if let Some(details) = details.filter(|value| !value.trim().is_empty()) {
		lines.push(String::new());
		lines.push(details.trim().to_string());
	}
	lines.push(String::new());
	Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
	use std::path::Path;
	use std::path::PathBuf;

	use super::batch_git_log;
	use super::parse_batch_git_log_bytes;
	use super::parse_batch_git_log_output;

	#[test]
	fn batch_git_log_returns_empty_maps_for_empty_paths() {
		let (introduced, last_updated) = batch_git_log(Path::new("."), &[]);
		assert!(introduced.is_empty());
		assert!(last_updated.is_empty());
	}

	#[test]
	fn batch_git_log_returns_empty_maps_when_git_log_fails() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let (introduced, last_updated) =
			batch_git_log(tempdir.path(), &[PathBuf::from(".changeset/feature.md")]);
		assert!(introduced.is_empty());
		assert!(last_updated.is_empty());
	}

	#[test]
	fn parse_batch_git_log_bytes_returns_empty_maps_for_invalid_utf8_output() {
		let (introduced, last_updated) =
			parse_batch_git_log_bytes(b"\xff", &[PathBuf::from(".changeset/feature.md")]);
		assert!(introduced.is_empty());
		assert!(last_updated.is_empty());
	}

	#[test]
	fn parse_batch_git_log_output_ignores_malformed_name_status_lines() {
		let (introduced, last_updated) = parse_batch_git_log_output(
			"abc123\x1fIfiok\x1fifiok@example.com\x1f2026-04-06T00:00:00Z\x1f2026-04-06T00:00:00Z\nM\n",
			&[PathBuf::from(".changeset/feature.md")],
		);
		assert!(introduced.is_empty());
		assert!(last_updated.is_empty());
	}
}
