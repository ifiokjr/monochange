use std::io::IsTerminal;

#[cfg(test)]
use std::cell::Cell;

use similar::TextDiff;

use super::*;

#[cfg(test)]
thread_local! {
	static FORCE_BUILD_FILE_DIFF_PREVIEWS_ERROR: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn build_release_targets(
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
	changeset_paths: &[PathBuf],
) -> Vec<ReleaseTarget> {
	let changes_count = changeset_paths.len();
	let source = configuration.source.as_ref();
	let defaults_release_title = configuration.defaults.release_title.as_deref();
	let defaults_changelog_title = configuration.defaults.changelog_version_title.as_deref();

	let mut release_targets = configuration
		.groups
		.iter()
		.filter_map(|group| {
			plan.groups
				.iter()
				.find(|pg| pg.group_id == group.id && pg.recommended_bump.is_release())
				.and_then(|pg| {
					pg.planned_version.as_ref().map(|version| {
						let vs = version.to_string();
						let tag = render_tag_name(&group.id, &vs, group.version_format);
						let prev = find_previous_tag(&configuration.root_path, &tag);
						let ctx = TitleRenderContext::new(
							&group.id,
							&vs,
							changes_count,
							source,
							&tag,
							prev.as_deref(),
						);
						let rt = effective_title_template(
							group.release_title.as_deref(),
							defaults_release_title,
							default_release_title_for_format(group.version_format),
						);
						let ct = effective_title_template(
							group.changelog_version_title.as_deref(),
							defaults_changelog_title,
							default_changelog_version_title_for_format(group.version_format),
						);
						ReleaseTarget {
							id: group.id.clone(),
							kind: ReleaseOwnerKind::Group,
							version: vs,
							tag: group.tag,
							release: group.release,
							version_format: group.version_format,
							tag_name: tag,
							members: group.packages.clone(),
							rendered_title: ctx.render(rt),
							rendered_changelog_title: ctx.render(ct),
						}
					})
				})
		})
		.collect::<Vec<_>>();
	for decision in plan
		.decisions
		.iter()
		.filter(|d| d.recommended_bump.is_release() && d.group_id.is_none())
	{
		let Some(package) = packages.iter().find(|p| p.id == decision.package_id) else {
			continue;
		};
		let Some(version) = decision.planned_version.as_ref() else {
			continue;
		};
		let config_id = package
			.metadata
			.get("config_id")
			.cloned()
			.unwrap_or_else(|| package.name.clone());
		let Some(identity) = configuration.effective_release_identity(&config_id) else {
			continue;
		};
		let vs = version.to_string();
		let tag = render_tag_name(&identity.owner_id, &vs, identity.version_format);
		let prev = find_previous_tag(&configuration.root_path, &tag);
		let pkg_def = configuration.package_by_id(&config_id);
		let ctx = TitleRenderContext::new(
			&identity.owner_id,
			&vs,
			changes_count,
			source,
			&tag,
			prev.as_deref(),
		);
		let rt = effective_title_template(
			pkg_def.and_then(|p| p.release_title.as_deref()),
			defaults_release_title,
			default_release_title_for_format(identity.version_format),
		);
		let ct = effective_title_template(
			pkg_def.and_then(|p| p.changelog_version_title.as_deref()),
			defaults_changelog_title,
			default_changelog_version_title_for_format(identity.version_format),
		);
		release_targets.push(ReleaseTarget {
			id: identity.owner_id.clone(),
			kind: identity.owner_kind,
			version: vs,
			tag: identity.tag,
			release: identity.release,
			version_format: identity.version_format,
			tag_name: tag,
			members: identity.members,
			rendered_title: ctx.render(rt),
			rendered_changelog_title: ctx.render(ct),
		});
	}
	release_targets.sort_by(|left, right| left.id.cmp(&right.id));
	release_targets
}

pub(crate) fn render_tag_name(id: &str, version: &str, version_format: VersionFormat) -> String {
	match version_format {
		VersionFormat::Namespaced => format!("{id}/v{version}"),
		VersionFormat::Primary => format!("v{version}"),
	}
}

/// Dispatch tag URL generation to the appropriate provider crate.
pub(crate) fn tag_url_for_provider(source: &SourceConfiguration, tag_name: &str) -> String {
	match source.provider {
		SourceProvider::GitHub => github_provider::tag_url(source, tag_name),
		SourceProvider::GitLab => gitlab_provider::tag_url(source, tag_name),
		SourceProvider::Gitea => gitea_provider::tag_url(source, tag_name),
	}
}

/// Dispatch compare URL generation to the appropriate provider crate.
pub(crate) fn compare_url_for_provider(
	source: &SourceConfiguration,
	previous_tag: &str,
	current_tag: &str,
) -> String {
	match source.provider {
		SourceProvider::GitHub => github_provider::compare_url(source, previous_tag, current_tag),
		SourceProvider::GitLab => gitlab_provider::compare_url(source, previous_tag, current_tag),
		SourceProvider::Gitea => gitea_provider::compare_url(source, previous_tag, current_tag),
	}
}

pub(crate) fn find_previous_tag(root: &Path, current_tag: &str) -> Option<String> {
	let output =
		monochange_core::git::git_command_output(root, &["tag", "--list", "--sort=-v:refname"])
			.ok()?;
	if !output.status.success() {
		return None;
	}
	let tags_text = String::from_utf8_lossy(&output.stdout);
	let all_tags: Vec<&str> = tags_text.lines().map(str::trim).collect();
	let (prefix, current_version) = parse_tag_prefix_and_version(current_tag)?;
	all_tags
		.into_iter()
		.filter(|tag| *tag != current_tag)
		.filter_map(|tag| {
			let (p, v) = parse_tag_prefix_and_version(tag)?;
			(p == prefix && v < current_version).then(|| (tag.to_string(), v))
		})
		.max_by(|a, b| a.1.cmp(&b.1))
		.map(|(tag, _)| tag)
}

pub(crate) fn parse_tag_prefix_and_version(tag: &str) -> Option<(String, semver::Version)> {
	let v_pos = tag.rfind('v')?;
	let prefix = &tag[..=v_pos];
	let version_str = &tag[v_pos + 1..];
	let version = semver::Version::parse(version_str).ok()?;
	Some((prefix.to_string(), version))
}

struct TitleRenderContext {
	id: String,
	version: String,
	previous_version: String,
	date: String,
	time: String,
	datetime: String,
	changes_count: usize,
	tag_url: String,
	compare_url: String,
}

impl TitleRenderContext {
	fn new(
		id: &str,
		version: &str,
		changes_count: usize,
		source: Option<&SourceConfiguration>,
		tag_name: &str,
		previous_tag_name: Option<&str>,
	) -> Self {
		let now = resolve_release_datetime();
		let date = now.format("%Y-%m-%d").to_string();
		let time = now.format("%H:%M:%S").to_string();
		let datetime = now.format("%Y-%m-%dT%H:%M:%S").to_string();
		let tag_url = source
			.map(|s| tag_url_for_provider(s, tag_name))
			.unwrap_or_default();
		let compare_url = match (source, previous_tag_name) {
			(Some(s), Some(prev)) => compare_url_for_provider(s, prev, tag_name),
			_ => tag_url.clone(),
		};
		// Extract the bare semver string from the previous tag (e.g. "pkg/v1.1.0" → "1.1.0").
		let previous_version = previous_tag_name
			.and_then(|t| parse_tag_prefix_and_version(t).map(|(_, v)| v.to_string()))
			.unwrap_or_default();
		Self {
			id: id.to_string(),
			version: version.to_string(),
			previous_version,
			date,
			time,
			datetime,
			changes_count,
			tag_url,
			compare_url,
		}
	}

	fn render(&self, template: &str) -> String {
		let context = minijinja::context! {
			id => &self.id,
			version => &self.version,
			previous_version => &self.previous_version,
			date => &self.date,
			time => &self.time,
			datetime => &self.datetime,
			changes_count => self.changes_count,
			tag_url => &self.tag_url,
			compare_url => &self.compare_url,
		};
		let jinja_value = minijinja::Value::from_serialize(&context);
		render_jinja_template(template, &jinja_value).unwrap_or_else(|_| self.version.clone())
	}
}

pub(crate) fn resolve_release_datetime() -> chrono::NaiveDateTime {
	use chrono::NaiveDate;
	use chrono::NaiveDateTime;

	if let Ok(env_date) = std::env::var("MONOCHANGE_RELEASE_DATE") {
		if let Ok(ndt) = NaiveDateTime::parse_from_str(&env_date, "%Y-%m-%dT%H:%M:%S") {
			return ndt;
		}
		if let Ok(nd) = NaiveDate::parse_from_str(&env_date, "%Y-%m-%d") {
			return nd.and_hms_opt(0, 0, 0).unwrap_or_default();
		}
	}
	chrono::Local::now().naive_local()
}

pub(crate) fn effective_title_template<'a>(
	specific: Option<&'a str>,
	defaults: Option<&'a str>,
	builtin: &'a str,
) -> &'a str {
	specific.or(defaults).unwrap_or(builtin)
}

