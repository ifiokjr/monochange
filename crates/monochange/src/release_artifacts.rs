#[cfg(test)]
use std::cell::Cell;
use std::io::IsTerminal;

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
	let package_by_id = packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<BTreeMap<_, _>>();
	let source = configuration.source.as_ref();
	let defaults_release_title = configuration.defaults.release_title.as_deref();
	let defaults_changelog_title = configuration.defaults.changelog_version_title.as_deref();

	// Cache the sorted tag list once for the whole command.
	//
	// Performance note:
	// the previous implementation ran `git tag --list --sort=-v:refname` once per
	// release target. On a repository with multiple release identities that turned
	// a tiny formatting helper into repeated subprocess latency. The target builder
	// only needs a stable view of tags for the current command, so sharing one
	// loaded list avoids re-running the same git command over and over.
	let sorted_tags = load_sorted_tags(&configuration.root_path);

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
						let prev = find_previous_tag_in(&tag, &sorted_tags);
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
		let Some(package) = package_by_id.get(decision.package_id.as_str()).copied() else {
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
		let prev = find_previous_tag_in(&tag, &sorted_tags);
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

pub(crate) fn build_package_publication_targets(
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> Vec<PackagePublicationTarget> {
	let package_by_id = packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<BTreeMap<_, _>>();
	let mut targets = plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
		.filter_map(|decision| {
			let package = package_by_id.get(decision.package_id.as_str()).copied()?;
			let version = decision.planned_version.as_ref()?;
			let config_id = package
				.metadata
				.get("config_id")
				.cloned()
				.unwrap_or_else(|| package.name.clone());
			let package_definition = configuration.package_by_id(&config_id)?;
			if !package_definition.publish.enabled {
				return None;
			}
			Some(PackagePublicationTarget {
				package: config_id,
				ecosystem: package.ecosystem,
				registry: package_definition.publish.registry.clone(),
				version: version.to_string(),
				mode: package_definition.publish.mode,
				trusted_publishing: package_definition.publish.trusted_publishing.clone(),
			})
		})
		.collect::<Vec<_>>();
	targets.sort_by(|left, right| left.package.cmp(&right.package));
	targets
}

pub(crate) fn build_manifest_updates_parallel(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	#[cfg(all(feature = "cargo", feature = "npm", feature = "deno", feature = "dart"))]
	{
		let ((cargo_updates, npm_updates), (deno_updates, dart_updates)) = rayon::join(
			|| {
				rayon::join(
					|| build_cargo_manifest_updates(packages, plan),
					|| build_npm_manifest_updates(packages, plan),
				)
			},
			|| {
				rayon::join(
					|| build_deno_manifest_updates(packages, plan),
					|| build_dart_manifest_updates(packages, plan),
				)
			},
		);
		Ok([cargo_updates?, npm_updates?, deno_updates?, dart_updates?].concat())
	}

	#[cfg(not(all(feature = "cargo", feature = "npm", feature = "deno", feature = "dart")))]
	{
		let mut updates = Vec::new();
		#[cfg(feature = "cargo")]
		updates.extend(build_cargo_manifest_updates(packages, plan)?);
		#[cfg(feature = "npm")]
		updates.extend(build_npm_manifest_updates(packages, plan)?);
		#[cfg(feature = "deno")]
		updates.extend(build_deno_manifest_updates(packages, plan)?);
		#[cfg(feature = "dart")]
		updates.extend(build_dart_manifest_updates(packages, plan)?);
		Ok(updates)
	}
}

#[allow(clippy::match_same_arms)]
pub(crate) fn render_tag_name(id: &str, version: &str, version_format: VersionFormat) -> String {
	match version_format {
		VersionFormat::Namespaced => format!("{id}/v{version}"),
		VersionFormat::Primary => format!("v{version}"),
		_ => format!("v{version}"),
	}
}

/// Dispatch tag URL generation to the appropriate provider crate.
pub(crate) fn tag_url_for_provider(source: &SourceConfiguration, tag_name: &str) -> String {
	match source.provider {
		#[cfg(feature = "github")]
		SourceProvider::GitHub => github_provider::tag_url(source, tag_name),
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => gitlab_provider::tag_url(source, tag_name),
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => gitea_provider::tag_url(source, tag_name),
		#[cfg(not(any(feature = "github", feature = "gitlab", feature = "gitea")))]
		_ => String::new(),
	}
}

/// Dispatch compare URL generation to the appropriate provider crate.
pub(crate) fn compare_url_for_provider(
	source: &SourceConfiguration,
	previous_tag: &str,
	current_tag: &str,
) -> String {
	match source.provider {
		#[cfg(feature = "github")]
		SourceProvider::GitHub => github_provider::compare_url(source, previous_tag, current_tag),
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => gitlab_provider::compare_url(source, previous_tag, current_tag),
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => gitea_provider::compare_url(source, previous_tag, current_tag),
		#[cfg(not(any(feature = "github", feature = "gitlab", feature = "gitea")))]
		_ => String::new(),
	}
}

fn load_sorted_tags(root: &Path) -> Vec<String> {
	let output = match monochange_core::git::git_command_output(
		root,
		&["tag", "--list", "--sort=-v:refname"],
	) {
		Ok(output) if output.status.success() => output,
		_ => return Vec::new(),
	};
	String::from_utf8_lossy(&output.stdout)
		.lines()
		.map(str::trim)
		.filter(|tag| !tag.is_empty())
		.map(ToString::to_string)
		.collect()
}

fn find_previous_tag_in(current_tag: &str, sorted_tags: &[String]) -> Option<String> {
	let (prefix, current_version) = parse_tag_prefix_and_version(current_tag)?;
	sorted_tags
		.iter()
		.filter(|tag| tag.as_str() != current_tag)
		.filter_map(|tag| {
			let (candidate_prefix, candidate_version) = parse_tag_prefix_and_version(tag)?;
			(candidate_prefix == prefix && candidate_version < current_version)
				.then(|| (tag.clone(), candidate_version))
		})
		.max_by(|left, right| left.1.cmp(&right.1))
		.map(|(tag, _)| tag)
}

#[cfg(test)]
pub(crate) fn find_previous_tag(root: &Path, current_tag: &str) -> Option<String> {
	find_previous_tag_in(current_tag, &load_sorted_tags(root))
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

	let Ok(env_date) = std::env::var("MONOCHANGE_RELEASE_DATE") else {
		return chrono::Local::now().naive_local();
	};

	if let Ok(ndt) = NaiveDateTime::parse_from_str(&env_date, "%Y-%m-%dT%H:%M:%S") {
		return ndt;
	}

	if let Ok(nd) = NaiveDate::parse_from_str(&env_date, "%Y-%m-%d") {
		return nd.and_hms_opt(0, 0, 0).unwrap_or_default();
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

#[allow(clippy::match_same_arms)]
pub(crate) fn default_release_title_for_format(version_format: VersionFormat) -> &'static str {
	match version_format {
		VersionFormat::Primary => DEFAULT_RELEASE_TITLE_PRIMARY,
		VersionFormat::Namespaced => DEFAULT_RELEASE_TITLE_NAMESPACED,
		_ => DEFAULT_RELEASE_TITLE_PRIMARY,
	}
}

#[allow(clippy::match_same_arms)]
pub(crate) fn default_changelog_version_title_for_format(
	version_format: VersionFormat,
) -> &'static str {
	match version_format {
		VersionFormat::Primary => DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY,
		VersionFormat::Namespaced => DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED,
		_ => DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY,
	}
}

#[cfg(feature = "cargo")]
pub(crate) fn build_cargo_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	use rayon::prelude::*;

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

	let mut updated_documents = packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Cargo)
		.par_bridge()
		.filter_map(|package| {
			let should_update_manifest = released_versions.contains_key(&package.id)
				|| package
					.declared_dependencies
					.iter()
					.any(|dependency| released_versions_by_name.contains_key(&dependency.name));
			should_update_manifest.then_some(package)
		})
		.map(|package| {
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
			Ok((package.manifest_path.clone(), updated))
		})
		.collect::<MonochangeResult<BTreeMap<_, _>>>()?;

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
		.map(|(path, document)| {
			FileUpdate {
				path,
				content: document.into_bytes(),
			}
		})
		.collect())
}