pub(crate) fn default_release_title_for_format(version_format: VersionFormat) -> &'static str {
	match version_format {
		VersionFormat::Primary => DEFAULT_RELEASE_TITLE_PRIMARY,
		VersionFormat::Namespaced => DEFAULT_RELEASE_TITLE_NAMESPACED,
	}
}

pub(crate) fn default_changelog_version_title_for_format(
	version_format: VersionFormat,
) -> &'static str {
	match version_format {
		VersionFormat::Primary => DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY,
		VersionFormat::Namespaced => DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED,
	}
}

pub(crate) fn build_cargo_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	let released_versions = plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
		.filter_map(|decision| {
			decision
				.planned_version
				.as_ref()
				.map(|version| (decision.package_id.clone(), version.to_string()))
		})
		.collect::<BTreeMap<_, _>>();
	let released_versions_by_name = packages
		.iter()
		.filter_map(|package| {
			released_versions
				.get(&package.id)
				.map(|version| (package.name.clone(), version.clone()))
		})
		.collect::<BTreeMap<_, _>>();
	if released_versions_by_name.is_empty() {
		return Ok(Vec::new());
	}

	let mut updated_documents = BTreeMap::<PathBuf, String>::new();
	for package in packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Cargo)
	{
		let should_update_manifest = released_versions.contains_key(&package.id)
			|| package
				.declared_dependencies
				.iter()
				.any(|dependency| released_versions_by_name.contains_key(&dependency.name));
		if !should_update_manifest {
			continue;
		}

		let contents = fs::read_to_string(&package.manifest_path).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read {}: {error}",
				package.manifest_path.display()
			))
		})?;
		let updated = monochange_cargo::update_versioned_file_text(
			&contents,
			monochange_cargo::CargoVersionedFileKind::Manifest,
			&["dependencies", "dev-dependencies", "build-dependencies"],
			released_versions.get(&package.id).map(String::as_str),
			None,
			&released_versions_by_name,
			&BTreeMap::new(),
		)
		.map_err(|error| {
			MonochangeError::Config(format!(
				"failed to parse {}: {error}",
				package.manifest_path.display()
			))
		})?;
		updated_documents.insert(package.manifest_path.clone(), updated);
	}

	for workspace_root in packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Cargo)
		.filter(|package| released_versions.contains_key(&package.id))
		.map(|package| package.workspace_root.clone())
		.collect::<BTreeSet<_>>()
	{
		let workspace_version = packages
			.iter()
			.filter(|package| {
				package.ecosystem == Ecosystem::Cargo
					&& package.workspace_root == workspace_root
					&& released_versions.contains_key(&package.id)
			})
			.filter_map(|package| released_versions.get(&package.id))
			.cloned()
			.collect::<BTreeSet<_>>();
		let Some(shared_workspace_version) = workspace_version.first().cloned() else {
			continue;
		};
		if workspace_version.len() != 1 {
			continue;
		}

		let workspace_manifest = workspace_root.join("Cargo.toml");
		if !workspace_manifest.exists() {
			continue;
		}
		let contents = if let Some(document) = updated_documents.remove(&workspace_manifest) {
			document
		} else {
			fs::read_to_string(&workspace_manifest).map_err(|error| {
				MonochangeError::Io(format!(
					"failed to read {}: {error}",
					workspace_manifest.display()
				))
			})?
		};
		let updated = monochange_cargo::update_versioned_file_text(
			&contents,
			monochange_cargo::CargoVersionedFileKind::Manifest,
			&["dependencies", "dev-dependencies", "build-dependencies"],
			None,
			Some(shared_workspace_version.as_str()),
			&released_versions_by_name,
			&BTreeMap::new(),
		)
		.map_err(|error| {
			MonochangeError::Config(format!(
				"failed to parse {}: {error}",
				workspace_manifest.display()
			))
		})?;
		updated_documents.insert(workspace_manifest, updated);
	}

	Ok(updated_documents
		.into_iter()
		.map(|(path, document)| FileUpdate {
			path,
			content: document.into_bytes(),
		})
		.collect())
}

pub(crate) fn build_npm_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	let released_versions = released_versions_by_record_id(plan);
	let mut updates = Vec::new();
	for package in packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Npm)
	{
		let Some(version) = released_versions.get(&package.id) else {
			continue;
		};
		let contents = fs::read_to_string(&package.manifest_path).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read {}: {error}",
				package.manifest_path.display()
			))
		})?;
		let rendered = monochange_core::update_json_manifest_text(
			&contents,
			Some(version),
			&[],
			&BTreeMap::new(),
		)
		.map_err(|error| {
			MonochangeError::Config(format!(
				"failed to parse {}: {error}",
				package.manifest_path.display()
			))
		})?;
		updates.push(FileUpdate {
			path: package.manifest_path.clone(),
			content: rendered.into_bytes(),
		});
	}
	Ok(updates)
}

pub(crate) fn build_deno_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	let released_versions = released_versions_by_record_id(plan);
	let mut updates = Vec::new();
	for package in packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Deno)
	{
		let Some(version) = released_versions.get(&package.id) else {
			continue;
		};
		let contents = fs::read_to_string(&package.manifest_path).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read {}: {error}",
				package.manifest_path.display()
			))
		})?;
		let rendered = monochange_core::update_json_manifest_text(
			&contents,
			Some(version),
			&[],
			&BTreeMap::new(),
		)
		.map_err(|error| {
			MonochangeError::Config(format!(
				"failed to parse {}: {error}",
				package.manifest_path.display()
			))
		})?;
		updates.push(FileUpdate {
			path: package.manifest_path.clone(),
			content: rendered.into_bytes(),
		});
	}
	Ok(updates)
}