#[cfg(feature = "npm")]
pub(crate) fn build_npm_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	use rayon::prelude::*;

	let released_versions = released_versions_by_record_id(plan);
	packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Npm)
		.par_bridge()
		.filter_map(|package| {
			released_versions
				.get(&package.id)
				.map(|version| (package, version))
		})
		.map(|(package, version)| {
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
			Ok(FileUpdate {
				path: package.manifest_path.clone(),
				content: rendered.into_bytes(),
			})
		})
		.collect()
}

#[cfg(feature = "deno")]
pub(crate) fn build_deno_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	use rayon::prelude::*;

	let released_versions = released_versions_by_record_id(plan);
	packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Deno)
		.par_bridge()
		.filter_map(|package| {
			released_versions
				.get(&package.id)
				.map(|version| (package, version))
		})
		.map(|(package, version)| {
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
			Ok(FileUpdate {
				path: package.manifest_path.clone(),
				content: rendered.into_bytes(),
			})
		})
		.collect()
}

#[cfg(feature = "dart")]
pub(crate) fn build_dart_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	use rayon::prelude::*;

	let released_versions = released_versions_by_record_id(plan);
	packages
		.iter()
		.filter(|package| {
			package.ecosystem == Ecosystem::Dart || package.ecosystem == Ecosystem::Flutter
		})
		.par_bridge()
		.filter_map(|package| {
			released_versions
				.get(&package.id)
				.map(|version| (package, version))
		})
		.map(|(package, version)| {
			let contents = fs::read_to_string(&package.manifest_path).map_err(|error| {
				MonochangeError::Io(format!(
					"failed to read {}: {error}",
					package.manifest_path.display()
				))
			})?;
			let rendered = monochange_dart::update_manifest_text(
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
			Ok(FileUpdate {
				path: package.manifest_path.clone(),
				content: rendered.into_bytes(),
			})
		})
		.collect()
}