pub(crate) fn build_dart_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	let released_versions = released_versions_by_record_id(plan);
	let mut updates = Vec::new();
	for package in packages.iter().filter(|package| {
		package.ecosystem == Ecosystem::Dart || package.ecosystem == Ecosystem::Flutter
	}) {
		let Some(version) = released_versions.get(&package.id) else {
			continue;
		};
		let contents = fs::read_to_string(&package.manifest_path).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read {}: {error}",
				package.manifest_path.display()
			))
		})?;
		let rendered =
			monochange_dart::update_manifest_text(&contents, Some(version), &[], &BTreeMap::new())
				.map_err(|error| {
					MonochangeError::Config(format!(
						"failed to parse {}: {error}",
						package.manifest_path.display()
					))
				})?;
		updates.push(FileUpdate {
			path: package.manifest_path.clone(),
			content: rendered.into_bytes(),
		});
	}
	Ok(updates)
}

pub(crate) fn apply_file_updates(updates: &[FileUpdate]) -> MonochangeResult<()> {
	for update in updates {
		if let Some(parent) = update.path.parent() {
			fs::create_dir_all(parent).map_err(|error| {
				MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
			})?;
		}
		atomic_write(&update.path, &update.content)?;
	}
	Ok(())
}

/// Write file content atomically: write to a temporary file in the same
/// directory, then rename into place. On Unix the rename is atomic within the
/// same filesystem, so the file is either fully written or untouched.
fn atomic_write(path: &Path, content: &[u8]) -> MonochangeResult<()> {
	let parent = path.parent().unwrap_or(path);
	let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create temp file in {}: {error}",
			parent.display()
		))
	})?;
	std::io::Write::write_all(&mut temp, content).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write temp file for {}: {error}",
			path.display()
		))
	})?;
	temp.persist(path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to rename temp file to {}: {error}",
			path.display()
		))
	})?;
	Ok(())
}

#[rustfmt::skip]
pub(crate) fn build_file_diff_previews(root: &Path, updates: &[FileUpdate]) -> MonochangeResult<Vec<PreparedFileDiff>> { let colorize_diffs = diff_output_colors_enabled();
	#[cfg(test)]
	if FORCE_BUILD_FILE_DIFF_PREVIEWS_ERROR.with(Cell::get) {
		return Err(MonochangeError::Io(
			"forced build_file_diff_previews test error".to_string(),
		));
	}
	let mut previews = updates
		.iter()
		.filter_map(|update| {
			let path = root_relative(root, &update.path);
			let before = match fs::read(&update.path) {
				Ok(content) => content,
				Err(error)
					if matches!(
						error.kind(),
						std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
					) =>
				{
					Vec::new()
				}
				Err(error) => {
					return Some(Err(MonochangeError::Io(format!(
						"failed to read {}: {error}",
						update.path.display()
					))));
				}
			};
			(before != update.content).then(|| {
				let diff = render_unified_file_diff(&path, &before, &update.content);
				Ok(PreparedFileDiff {
					path: path.clone(),
					display_diff: render_display_file_diff(&diff, colorize_diffs),
					diff,
				})
			})
		})
		.collect::<MonochangeResult<Vec<_>>>()?;
	previews.sort_by(|left, right| left.path.cmp(&right.path));
	Ok(previews)
}