#[must_use = "the file update result must be checked"]
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
	// Capture original permissions before overwriting (if the file exists).
	let original_permissions = fs::metadata(path).ok().map(|meta| meta.permissions());
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
	// Restore original permissions after rename.
	if let Some(permissions) = original_permissions {
		fs::set_permissions(path, permissions).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to restore permissions on {}: {error}",
				path.display()
			))
		})?;
	}
	Ok(())
}

#[rustfmt::skip]
#[tracing::instrument(skip_all)]
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
		OutputFormat::Json => {
			serde_json::to_string_pretty(&json_discovery_report(report))
				.map_err(|error| MonochangeError::Discovery(error.to_string()))
		}
		OutputFormat::Markdown | OutputFormat::Text => Ok(text_discovery_report(report)),
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
			.map(|target| {
				ReleaseManifestTarget {
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
				}
			})
			.collect(),
		released_packages: prepared_release.released_packages.clone(),
		changed_files: prepared_release.changed_files.clone(),
		changelogs: prepared_release
			.changelogs
			.iter()
			.map(|changelog| {
				ReleaseManifestChangelog {
					owner_id: changelog.owner_id.clone(),
					owner_kind: changelog.owner_kind,
					path: changelog.path.clone(),
					format: changelog.format,
					notes: changelog.notes.clone(),
					rendered: changelog.rendered.clone(),
				}
			})
			.collect(),
		package_publications: prepared_release.package_publications.clone(),
		changesets: prepared_release.changesets.clone(),
		deleted_changesets: prepared_release.deleted_changesets.clone(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: prepared_release
				.plan
				.decisions
				.iter()
				.map(|decision| {
					ReleaseManifestPlanDecision {
						package: decision.package_id.clone(),
						bump: decision.recommended_bump,
						trigger: decision.trigger_type.clone(),
						planned_version: decision.planned_version.as_ref().map(ToString::to_string),
						reasons: decision.reasons.clone(),
						upstream_sources: decision.upstream_sources.clone(),
					}
				})
				.collect(),
			groups: prepared_release
				.plan
				.groups
				.iter()
				.map(|group| {
					ReleaseManifestPlanGroup {
						id: group.group_id.clone(),
						planned_version: group.planned_version.as_ref().map(ToString::to_string),
						members: group.members.clone(),
						bump: group.recommended_bump,
					}
				})
				.collect(),
			warnings: prepared_release.plan.warnings.clone(),
			unresolved_items: prepared_release.plan.unresolved_items.clone(),
			compatibility_evidence: prepared_release
				.plan
				.compatibility_evidence
				.iter()
				.map(|assessment| {
					ReleaseManifestCompatibilityEvidence {
						package: assessment.package_id.clone(),
						provider: assessment.provider_id.clone(),
						severity: assessment.severity,
						summary: assessment.summary.clone(),
						confidence: assessment.confidence.clone(),
						evidence_location: assessment.evidence_location.clone(),
					}
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
			.map(|target| {
				ReleaseRecordTarget {
					id: target.id.clone(),
					kind: target.kind,
					version: target.version.clone(),
					version_format: target.version_format,
					tag: target.tag,
					release: target.release,
					tag_name: target.tag_name.clone(),
					members: target.members.clone(),
				}
			})
			.collect(),
		released_packages: manifest.released_packages.clone(),
		changed_files: manifest.changed_files.clone(),
		package_publications: manifest.package_publications.clone(),
		updated_changelogs: manifest
			.changelogs
			.iter()
			.map(|changelog| changelog.path.clone())
			.collect(),
		deleted_changesets: manifest.deleted_changesets.clone(),
		provider: source.map(|source| {
			ReleaseRecordProvider {
				kind: source.provider,
				owner: source.owner.clone(),
				repo: source.repo.clone(),
				host: source.host.clone(),
			}
		}),
	}
}

pub(crate) fn build_release_commit_message(
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> CommitMessage {
	CommitMessage {
		subject: source.map_or_else(
			|| monochange_core::ProviderMergeRequestSettings::default().title,
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

#[must_use = "the manifest render result must be checked"]
pub(crate) fn render_release_manifest_json(manifest: &ReleaseManifest) -> MonochangeResult<String> {
	serde_json::to_string_pretty(manifest)
		.map_err(|error| MonochangeError::Discovery(error.to_string()))
}

pub(crate) fn build_source_release_requests(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> Vec<SourceReleaseRequest> {
	match source.provider {
		#[cfg(feature = "github")]
		SourceProvider::GitHub => github_provider::build_release_requests(source, manifest),
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => gitlab_provider::build_release_requests(source, manifest),
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => gitea_provider::build_release_requests(source, manifest),
		#[cfg(not(any(feature = "github", feature = "gitlab", feature = "gitea")))]
		_ => Vec::new(),
	}
}

pub(crate) fn build_source_change_request(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> SourceChangeRequest {
	let mut request = match source.provider {
		#[cfg(feature = "github")]
		SourceProvider::GitHub => github_provider::build_release_pull_request_request(source, manifest),
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => gitlab_provider::build_release_pull_request_request(source, manifest),
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => gitea_provider::build_release_pull_request_request(source, manifest),
		#[cfg(not(any(feature = "github", feature = "gitlab", feature = "gitea")))]
		_ => {
			unreachable!(
				"a hosting provider feature must be enabled to build source change requests"
			)
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
		#[cfg(feature = "github")]
		SourceProvider::GitHub => github_provider::publish_release_requests(source, requests),
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => gitlab_provider::publish_release_requests(source, requests),
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => gitea_provider::publish_release_requests(source, requests),
		#[cfg(not(any(feature = "github", feature = "gitlab", feature = "gitea")))]
		_ => Ok(Vec::new()),
	}
}

pub(crate) fn publish_source_change_request(
	source: &SourceConfiguration,
	root: &Path,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<SourceChangeRequestOutcome> {
	match source.provider {
		#[cfg(feature = "github")]
		SourceProvider::GitHub => {
			github_provider::publish_release_pull_request(source, root, request, tracked_paths)
		}
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => {
			gitlab_provider::publish_release_pull_request(source, root, request, tracked_paths)
		}
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => {
			gitea_provider::publish_release_pull_request(source, root, request, tracked_paths)
		}
		#[cfg(not(any(feature = "github", feature = "gitlab", feature = "gitea")))]
		_ => {
			Err(MonochangeError::Config(
				"no hosting provider feature enabled".to_string(),
			))
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
		SourceChangeRequestOperation::Skipped => "skipped",
	}
}

pub(crate) struct ReleaseCliJsonSections<'a> {
	pub releases: &'a [SourceReleaseRequest],
	pub release_request: Option<&'a SourceChangeRequest>,
	pub issue_comments: &'a [HostedIssueCommentPlan],
	pub release_commit: Option<&'a CommitReleaseReport>,
	pub package_publish: Option<&'a package_publish::PackagePublishReport>,
	pub publish_rate_limits: Option<&'a monochange_core::PublishRateLimitReport>,
	pub file_diffs: &'a [PreparedFileDiff],
}

pub(crate) fn render_release_cli_command_json(
	manifest: &ReleaseManifest,
	sections: &ReleaseCliJsonSections,
) -> MonochangeResult<String> {
	if sections.releases.is_empty()
		&& sections.release_request.is_none()
		&& sections.issue_comments.is_empty()
		&& sections.release_commit.is_none()
		&& sections.package_publish.is_none()
		&& sections.publish_rate_limits.is_none()
		&& sections.file_diffs.is_empty()
	{
		return render_release_manifest_json(manifest);
	}
	let mut value = json!({
		"manifest": manifest,
		"releaseCommit": sections.release_commit,
		"releases": sections.releases,
		"releaseRequest": sections.release_request,
		"issueComments": sections.issue_comments,
		"packagePublish": sections.package_publish,
		"publishRateLimits": sections.publish_rate_limits,
	});
	if !sections.file_diffs.is_empty() {
		#[rustfmt::skip]
		value.as_object_mut().unwrap_or_else(|| panic!("release json wrapper must stay object")).insert("fileDiffs".to_string(), serde_json::to_value(sections.file_diffs).unwrap_or_default());
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

#[cfg(test)]
mod tests {
	use std::fs;

	use monochange_core::GroupChangelogInclude;
	use monochange_core::PackageDefinition;
	use monochange_core::PackageType;
	use monochange_core::ProviderBotSettings;
	use monochange_core::ProviderMergeRequestSettings;
	use monochange_core::ProviderReleaseNotesSource;
	use monochange_core::ProviderReleaseSettings;
	use monochange_core::PublishMode;
	use monochange_core::PublishRegistry;
	use monochange_core::PublishSettings;
	use monochange_core::PublishState;
	use monochange_core::RegistryKind;
	use monochange_core::ReleaseDecision;
	use monochange_core::ReleaseManifest;
	use monochange_core::ReleaseManifestChangelog;
	use monochange_core::ReleaseManifestCompatibilityEvidence;
	use monochange_core::ReleaseManifestPlan;
	use monochange_core::ReleaseManifestPlanDecision;
	use monochange_core::ReleaseManifestPlanGroup;
	use monochange_core::ReleaseManifestTarget;
	use monochange_core::SourceChangeRequest;
	use monochange_core::SourceConfiguration;
	use monochange_core::SourceProvider;
	use monochange_core::WorkspaceConfiguration;
	use monochange_core::WorkspaceDefaults;
	use semver::Version;
	use tempfile::tempdir;

	use super::*;

	fn empty_configuration(root: &Path) -> WorkspaceConfiguration {
		WorkspaceConfiguration {
			root_path: root.to_path_buf(),
			defaults: WorkspaceDefaults::default(),
			changelog: ChangelogSettings::default(),
			packages: Vec::new(),
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
			lints: monochange_core::lint::WorkspaceLintSettings::default(),
			cargo: monochange_core::EcosystemSettings::default(),
			npm: monochange_core::EcosystemSettings::default(),
			deno: monochange_core::EcosystemSettings::default(),
			dart: monochange_core::EcosystemSettings::default(),
		}
	}

	fn source_configuration(provider: SourceProvider) -> SourceConfiguration {
		SourceConfiguration {
			provider,
			owner: "acme".to_string(),
			repo: "monochange".to_string(),
			host: Some("https://example.com".to_string()),
			api_url: None,
			releases: ProviderReleaseSettings {
				generate_notes: matches!(provider, SourceProvider::GitHub),
				source: ProviderReleaseNotesSource::Monochange,
				..ProviderReleaseSettings::default()
			},
			pull_requests: ProviderMergeRequestSettings::default(),
			bot: ProviderBotSettings::default(),
		}
	}

	fn sample_manifest() -> ReleaseManifest {
		ReleaseManifest {
			command: "release".to_string(),
			dry_run: false,
			version: Some("1.2.3".to_string()),
			group_version: Some("2.0.0".to_string()),
			release_targets: vec![ReleaseManifestTarget {
				id: "sdk".to_string(),
				kind: ReleaseOwnerKind::Group,
				version: "2.0.0".to_string(),
				tag: true,
				release: true,
				version_format: VersionFormat::Namespaced,
				tag_name: "sdk/v2.0.0".to_string(),
				members: vec!["pkg-a".to_string(), "pkg-b".to_string()],
				rendered_title: "Release sdk v2.0.0".to_string(),
				rendered_changelog_title: "sdk v2.0.0".to_string(),
			}],
			released_packages: vec!["pkg-a".to_string(), "pkg-b".to_string()],
			changed_files: vec![
				PathBuf::from("Cargo.toml"),
				PathBuf::from("packages/pkg-a/package.json"),
			],
			changelogs: vec![ReleaseManifestChangelog {
				owner_id: "sdk".to_string(),
				owner_kind: ReleaseOwnerKind::Group,
				path: PathBuf::from("CHANGELOG.md"),
				format: ChangelogFormat::Monochange,
				notes: ReleaseNotesDocument {
					title: "2.0.0".to_string(),
					summary: vec!["Grouped release".to_string()],
					sections: vec![ReleaseNotesSection {
						title: "Features".to_string(),
						entries: vec!["- Added batching".to_string()],
					}],
				},
				rendered: "## 2.0.0\n- Added batching".to_string(),
			}],
			changesets: Vec::new(),
			deleted_changesets: vec![PathBuf::from(".changeset/feature.md")],
			package_publications: Vec::new(),
			plan: ReleaseManifestPlan {
				workspace_root: PathBuf::from("."),
				decisions: vec![ReleaseManifestPlanDecision {
					package: "pkg-a".to_string(),
					bump: BumpSeverity::Minor,
					trigger: "changeset".to_string(),
					planned_version: Some("1.2.3".to_string()),
					reasons: vec!["feature".to_string()],
					upstream_sources: vec!["github".to_string()],
				}],
				groups: vec![ReleaseManifestPlanGroup {
					id: "sdk".to_string(),
					planned_version: Some("2.0.0".to_string()),
					members: vec!["pkg-a".to_string(), "pkg-b".to_string()],
					bump: BumpSeverity::Minor,
				}],
				warnings: vec!["warn".to_string()],
				unresolved_items: vec!["todo".to_string()],
				compatibility_evidence: vec![ReleaseManifestCompatibilityEvidence {
					package: "pkg-a".to_string(),
					provider: "rust-semver".to_string(),
					severity: BumpSeverity::Minor,
					summary: "minor api expansion".to_string(),
					confidence: "high".to_string(),
					evidence_location: Some("src/lib.rs".to_string()),
				}],
			},
		}
	}

	fn sample_package(root: &Path, config_id: &str, package_type: PackageType) -> PackageRecord {
		let manifest_path = root.join(format!("{config_id}/manifest"));
		fs::create_dir_all(
			manifest_path
				.parent()
				.unwrap_or_else(|| panic!("manifest path should have a parent")),
		)
		.unwrap_or_else(|error| panic!("create package dir: {error}"));
		fs::write(&manifest_path, "manifest\n")
			.unwrap_or_else(|error| panic!("write manifest: {error}"));
		let ecosystem = match package_type {
			PackageType::Cargo => Ecosystem::Cargo,
			PackageType::Npm => Ecosystem::Npm,
			PackageType::Deno => Ecosystem::Deno,
			PackageType::Dart => Ecosystem::Dart,
			PackageType::Flutter => Ecosystem::Flutter,
			_ => unreachable!("unsupported package type in sample_package"),
		};
		let mut package = PackageRecord::new(
			ecosystem,
			config_id,
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

	#[test]
	fn release_target_and_title_helpers_cover_provider_and_skip_paths() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		let mut configuration = empty_configuration(root);
		let source = source_configuration(SourceProvider::Gitea);
		configuration.source = Some(source.clone());
		configuration.packages = vec![PackageDefinition {
			id: "pkg-a".to_string(),
			path: PathBuf::from("pkg-a"),
			package_type: PackageType::Cargo,
			changelog: None,
			excluded_changelog_types: Vec::new(),
			empty_update_message: None,
			release_title: Some(
				"Package {{ id }} {{ previous_version }} -> {{ version }}".to_string(),
			),
			changelog_version_title: Some("{{ version }}".to_string()),
			versioned_files: Vec::new(),
			ignore_ecosystem_versioned_files: false,
			ignored_paths: Vec::new(),
			additional_paths: Vec::new(),
			tag: true,
			release: true,
			publish: PublishSettings::default(),
			version_format: VersionFormat::Namespaced,
		}];
		configuration.groups = vec![monochange_core::GroupDefinition {
			id: "sdk".to_string(),
			packages: vec!["pkg-a".to_string()],
			changelog: None,
			changelog_include: GroupChangelogInclude::All,
			excluded_changelog_types: Vec::new(),
			empty_update_message: None,
			release_title: Some("Group {{ id }} {{ compare_url }}".to_string()),
			changelog_version_title: None,
			versioned_files: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Namespaced,
		}];
		let package = sample_package(root, "pkg-a", PackageType::Cargo);
		let sorted_tags = vec![
			"sdk/v2.0.0".to_string(),
			"sdk/v1.5.0".to_string(),
			"pkg-a/v1.0.0".to_string(),
			"pkg-a/v0.9.0".to_string(),
		];
		assert_eq!(
			find_previous_tag_in("pkg-a/v1.0.0", &sorted_tags),
			Some("pkg-a/v0.9.0".to_string())
		);
		assert_eq!(
			parse_tag_prefix_and_version("pkg-a/v1.2.3"),
			Some(("pkg-a/v".to_string(), Version::new(1, 2, 3)))
		);
		assert_eq!(
			compare_url_for_provider(&source, "pkg-a/v0.9.0", "pkg-a/v1.0.0"),
			"https://example.com/acme/monochange/compare/pkg-a/v0.9.0...pkg-a/v1.0.0"
		);

		let plan = ReleasePlan {
			workspace_root: root.to_path_buf(),
			decisions: vec![
				ReleaseDecision {
					package_id: "missing".to_string(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Patch,
					planned_version: Some(Version::new(1, 0, 1)),
					group_id: None,
					reasons: Vec::new(),
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
				ReleaseDecision {
					package_id: package.id.clone(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Patch,
					planned_version: None,
					group_id: None,
					reasons: Vec::new(),
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
				ReleaseDecision {
					package_id: package.id.clone(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Minor,
					planned_version: Some(Version::new(1, 0, 0)),
					group_id: None,
					reasons: vec!["feature".to_string()],
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
			],
			groups: vec![monochange_core::PlannedVersionGroup {
				group_id: "sdk".to_string(),
				display_name: "SDK".to_string(),
				members: vec![package.id.clone()],
				mismatch_detected: false,
				planned_version: Some(Version::new(2, 0, 0)),
				recommended_bump: BumpSeverity::Minor,
			}],
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		};

		let targets = build_release_targets(
			&configuration,
			&[package],
			&plan,
			&[PathBuf::from(".changeset/feature.md")],
		);
		assert_eq!(targets.len(), 2);
		assert!(
			targets
				.iter()
				.any(|target| target.id == "sdk" && !target.rendered_title.is_empty())
		);
		assert!(
			targets
				.iter()
				.all(|target| !target.rendered_changelog_title.is_empty())
		);

		assert_eq!(
			effective_title_template(Some("specific"), Some("default"), "builtin"),
			"specific"
		);
		assert_eq!(
			effective_title_template(None, Some("default"), "builtin"),
			"default"
		);
		assert_eq!(
			default_release_title_for_format(VersionFormat::Primary),
			DEFAULT_RELEASE_TITLE_PRIMARY
		);
		assert_eq!(
			default_changelog_version_title_for_format(VersionFormat::Namespaced),
			DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED
		);
		assert!(
			build_cargo_manifest_updates(
				&[],
				&ReleasePlan {
					workspace_root: root.to_path_buf(),
					decisions: Vec::new(),
					groups: Vec::new(),
					warnings: Vec::new(),
					unresolved_items: Vec::new(),
					compatibility_evidence: Vec::new(),
				}
			)
			.unwrap_or_else(|error| panic!("build empty cargo manifest updates: {error}"))
			.is_empty()
		);

		assert!(
			!resolve_release_datetime()
				.format("%Y-%m-%d")
				.to_string()
				.is_empty()
		);
	}

	#[test]
	fn resolve_release_datetime_falls_back_for_invalid_environment_values() {
		temp_env::with_var("MONOCHANGE_RELEASE_DATE", Some("not-a-date"), || {
			assert!(
				!resolve_release_datetime()
					.format("%Y-%m-%d")
					.to_string()
					.is_empty()
			);
		});
	}

	#[test]
	fn build_package_publication_targets_filters_disabled_and_preserves_publish_metadata() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		let mut configuration = empty_configuration(root);
		configuration.packages = vec![
			PackageDefinition {
				id: "core".to_string(),
				path: PathBuf::from("core"),
				package_type: PackageType::Cargo,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				publish: PublishSettings {
					registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
					..PublishSettings::default()
				},
				version_format: VersionFormat::Primary,
			},
			PackageDefinition {
				id: "web".to_string(),
				path: PathBuf::from("web"),
				package_type: PackageType::Npm,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				publish: PublishSettings {
					mode: PublishMode::External,
					registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
					..PublishSettings::default()
				},
				version_format: VersionFormat::Primary,
			},
			PackageDefinition {
				id: "private".to_string(),
				path: PathBuf::from("private"),
				package_type: PackageType::Cargo,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				publish: PublishSettings {
					enabled: false,
					registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
					..PublishSettings::default()
				},
				version_format: VersionFormat::Primary,
			},
		];

		let packages = vec![
			sample_package(root, "core", PackageType::Cargo),
			sample_package(root, "web", PackageType::Npm),
			sample_package(root, "private", PackageType::Cargo),
		];
		let plan = ReleasePlan {
			workspace_root: root.to_path_buf(),
			decisions: vec![
				ReleaseDecision {
					package_id: "cargo:core/manifest".to_string(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Minor,
					planned_version: Some(Version::new(1, 2, 0)),
					group_id: None,
					reasons: vec!["feature".to_string()],
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
				ReleaseDecision {
					package_id: "npm:web/manifest".to_string(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Patch,
					planned_version: Some(Version::new(2, 0, 1)),
					group_id: None,
					reasons: vec!["fix".to_string()],
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
				ReleaseDecision {
					package_id: "cargo:private/manifest".to_string(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Patch,
					planned_version: Some(Version::new(1, 0, 1)),
					group_id: None,
					reasons: vec!["fix".to_string()],
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
				ReleaseDecision {
					package_id: "cargo:core/manifest".to_string(),
					trigger_type: "metadata".to_string(),
					recommended_bump: BumpSeverity::None,
					planned_version: Some(Version::new(9, 9, 9)),
					group_id: None,
					reasons: Vec::new(),
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
			],
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		};

		let targets = build_package_publication_targets(&configuration, &packages, &plan);
		assert_eq!(
			targets,
			vec![
				PackagePublicationTarget {
					package: "core".to_string(),
					ecosystem: Ecosystem::Cargo,
					registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
					version: "1.2.0".to_string(),
					mode: PublishMode::Builtin,
					trusted_publishing: monochange_core::TrustedPublishingSettings::default(),
				},
				PackagePublicationTarget {
					package: "web".to_string(),
					ecosystem: Ecosystem::Npm,
					registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
					version: "2.0.1".to_string(),
					mode: PublishMode::External,
					trusted_publishing: monochange_core::TrustedPublishingSettings::default(),
				},
			]
		);
	}

	#[test]
	fn build_release_manifest_copies_package_publications_from_prepared_release() {
		let cli_command = CliCommandDefinition {
			name: "release".to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: Vec::new(),
		};
		let prepared_release = PreparedRelease {
			plan: ReleasePlan {
				workspace_root: PathBuf::from("."),
				decisions: Vec::new(),
				groups: Vec::new(),
				warnings: Vec::new(),
				unresolved_items: Vec::new(),
				compatibility_evidence: Vec::new(),
			},
			changeset_paths: Vec::new(),
			changesets: Vec::new(),
			released_packages: vec!["core".to_string()],
			version: Some("1.2.3".to_string()),
			group_version: None,
			release_targets: Vec::new(),
			changed_files: Vec::new(),
			changelogs: Vec::new(),
			updated_changelogs: Vec::new(),
			deleted_changesets: Vec::new(),
			package_publications: vec![PackagePublicationTarget {
				package: "core".to_string(),
				ecosystem: Ecosystem::Cargo,
				registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
				version: "1.2.3".to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: monochange_core::TrustedPublishingSettings::default(),
			}],
			dry_run: false,
		};

		let manifest = build_release_manifest(&cli_command, &prepared_release, &[]);
		assert_eq!(
			manifest.package_publications,
			prepared_release.package_publications
		);
	}

	#[test]
	fn render_release_cli_command_json_includes_publish_rate_limits_when_present() {
		let manifest = sample_manifest();
		let file_diffs = vec![PreparedFileDiff {
			path: PathBuf::from("Cargo.toml"),
			diff: "-old\n+new".to_string(),
			display_diff: "--- a/Cargo.toml\n+++ b/Cargo.toml\n-old\n+new".to_string(),
		}];
		let json = render_release_cli_command_json(
			&manifest,
			&ReleaseCliJsonSections {
				releases: &[],
				release_request: None,
				issue_comments: &[],
				release_commit: None,
				package_publish: None,
				publish_rate_limits: Some(&monochange_core::PublishRateLimitReport {
					dry_run: true,
					windows: vec![monochange_core::RegistryRateLimitWindowPlan {
						registry: RegistryKind::Npm,
						operation: monochange_core::RateLimitOperation::Publish,
						limit: None,
						window_seconds: None,
						pending: 1,
						batches_required: 1,
						fits_single_window: true,
						confidence: monochange_core::RateLimitConfidence::Low,
						notes: "npm soft limit".to_string(),
						evidence: Vec::new(),
					}],
					batches: vec![monochange_core::PublishRateLimitBatch {
						registry: RegistryKind::Npm,
						operation: monochange_core::RateLimitOperation::Publish,
						batch_index: 1,
						total_batches: 1,
						packages: vec!["pkg".to_string()],
						recommended_wait_seconds: None,
					}],
					warnings: Vec::new(),
				}),
				file_diffs: &file_diffs,
			},
		)
		.unwrap_or_else(|error| panic!("release cli json: {error}"));
		assert!(json.contains("publishRateLimits"));
	}

	#[test]
	fn release_manifest_and_source_helpers_cover_provider_specific_paths() {
		let manifest = sample_manifest();
		let source = source_configuration(SourceProvider::GitLab);
		let record = build_release_record(Some(&source), &manifest);
		assert_eq!(record.kind, monochange_core::RELEASE_RECORD_KIND);
		assert!(record.created_at.ends_with('Z'));
		assert_eq!(
			record
				.provider
				.as_ref()
				.map(|provider| provider.repo.as_str()),
			Some("monochange")
		);
		assert_eq!(
			record.updated_changelogs,
			vec![PathBuf::from("CHANGELOG.md")]
		);
		assert_eq!(record.release_targets[0].tag_name, "sdk/v2.0.0");

		let release_request = build_source_release_requests(&source, &manifest);
		assert_eq!(release_request.len(), 1);
		assert_eq!(release_request[0].provider, SourceProvider::GitLab);

		let change_request = build_source_change_request(&source, &manifest);
		assert_eq!(change_request.provider, SourceProvider::GitLab);
		assert!(
			change_request
				.commit_message
				.body
				.as_deref()
				.is_some_and(|body| body.contains("Prepare release."))
		);

		let gitea = source_configuration(SourceProvider::Gitea);
		let gitea_change_request = build_source_change_request(&gitea, &manifest);
		assert_eq!(gitea_change_request.provider, SourceProvider::Gitea);
		assert!(tag_url_for_provider(&gitea, "sdk/v2.0.0").contains("/releases/tag/"));

		let dry_requests_error = publish_source_release_requests(&gitea, &[])
			.err()
			.unwrap_or_else(|| {
				panic!("expected publishing gitea release requests without auth to fail")
			});
		assert!(dry_requests_error.to_string().contains("GITEA_TOKEN"));

		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let publish_error = publish_source_change_request(
			&gitea,
			tempdir.path(),
			&SourceChangeRequest {
				provider: SourceProvider::Gitea,
				repository: "acme/monochange".to_string(),
				owner: "acme".to_string(),
				repo: "monochange".to_string(),
				base_branch: "main".to_string(),
				head_branch: "release/v2.0.0".to_string(),
				title: "chore: prepare release".to_string(),
				body: "release body".to_string(),
				labels: vec!["release".to_string()],
				auto_merge: false,
				commit_message: build_release_commit_message(Some(&gitea), &manifest),
			},
			&manifest.changed_files,
		)
		.err()
		.unwrap_or_else(|| {
			panic!("expected publishing a gitea change request outside a git repo to fail")
		});
		assert!(
			publish_error.to_string().contains("git")
				|| publish_error.to_string().contains("failed")
		);
	}
}