fn render_unified_file_diff(path: &Path, before: &[u8], after: &[u8]) -> String {
	let before_text = String::from_utf8_lossy(before);
	let after_text = String::from_utf8_lossy(after);
	let context_radius = before_text.lines().count().max(after_text.lines().count());
	let diff = TextDiff::from_lines(before_text.as_ref(), after_text.as_ref());
	let mut unified = diff.unified_diff();
	unified.context_radius(context_radius).header(
		&format!("a/{}", path.display()),
		&format!("b/{}", path.display()),
	);
	unified.to_string().trim_end_matches('\n').to_string()
}

fn render_display_file_diff(diff: &str, colorize: bool) -> String {
	if !colorize {
		return diff.to_string();
	}
	colorize_diff_output(diff)
}

fn diff_output_colors_enabled() -> bool {
	diff_output_supports_color(std::io::stdout().is_terminal())
}

#[cfg(test)]
pub(crate) fn set_force_build_file_diff_previews_error(enabled: bool) {
	FORCE_BUILD_FILE_DIFF_PREVIEWS_ERROR.with(|value| value.set(enabled));
}

pub(crate) fn diff_output_supports_color(stdout_is_terminal: bool) -> bool {
	if std::env::var_os("NO_COLOR").is_some() {
		return false;
	}
	if std::env::var("CLICOLOR_FORCE")
		.ok()
		.is_some_and(|value| value != "0")
	{
		return true;
	}
	if std::env::var("CLICOLOR")
		.ok()
		.is_some_and(|value| value == "0")
	{
		return false;
	}
	stdout_is_terminal
}

fn colorize_diff_output(diff: &str) -> String {
	diff.lines()
		.map(colorize_diff_line)
		.collect::<Vec<_>>()
		.join("\n")
}

pub(crate) fn colorize_diff_line(line: &str) -> String {
	if line.starts_with("--- ") || line.starts_with("+++ ") {
		apply_ansi_style(line, "1;36")
	} else if line.starts_with("@@ ") {
		apply_ansi_style(line, "36")
	} else if line.starts_with('+') && !line.starts_with("+++") {
		apply_ansi_style(line, "32")
	} else if line.starts_with('-') && !line.starts_with("---") {
		apply_ansi_style(line, "31")
	} else if line == r"\ No newline at end of file" {
		apply_ansi_style(line, "33")
	} else {
		line.to_string()
	}
}

fn apply_ansi_style(line: &str, style: &str) -> String {
	format!("\u{1b}[{style}m{line}\u{1b}[0m")
}

pub(crate) fn shared_release_version(plan: &ReleasePlan) -> Option<String> {
	let versions = plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
		.filter_map(|decision| decision.planned_version.as_ref().map(ToString::to_string))
		.collect::<BTreeSet<_>>();
	if versions.len() == 1 {
		versions.first().cloned()
	} else {
		None
	}
}

pub(crate) fn shared_group_version(plan: &ReleasePlan) -> Option<String> {
	let versions = plan
		.groups
		.iter()
		.filter(|group| group.recommended_bump.is_release())
		.filter_map(|group| group.planned_version.as_ref().map(ToString::to_string))
		.collect::<BTreeSet<_>>();
	if versions.len() == 1 {
		versions.first().cloned()
	} else {
		None
	}
}

pub(crate) fn root_relative(root: &Path, path: &Path) -> PathBuf {
	let relative = relative_to_root(root, path).unwrap_or_else(|| path.to_path_buf());
	if relative.as_os_str().is_empty() {
		PathBuf::from(".")
	} else {
		relative
	}
}

pub(crate) fn render_discovery_report(
	report: &DiscoveryReport,
	format: OutputFormat,
) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json => serde_json::to_string_pretty(&json_discovery_report(report))
			.map_err(|error| MonochangeError::Discovery(error.to_string())),
		OutputFormat::Text => Ok(text_discovery_report(report)),
	}
}

pub(crate) fn build_release_manifest(
	cli_command: &CliCommandDefinition,
	prepared_release: &PreparedRelease,
	_command_logs: &[String],
) -> ReleaseManifest {
	ReleaseManifest {
		command: cli_command.name.clone(),
		dry_run: prepared_release.dry_run,
		version: prepared_release.version.clone(),
		group_version: prepared_release.group_version.clone(),
		release_targets: prepared_release
			.release_targets
			.iter()
			.map(|target| ReleaseManifestTarget {
				id: target.id.clone(),
				kind: target.kind,
				version: target.version.clone(),
				tag: target.tag,
				release: target.release,
				version_format: target.version_format,
				tag_name: target.tag_name.clone(),
				members: target.members.clone(),
				rendered_title: target.rendered_title.clone(),
				rendered_changelog_title: target.rendered_changelog_title.clone(),
			})
			.collect(),
		released_packages: prepared_release.released_packages.clone(),
		changed_files: prepared_release.changed_files.clone(),
		changelogs: prepared_release
			.changelogs
			.iter()
			.map(|changelog| ReleaseManifestChangelog {
				owner_id: changelog.owner_id.clone(),
				owner_kind: changelog.owner_kind,
				path: changelog.path.clone(),
				format: changelog.format,
				notes: changelog.notes.clone(),
				rendered: changelog.rendered.clone(),
			})
			.collect(),
		changesets: prepared_release.changesets.clone(),
		deleted_changesets: prepared_release.deleted_changesets.clone(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: prepared_release
				.plan
				.decisions
				.iter()
				.map(|decision| ReleaseManifestPlanDecision {
					package: decision.package_id.clone(),
					bump: decision.recommended_bump,
					trigger: decision.trigger_type.clone(),
					planned_version: decision.planned_version.as_ref().map(ToString::to_string),
					reasons: decision.reasons.clone(),
					upstream_sources: decision.upstream_sources.clone(),
				})
				.collect(),
			groups: prepared_release
				.plan
				.groups
				.iter()
				.map(|group| ReleaseManifestPlanGroup {
					id: group.group_id.clone(),
					planned_version: group.planned_version.as_ref().map(ToString::to_string),
					members: group.members.clone(),
					bump: group.recommended_bump,
				})
				.collect(),
			warnings: prepared_release.plan.warnings.clone(),
			unresolved_items: prepared_release.plan.unresolved_items.clone(),
			compatibility_evidence: prepared_release
				.plan
				.compatibility_evidence
				.iter()
				.map(|assessment| ReleaseManifestCompatibilityEvidence {
					package: assessment.package_id.clone(),
					provider: assessment.provider_id.clone(),
					severity: assessment.severity,
					summary: assessment.summary.clone(),
					confidence: assessment.confidence.clone(),
					evidence_location: assessment.evidence_location.clone(),
				})
				.collect(),
		},
	}
}

pub(crate) fn build_release_record(
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> ReleaseRecord {
	ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: resolve_release_datetime()
			.and_utc()
			.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
		command: manifest.command.clone(),
		version: manifest.version.clone(),
		group_version: manifest.group_version.clone(),
		release_targets: manifest
			.release_targets
			.iter()
			.map(|target| ReleaseRecordTarget {
				id: target.id.clone(),
				kind: target.kind,
				version: target.version.clone(),
				version_format: target.version_format,
				tag: target.tag,
				release: target.release,
				tag_name: target.tag_name.clone(),
				members: target.members.clone(),
			})
			.collect(),
		released_packages: manifest.released_packages.clone(),
		changed_files: manifest.changed_files.clone(),
		updated_changelogs: manifest
			.changelogs
			.iter()
			.map(|changelog| changelog.path.clone())
			.collect(),
		deleted_changesets: manifest.deleted_changesets.clone(),
		provider: source.map(|source| ReleaseRecordProvider {
			kind: source.provider,
			owner: source.owner.clone(),
			repo: source.repo.clone(),
			host: source.host.clone(),
		}),
	}
}

pub(crate) fn build_release_commit_message(
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> CommitMessage {
	CommitMessage {
		subject: source.map_or_else(
			|| monochange_core::ChangeRequestSettings::default().title,
			|source| source.pull_requests.title.clone(),
		),
		body: Some(render_release_commit_body(source, manifest)),
	}
}

pub(crate) fn render_release_commit_body(
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> String {
	let mut lines = vec!["Prepare release.".to_string()];
	if !manifest.release_targets.is_empty() {
		lines.push(String::new());
		lines.push(format!(
			"- release targets: {}",
			manifest
				.release_targets
				.iter()
				.map(|target| format!("{} ({})", target.id, target.version))
				.collect::<Vec<_>>()
				.join(", ")
		));
	}
	if !manifest.released_packages.is_empty() {
		lines.push(format!(
			"- released packages: {}",
			manifest.released_packages.join(", ")
		));
	}
	if !manifest.changelogs.is_empty() {
		lines.push(format!(
			"- updated changelogs: {}",
			manifest
				.changelogs
				.iter()
				.map(|changelog| changelog.path.display().to_string())
				.collect::<Vec<_>>()
				.join(", ")
		));
	}
	if !manifest.deleted_changesets.is_empty() {
		lines.push(format!(
			"- deleted changesets: {}",
			manifest
				.deleted_changesets
				.iter()
				.map(|path| path.display().to_string())
				.collect::<Vec<_>>()
				.join(", ")
		));
	}
	let release_record = build_release_record(source, manifest);
	let release_record_block = render_release_record_block(&release_record)
		.unwrap_or_else(|error| panic!("release record generation bug: {error}"));
	format!("{}\n\n{}", lines.join("\n"), release_record_block)
}

pub(crate) fn render_release_manifest_json(manifest: &ReleaseManifest) -> MonochangeResult<String> {
	serde_json::to_string_pretty(manifest)
		.map_err(|error| MonochangeError::Discovery(error.to_string()))
}

pub(crate) fn build_source_release_requests(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> Vec<SourceReleaseRequest> {
	match source.provider {
		SourceProvider::GitHub => github_provider::build_release_requests(source, manifest),
		SourceProvider::GitLab => gitlab_provider::build_release_requests(source, manifest),
		SourceProvider::Gitea => gitea_provider::build_release_requests(source, manifest),
	}
}

pub(crate) fn build_source_change_request(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> SourceChangeRequest {
	let mut request = match source.provider {
		SourceProvider::GitHub => {
			github_provider::build_release_pull_request_request(source, manifest)
		}
		SourceProvider::GitLab => {
			gitlab_provider::build_release_pull_request_request(source, manifest)
		}
		SourceProvider::Gitea => {
			gitea_provider::build_release_pull_request_request(source, manifest)
		}
	};
	request.commit_message = build_release_commit_message(Some(source), manifest);
	request
}

pub(crate) fn publish_source_release_requests(
	source: &SourceConfiguration,
	requests: &[SourceReleaseRequest],
) -> MonochangeResult<Vec<SourceReleaseOutcome>> {
	match source.provider {
		SourceProvider::GitHub => github_provider::publish_release_requests(source, requests),
		SourceProvider::GitLab => gitlab_provider::publish_release_requests(source, requests),
		SourceProvider::Gitea => gitea_provider::publish_release_requests(source, requests),
	}
}

pub(crate) fn publish_source_change_request(
	source: &SourceConfiguration,
	root: &Path,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<SourceChangeRequestOutcome> {
	match source.provider {
		SourceProvider::GitHub => {
			github_provider::publish_release_pull_request(source, root, request, tracked_paths)
		}
		SourceProvider::GitLab => {
			gitlab_provider::publish_release_pull_request(source, root, request, tracked_paths)
		}
		SourceProvider::Gitea => {
			gitea_provider::publish_release_pull_request(source, root, request, tracked_paths)
		}
	}
}

pub(crate) fn format_source_operation(operation: &SourceReleaseOperation) -> &'static str {
	match operation {
		SourceReleaseOperation::Created => "created",
		SourceReleaseOperation::Updated => "updated",
	}
}

pub(crate) fn format_change_request_operation(
	operation: &SourceChangeRequestOperation,
) -> &'static str {
	match operation {
		SourceChangeRequestOperation::Created => "created",
		SourceChangeRequestOperation::Updated => "updated",
	}
}

pub(crate) fn render_release_cli_command_json(
	manifest: &ReleaseManifest,
	releases: &[SourceReleaseRequest],
	release_request: Option<&SourceChangeRequest>,
	issue_comments: &[github_provider::GitHubIssueCommentPlan],
	release_commit: Option<&CommitReleaseReport>,
	file_diffs: &[PreparedFileDiff],
) -> MonochangeResult<String> {
	if releases.is_empty()
		&& release_request.is_none()
		&& issue_comments.is_empty()
		&& release_commit.is_none()
		&& file_diffs.is_empty()
	{
		return render_release_manifest_json(manifest);
	}
	let mut value = json!({
		"manifest": manifest,
		"releaseCommit": release_commit,
		"releases": releases,
		"releaseRequest": release_request,
		"issueComments": issue_comments,
	});
	if !file_diffs.is_empty() {
		value
			.as_object_mut()
			.unwrap_or_else(|| panic!("release json wrapper must stay object"))
			.insert(
				"fileDiffs".to_string(),
				serde_json::to_value(file_diffs).unwrap_or_default(),
			);
	}
	serde_json::to_string_pretty(&value)
		.map_err(|error| MonochangeError::Discovery(error.to_string()))
}

pub(crate) fn commit_release(
	root: &Path,
	context: &CliContext,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> MonochangeResult<CommitReleaseReport> {
	let tracked_paths = tracked_release_pull_request_paths(context, manifest);
	let message = build_release_commit_message(source, manifest);
	if !context.dry_run {
		git_stage_paths(root, &tracked_paths)?;
		git_commit_paths(root, &message)?;
	}
	Ok(CommitReleaseReport {
		subject: message.subject,
		body: message.body.unwrap_or_default(),
		commit: if context.dry_run {
			None
		} else {
			Some(git_head_commit(root)?)
		},
		tracked_paths,
		dry_run: context.dry_run,
		status: if context.dry_run {
			"dry_run".to_string()
		} else {
			"completed".to_string()
		},
	})
}

pub(crate) fn tracked_release_pull_request_paths(
	context: &CliContext,
	manifest: &ReleaseManifest,
) -> Vec<PathBuf> {
	let mut tracked_paths = manifest.changed_files.clone();
	tracked_paths.extend(manifest.deleted_changesets.clone());
	if let Some(path) = &context.release_manifest_path {
		tracked_paths.push(path.clone());
	}
	tracked_paths.sort();
	tracked_paths.dedup();
	tracked_paths
}

pub(crate) fn json_discovery_report(report: &DiscoveryReport) -> serde_json::Value {
	json!({
		"workspaceRoot": PathBuf::from("."),
		"packages": report.packages.iter().map(|package| {
			json!({
				"id": package.id,
				"name": package.name,
				"ecosystem": package.ecosystem.as_str(),
				"manifestPath": root_relative(&report.workspace_root, &package.manifest_path),
				"workspaceRoot": PathBuf::from("."),
				"version": package.current_version.as_ref().map(ToString::to_string),
				"versionGroup": package.version_group_id,
				"publishState": format_publish_state(package.publish_state),
			})
		}).collect::<Vec<_>>(),
		"dependencies": report.dependencies.iter().map(|edge| {
			json!({
				"from": edge.from_package_id,
				"to": edge.to_package_id,
				"kind": edge.dependency_kind.to_string(),
				"direct": edge.is_direct,
			})
		}).collect::<Vec<_>>(),
		"versionGroups": report.version_groups.iter().map(|group| {
			json!({
				"id": group.group_id,
				"members": group.members,
				"mismatchDetected": group.mismatch_detected,
			})
		}).collect::<Vec<_>>(),
		"warnings": report.warnings,
	})
}

pub(crate) fn text_discovery_report(report: &DiscoveryReport) -> String {
	let mut counts = BTreeMap::<Ecosystem, usize>::new();
	for package in &report.packages {
		*counts.entry(package.ecosystem).or_default() += 1;
	}

	let mut lines = vec![format!(
		"Workspace discovery for {}",
		report.workspace_root.display()
	)];
	lines.push(format!("Packages: {}", report.packages.len()));
	for (ecosystem, count) in counts {
		lines.push(format!("- {ecosystem}: {count}"));
	}
	lines.push(format!("Dependencies: {}", report.dependencies.len()));
	if !report.version_groups.is_empty() {
		lines.push("Version groups:".to_string());
		for group in &report.version_groups {
			lines.push(format!("- {} ({})", group.group_id, group.members.len()));
		}
	}
	if !report.warnings.is_empty() {
		lines.push("Warnings:".to_string());
		for warning in &report.warnings {
			lines.push(format!("- {warning}"));
		}
	}
	lines.join("\n")
}
