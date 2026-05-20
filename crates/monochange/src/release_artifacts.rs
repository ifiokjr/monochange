#[cfg(test)]
use std::cell::Cell;
use std::io::BufRead;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::IsTerminal;

use similar::TextDiff;

use super::*;

#[cfg(test)]
thread_local! {
	pub(crate) static FORCE_BUILD_FILE_DIFF_PREVIEWS_ERROR: Cell<bool> = const { Cell::new(false) };
}

thread_local! {
	pub(crate) static DEDUPLICATED_CACHE: std::cell::RefCell<std::collections::HashSet<(PathBuf, String)>> =
		std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Path to the persistent deduplication index relative to the workspace root.
const DEDUP_INDEX_PATH: &str = ".monochange/local/release-index.jsonl";

/// Load the persistent deduplication index as a set of hashes.
///
/// The index is stored as a JSONL file at `.monochange/local/release-index.jsonl`.
/// Each line is a JSON object with a `hash` field. Missing or unreadable files
/// are treated as empty indices.
fn load_dedup_index(root: &Path) -> std::collections::HashSet<String> {
	let path = root.join(DEDUP_INDEX_PATH);
	let Ok(file) = fs::File::open(&path) else {
		return std::collections::HashSet::new();
	};
	let reader = BufReader::new(file);
	load_dedup_index_from_reader(reader).unwrap_or_default()
}

fn load_dedup_index_from_reader(reader: impl BufRead) -> Option<std::collections::HashSet<String>> {
	let mut index = std::collections::HashSet::new();
	for line in reader.lines() {
		let Ok(line) = line else {
			return None;
		};
		let line = line.trim();
		if line.is_empty() {
			continue;
		}
		if let Some(hash) = parse_dedup_index_hash(line) {
			index.insert(hash.to_owned());
		}
	}
	Some(index)
}

fn parse_dedup_index_hash(line: &str) -> Option<&str> {
	#[derive(serde::Deserialize)]
	struct DedupIndexEntry<'a> {
		#[serde(borrow)]
		hash: &'a str,
	}

	serde_json::from_str::<DedupIndexEntry<'_>>(line)
		.ok()
		.map(|entry| entry.hash)
}

/// Save the persistent deduplication index atomically.
///
/// Writes to a temporary file next to the target and renames it into place.
/// This avoids corrupting the index if the process is interrupted mid-write.
fn save_dedup_index(
	root: &Path,
	index: &std::collections::HashSet<String>,
) -> MonochangeResult<()> {
	let path = root.join(DEDUP_INDEX_PATH);
	let parent = path.parent().unwrap_or(root);
	fs::create_dir_all(parent)
		.map_err(|error| MonochangeError::Io(format!("create dedup index dir: {error}")))?;
	let mut hashes = index.iter().map(String::as_str).collect::<Vec<_>>();
	hashes.sort_unstable();
	let temp = path.with_extension("tmp");
	let file = fs::File::create(&temp)
		.map_err(|error| MonochangeError::Io(format!("write dedup index: {error}")))?;
	let mut writer = BufWriter::new(file);
	for (position, hash) in hashes.iter().enumerate() {
		if position > 0 {
			std::io::Write::write_all(&mut writer, b"\n")
				.map_err(|error| MonochangeError::Io(format!("write dedup index: {error}")))?;
		}
		std::io::Write::write_fmt(&mut writer, format_args!(r#"{{"hash":"{hash}"}}"#))
			.map_err(|error| MonochangeError::Io(format!("write dedup index: {error}")))?;
	}
	std::io::Write::flush(&mut writer)
		.map_err(|error| MonochangeError::Io(format!("write dedup index: {error}")))?;
	fs::rename(&temp, &path)
		.map_err(|error| MonochangeError::Io(format!("rename dedup index: {error}")))?;
	Ok(())
}

/// Add a hash to the persistent deduplication index.
fn add_to_dedup_index(root: &Path, hash: &str) -> MonochangeResult<()> {
	let mut index = load_dedup_index(root);
	index.insert(hash.to_string());
	save_dedup_index(root, &index)
}

/// Remove a hash from the persistent deduplication index.
fn remove_from_dedup_index(root: &Path, hash: &str) -> MonochangeResult<()> {
	let mut index = load_dedup_index(root);
	index.remove(hash);
	save_dedup_index(root, &index)
}

pub(crate) async fn build_release_targets(
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
	let sorted_tags = load_sorted_tags(&configuration.root_path).await;
	let configured_package_by_id = configuration
		.packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<std::collections::HashMap<_, _>>();
	let mut group_by_package_id = std::collections::HashMap::with_capacity(
		configuration
			.groups
			.iter()
			.map(|group| group.packages.len())
			.sum(),
	);
	for group in &configuration.groups {
		for package_id in &group.packages {
			group_by_package_id
				.entry(package_id.as_str())
				.or_insert(group);
		}
	}
	let planned_group_by_id = plan
		.groups
		.iter()
		.filter(|group| group.recommended_bump.is_release())
		.map(|group| (group.group_id.as_str(), group))
		.collect::<std::collections::HashMap<_, _>>();

	let mut release_targets = Vec::with_capacity(configuration.groups.len() + plan.decisions.len());
	release_targets.extend(configuration.groups.iter().filter_map(|group| {
		planned_group_by_id.get(group.id.as_str()).and_then(|pg| {
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
	}));
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
		let Some(package_definition) = configured_package_by_id.get(config_id.as_str()).copied()
		else {
			continue;
		};
		let (owner_id, owner_kind, tag_enabled, release_enabled, version_format, members) =
			if let Some(group) = group_by_package_id.get(config_id.as_str()).copied() {
				(
					&group.id,
					ReleaseOwnerKind::Group,
					group.tag,
					group.release,
					group.version_format,
					group.packages.clone(),
				)
			} else {
				(
					&package_definition.id,
					ReleaseOwnerKind::Package,
					package_definition.tag,
					package_definition.release,
					package_definition.version_format,
					vec![package_definition.id.clone()],
				)
			};
		let vs = version.to_string();
		let tag = render_tag_name(owner_id, &vs, version_format);
		let prev = find_previous_tag_in(&tag, &sorted_tags);
		let ctx =
			TitleRenderContext::new(owner_id, &vs, changes_count, source, &tag, prev.as_deref());
		let rt = effective_title_template(
			package_definition.release_title.as_deref(),
			defaults_release_title,
			default_release_title_for_format(version_format),
		);
		let ct = effective_title_template(
			package_definition.changelog_version_title.as_deref(),
			defaults_changelog_title,
			default_changelog_version_title_for_format(version_format),
		);
		release_targets.push(ReleaseTarget {
			id: owner_id.clone(),
			kind: owner_kind,
			version: vs,
			tag: tag_enabled,
			release: release_enabled,
			version_format,
			tag_name: tag,
			members,
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
		.collect::<std::collections::HashMap<_, _>>();
	let configured_package_by_id = configuration
		.packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<std::collections::HashMap<_, _>>();
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
			let package_definition = configured_package_by_id.get(config_id.as_str()).copied()?;
			if !package_definition.publish.enabled
				|| matches!(
					package.publish_state,
					monochange_core::PublishState::Private
						| monochange_core::PublishState::Excluded
				) {
				return None;
			}
			Some(PackagePublicationTarget {
				package: config_id,
				ecosystem: package.ecosystem,
				registry: package_definition.publish.registry.clone(),
				version: version.to_string(),
				mode: package_definition.publish.mode,
				trusted_publishing: package_definition.publish.trusted_publishing.clone(),
				attestations: package_definition.publish.attestations.clone(),
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
		#[cfg(feature = "forgejo")]
		SourceProvider::Forgejo => forgejo_provider::tag_url(source, tag_name),
		#[cfg(not(any(
			feature = "github",
			feature = "gitlab",
			feature = "gitea",
			feature = "forgejo"
		)))]
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
		#[cfg(feature = "forgejo")]
		SourceProvider::Forgejo => forgejo_provider::compare_url(source, previous_tag, current_tag),
		#[cfg(not(any(
			feature = "github",
			feature = "gitlab",
			feature = "gitea",
			feature = "forgejo"
		)))]
		_ => String::new(),
	}
}

async fn load_sorted_tags(root: &Path) -> Vec<String> {
	let output = match monochange_core::git::git_command_output(
		root,
		&["tag", "--list", "--sort=-v:refname"],
	)
	.await
	{
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
pub(crate) async fn find_previous_tag(root: &Path, current_tag: &str) -> Option<String> {
	find_previous_tag_in(current_tag, &load_sorted_tags(root).await)
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
	write_temp_file(&mut temp, path, content)?;
	persist_temp_file(temp, path)?;
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

fn write_temp_file(
	writer: &mut impl std::io::Write,
	path: &Path,
	content: &[u8],
) -> MonochangeResult<()> {
	std::io::Write::write_all(writer, content).map_err(|error| temp_file_write_error(path, &error))
}

fn persist_temp_file(temp: tempfile::NamedTempFile, path: &Path) -> MonochangeResult<()> {
	temp.persist(path)
		.map(|_| ())
		.map_err(|error| temp_file_persist_error(path, &error))
}

fn temp_file_write_error(path: &Path, error: &std::io::Error) -> MonochangeError {
	MonochangeError::Io(format!(
		"failed to write temp file for {}: {error}",
		path.display()
	))
}

fn temp_file_persist_error(path: &Path, error: &tempfile::PersistError) -> MonochangeError {
	MonochangeError::Io(format!(
		"failed to rename temp file to {}: {error}",
		path.display()
	))
}

#[tracing::instrument(skip_all)]
pub(crate) fn build_file_diff_previews(
	root: &Path,
	updates: &[FileUpdate],
) -> MonochangeResult<Vec<PreparedFileDiff>> {
	let colorize_diffs = diff_output_colors_enabled();
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

pub(crate) fn colorize_diff_output(diff: &str) -> String {
	let mut output = String::with_capacity(diff.len());
	for (index, line) in diff.lines().enumerate() {
		if index > 0 {
			output.push('\n');
		}
		push_colorized_diff_line(&mut output, line);
	}
	output
}

fn push_colorized_diff_line(output: &mut String, line: &str) {
	if line.starts_with("--- ") || line.starts_with("+++ ") {
		push_ansi_style(output, line, "1;36");
	} else if line.starts_with("@@ ") {
		push_ansi_style(output, line, "36");
	} else if line.starts_with('+') && !line.starts_with("+++") {
		push_ansi_style(output, line, "32");
	} else if line.starts_with('-') && !line.starts_with("---") {
		push_ansi_style(output, line, "31");
	} else if line == r"\ No newline at end of file" {
		push_ansi_style(output, line, "33");
	} else {
		output.push_str(line);
	}
}

fn push_ansi_style(output: &mut String, line: &str, style: &str) {
	output.push_str("\u{1b}[");
	output.push_str(style);
	output.push('m');
	output.push_str(line);
	output.push_str("\u{1b}[0m");
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

pub(crate) fn build_release_manifest_from_record(record: &ReleaseRecord) -> ReleaseManifest {
	ReleaseManifest {
		command: record.command.clone(),
		dry_run: false,
		version: record.version.clone(),
		group_version: None,
		release_targets: record
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
					rendered_title: String::new(),
					rendered_changelog_title: String::new(),
				}
			})
			.collect(),
		released_packages: record.released_packages.clone(),
		changed_files: record.changed_files.clone(),
		changelogs: if record.changelogs.is_empty() {
			record
				.updated_changelogs
				.iter()
				.map(|path| {
					ReleaseManifestChangelog {
						owner_id: String::new(),
						owner_kind: ReleaseOwnerKind::Group,
						path: path.clone(),
						format: ChangelogFormat::default(),
						notes: ReleaseNotesDocument {
							title: String::new(),
							summary: Vec::new(),
							sections: Vec::new(),
						},
						rendered: String::new(),
					}
				})
				.collect()
		} else {
			record.changelogs.clone()
		},
		package_publications: record.package_publications.clone(),
		changesets: record.changesets.clone(),
		deleted_changesets: record.deleted_changesets.clone(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
	}
}

fn release_record_versions(release_targets: &[ReleaseManifestTarget]) -> BTreeMap<String, String> {
	release_targets
		.iter()
		.map(|target| (target.id.clone(), target.version.clone()))
		.collect()
}

pub(crate) fn build_release_record(
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> ReleaseRecord {
	ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: resolve_release_datetime()
			.and_utc()
			.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
		command: manifest.command.clone(),
		version: manifest.version.clone(),
		versions: release_record_versions(&manifest.release_targets),
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
		changelogs: manifest.changelogs.clone(),
		deleted_changesets: manifest.deleted_changesets.clone(),
		changesets: manifest.changesets.clone(),
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
	_source: Option<&SourceConfiguration>,
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
	lines.join("\n")
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
		#[cfg(feature = "forgejo")]
		SourceProvider::Forgejo => forgejo_provider::build_release_requests(source, manifest),
		#[cfg(not(any(
			feature = "github",
			feature = "gitlab",
			feature = "gitea",
			feature = "forgejo"
		)))]
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
		#[cfg(feature = "forgejo")]
		SourceProvider::Forgejo => forgejo_provider::build_release_pull_request_request(source, manifest),
		#[cfg(not(any(
			feature = "github",
			feature = "gitlab",
			feature = "gitea",
			feature = "forgejo"
		)))]
		_ => {
			unreachable!(
				"a hosting provider feature must be enabled to build source change requests"
			)
		}
	};
	request.commit_message = build_release_commit_message(Some(source), manifest);
	request
}

pub(crate) async fn publish_source_release_requests(
	source: &SourceConfiguration,
	requests: &[SourceReleaseRequest],
) -> MonochangeResult<Vec<SourceReleaseOutcome>> {
	match source.provider {
		#[cfg(feature = "github")]
		SourceProvider::GitHub => github_provider::publish_release_requests(source, requests).await,
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => gitlab_provider::publish_release_requests(source, requests).await,
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => gitea_provider::publish_release_requests(source, requests).await,
		#[cfg(feature = "forgejo")]
		SourceProvider::Forgejo => forgejo_provider::publish_release_requests(source, requests).await,
		#[cfg(not(any(
			feature = "github",
			feature = "gitlab",
			feature = "gitea",
			feature = "forgejo"
		)))]
		_ => Ok(Vec::new()),
	}
}

pub(crate) async fn publish_source_change_request(
	source: &SourceConfiguration,
	root: &Path,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
	no_verify: bool,
	stage_all: bool,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	match source.provider {
		#[cfg(feature = "github")]
		SourceProvider::GitHub => {
			github_provider::publish_release_pull_request(
				source,
				root,
				request,
				tracked_paths,
				no_verify,
				stage_all,
			)
			.await
		}
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => {
			gitlab_provider::publish_release_pull_request(
				source,
				root,
				request,
				tracked_paths,
				no_verify,
				stage_all,
			)
			.await
		}
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => {
			gitea_provider::publish_release_pull_request(
				source,
				root,
				request,
				tracked_paths,
				no_verify,
				stage_all,
			)
			.await
		}
		#[cfg(feature = "forgejo")]
		SourceProvider::Forgejo => {
			forgejo_provider::publish_release_pull_request(
				source,
				root,
				request,
				tracked_paths,
				no_verify,
				stage_all,
			)
			.await
		}
		#[cfg(not(any(
			feature = "github",
			feature = "gitlab",
			feature = "gitea",
			feature = "forgejo"
		)))]
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
		value
			.as_object_mut()
			.unwrap_or_else(|| panic!("release json wrapper must stay object"))
			.insert(
				"fileDiffs".to_string(),
				serde_json::to_value(sections.file_diffs).unwrap_or_default(),
			);
	}
	serde_json::to_string_pretty(&value)
		.map_err(|error| MonochangeError::Discovery(error.to_string()))
}

pub(crate) fn write_release_record_file(
	root: &Path,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> MonochangeResult<PathBuf> {
	let paths = ReleasePaths::from_manifest(root, manifest);

	// If the record already exists, return it without overwriting so that
	// subsequent PrepareRelease steps (for example during `mc release-pr`)
	// do not produce a dirty working tree.
	if paths.absolute.is_file() {
		add_to_dedup_index(root, &paths.hash).ok();
		return Ok(paths.absolute);
	}

	let record = build_release_record(source, manifest);
	deduplicate_overlapping_release_records(
		root,
		&record.release_targets,
		paths.absolute.parent().unwrap_or(root),
	)?;
	let json = serde_json::to_string_pretty(&record).unwrap_or_default();
	fs::create_dir_all(paths.absolute.parent().unwrap_or(root))
		.map_err(|error| MonochangeError::Io(format!("create release record dir: {error}")))?;
	fs::write(&paths.absolute, json)
		.map_err(|error| MonochangeError::Io(format!("write release record: {error}")))?;
	add_to_dedup_index(root, &paths.hash)?;
	Ok(paths.absolute)
}

/// Compare two JSON strings for semantic equality.
/// If the strings are byte-equal, they match immediately.
/// Otherwise, both are parsed into `serde_json::Value` and compared structurally.
fn compare_json_strings(json1: &str, json2: &str) -> bool {
	if json1 == json2 {
		return true;
	}

	let parsed1: Result<serde_json::Value, _> = serde_json::from_str(json1);
	let parsed2: Result<serde_json::Value, _> = serde_json::from_str(json2);

	match (parsed1, parsed2) {
		(Ok(v1), Ok(v2)) => v1 == v2,
		_ => false,
	}
}

/// Validate that the release record file expected for `manifest` still exists
/// on disk after re-running deduplication. Called by `commit_release` to
/// guard against stale or missing records between `prepare_release` and
/// `commit_release`.
///
/// Performance note: when the file already exists and its `release_targets`
/// match the manifest, this function skips rebuilding the `ReleaseRecord`
/// entirely. It only falls back to `build_release_record` when the file is
/// missing or the targets differ, avoiding the JSON round-trip on the hot
/// path.
pub(crate) fn validate_release_record_file(
	root: &Path,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
	update_release_json: bool,
) -> MonochangeResult<PathBuf> {
	// Compute the expected path from the manifest without building the record.
	let paths = ReleasePaths::from_manifest(root, manifest);

	// Fast path: if the file exists, verify its release_targets identity
	// without rebuilding the entire ReleaseRecord.
	if paths.absolute.is_file() {
		match fs::read_to_string(&paths.absolute) {
			Ok(existing_json) => {
				if let Ok(existing_value) =
					serde_json::from_str::<serde_json::Value>(&existing_json)
					&& let Some(existing_targets) = existing_value
						.get("releaseTargets")
						.and_then(|v| v.as_array())
				{
					let manifest_targets = &manifest.release_targets;
					if existing_targets.len() == manifest_targets.len() {
						let all_match = manifest_targets.iter().all(|mt| {
							existing_targets.iter().any(|et| {
								let Some(id) = et.get("id").and_then(|v| v.as_str()) else {
									return false;
								};
								let Some(kind) = et.get("kind").and_then(|v| v.as_str()) else {
									return false;
								};
								let Some(version) = et.get("version").and_then(|v| v.as_str())
								else {
									return false;
								};
								mt.id == id && mt.kind.as_str() == kind && mt.version == version
							})
						});
						if all_match {
							// Targets match — no need to rebuild or rewrite.
							add_to_dedup_index(root, &paths.hash).ok();
							return Ok(paths.absolute);
						}
					}
				}
			}
			Err(error) => {
				return Err(MonochangeError::Io(format!("read release record: {error}")));
			}
		}
	}

	// Slow path: rebuild the record, deduplicate, and validate or rewrite.
	let record = build_release_record(source, manifest);
	deduplicate_overlapping_release_records(
		root,
		&record.release_targets,
		paths.absolute.parent().unwrap_or(root),
	)?;
	let json = serde_json::to_string_pretty(&record).unwrap_or_default();
	if paths.absolute.is_file() {
		let existing = fs::read_to_string(&paths.absolute)
			.map_err(|error| MonochangeError::Io(format!("read release record: {error}")))?;
		if !compare_json_strings(&existing, &json) {
			if update_release_json {
				fs::write(&paths.absolute, json).map_err(|error| {
					MonochangeError::Io(format!("update release record: {error}"))
				})?;
			} else {
				return Err(MonochangeError::Io(format!(
					"release record at {} does not match expected content — the file has been modified since it was prepared. Set `update_release_json = true` on the CommitRelease step to allow overwriting.",
					paths.absolute.display()
				)));
			}
		}
	} else if update_release_json {
		fs::create_dir_all(paths.absolute.parent().unwrap_or(root))
			.map_err(|error| MonochangeError::Io(format!("create release record dir: {error}")))?;
		fs::write(&paths.absolute, json)
			.map_err(|error| MonochangeError::Io(format!("write release record: {error}")))?;
	} else {
		return Err(MonochangeError::Io(format!(
			"no release record found at {} — was it removed by deduplication or never written?",
			paths.absolute.display()
		)));
	}
	Ok(paths.absolute)
}

/// Derived filesystem paths for a release record.
///
/// The record path is a deterministic function of the manifest's
/// `release_targets`. It is computed on demand rather than stored in the
/// manifest so that the manifest remains portable and the path format can
/// evolve without invalidating cached manifests.
#[allow(dead_code)]
pub(crate) struct ReleasePaths {
	/// Hexadecimal hash derived from the release targets.
	pub hash: String,
	/// Path relative to the workspace root (`.monochange/releases/<hash>/release.json`).
	pub relative: PathBuf,
	/// Absolute path resolved against the workspace root.
	pub absolute: PathBuf,
}

impl ReleasePaths {
	/// Compute paths from an already-built `ReleaseRecord`.
	///
	/// Use this when you have the record in hand to avoid rebuilding it.
	#[allow(dead_code)]
	pub fn from_record(root: &Path, record: &ReleaseRecord) -> Self {
		let hash = release_targets_hash(&record.release_targets);
		let relative = PathBuf::from(".monochange/releases")
			.join(&hash)
			.join("release.json");
		let absolute = root.join(&relative);
		Self {
			hash,
			relative,
			absolute,
		}
	}

	/// Compute paths directly from a `ReleaseManifest`.
	///
	/// This builds the intermediate `ReleaseRecord` internally, so prefer
	/// `from_record` when the record is already available.
	/// Compute paths directly from a `ReleaseManifest`.
	///
	/// Unlike `from_record`, this does **not** build the intermediate
	/// `ReleaseRecord`. The hash is derived from `manifest.release_targets`
	/// directly so callers can check file existence before doing expensive work.
	pub fn from_manifest(root: &Path, manifest: &ReleaseManifest) -> Self {
		let hash = release_targets_hash(&manifest.release_targets);
		let relative = PathBuf::from(".monochange/releases")
			.join(&hash)
			.join("release.json");
		let absolute = root.join(&relative);
		Self {
			hash,
			relative,
			absolute,
		}
	}
}

/// Identity-aware hash for a slice of release targets.
///
/// The hash is deterministic: targets are sorted by `(id, kind, version)`
/// before hashing so that manifest order never affects the path.
///
/// Fields included in the hash: `id`, `kind`, `version`.
/// Excluded: `tag`, `release`, `tag_name`, `version_format`, `members`.
fn release_targets_hash<T: ReleaseTargetIdentity>(release_targets: &[T]) -> String {
	use std::collections::hash_map::DefaultHasher;
	use std::hash::Hasher;
	let mut hasher = DefaultHasher::new();
	let mut sorted: Vec<&T> = release_targets.iter().collect();
	sorted.sort_by(|a, b| {
		a.id()
			.cmp(b.id())
			.then_with(|| a.kind().as_str().cmp(b.kind().as_str()))
			.then_with(|| a.version().cmp(b.version()))
	});
	for target in sorted {
		hasher.write(target.id().as_bytes());
		hasher.write(target.kind().as_str().as_bytes());
		hasher.write(target.version().as_bytes());
	}
	format!("{:016x}", hasher.finish())
}

/// Trait exposing the identity fields that participate in the release-target
/// hash. Implemented for both `ReleaseManifestTarget` and `ReleaseRecordTarget`
/// so that `release_targets_hash` can work with either slice type.
trait ReleaseTargetIdentity {
	fn id(&self) -> &str;
	fn kind(&self) -> ReleaseOwnerKind;
	fn version(&self) -> &str;
}

impl ReleaseTargetIdentity for ReleaseManifestTarget {
	fn id(&self) -> &str {
		&self.id
	}

	fn kind(&self) -> ReleaseOwnerKind {
		self.kind
	}

	fn version(&self) -> &str {
		&self.version
	}
}

impl ReleaseTargetIdentity for ReleaseRecordTarget {
	fn id(&self) -> &str {
		&self.id
	}

	fn kind(&self) -> ReleaseOwnerKind {
		self.kind
	}

	fn version(&self) -> &str {
		&self.version
	}
}

fn deduplicate_overlapping_release_records(
	root: &Path,
	release_targets: &[ReleaseRecordTarget],
	current_record_dir: &Path,
) -> MonochangeResult<()> {
	let hash = release_targets_hash(release_targets);
	let already_deduped = DEDUPLICATED_CACHE
		.with(|cache| cache.borrow().contains(&(root.to_path_buf(), hash.clone())));
	if already_deduped {
		return Ok(());
	}

	let persistent_index = load_dedup_index(root);
	if persistent_index.contains(&hash) {
		DEDUPLICATED_CACHE.with(|cache| {
			cache
				.borrow_mut()
				.insert((root.to_path_buf(), hash.clone()));
		});
		return Ok(());
	}

	let new_tags: std::collections::HashSet<(&str, &str)> = release_targets
		.iter()
		.map(|target| (target.id.as_str(), target.version.as_str()))
		.collect();

	let releases_dir = root.join(".monochange/releases");
	if !releases_dir.is_dir() {
		return Ok(());
	}
	for entry in fs::read_dir(&releases_dir)
		.map_err(|error| MonochangeError::Io(format!("read releases dir: {error}")))?
	{
		let entry =
			entry.map_err(|error| MonochangeError::Io(format!("read dir entry: {error}")))?;
		let path = entry.path();
		if path == current_record_dir {
			continue;
		}
		if !path.is_dir() {
			continue;
		}
		let record_file = path.join("release.json");
		if !record_file.is_file() {
			continue;
		}
		let Ok(content) = fs::read_to_string(&record_file) else {
			continue;
		};
		let Ok(existing) = serde_json::from_str::<ReleaseRecord>(&content) else {
			continue;
		};
		let has_overlap = existing
			.release_targets
			.iter()
			.any(|t| new_tags.contains(&(t.id.as_str(), t.version.as_str())));
		if has_overlap {
			let hash_to_remove = release_targets_hash(&existing.release_targets);
			fs::remove_dir_all(&path).map_err(|error| {
				MonochangeError::Io(format!("remove stale release record dir: {error}"))
			})?;
			remove_from_dedup_index(root, &hash_to_remove).ok();
		}
	}

	DEDUPLICATED_CACHE.with(|cache| {
		cache
			.borrow_mut()
			.insert((root.to_path_buf(), hash.clone()));
	});
	add_to_dedup_index(root, &hash)?;

	Ok(())
}

struct PreparedReleaseCommit {
	message: CommitMessage,
	tracked_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct HostedCommitOptions {
	pub(crate) auth: monochange_core::HostedCommitAuth,
	pub(crate) url: Option<String>,
	pub(crate) oidc_audience: Option<String>,
}

#[derive(Serialize)]
struct HostedCommitRequest {
	provider: &'static str,
	owner: String,
	repository: String,
	branch: String,
	base_commit: String,
	subject: String,
	body: String,
	files: Vec<HostedCommitFile>,
	dry_run: bool,
}

#[derive(Serialize)]
struct HostedCommitFile {
	path: String,
	content: Option<String>,
}

#[derive(Deserialize)]
struct HostedCommitResponse {
	commit: Option<String>,
	status: Option<String>,
}

fn prepare_release_commit(
	root: &Path,
	context: &CliContext,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
	update_release_json: bool,
) -> MonochangeResult<PreparedReleaseCommit> {
	let tracked_paths = tracked_release_pull_request_paths(context, manifest);
	let message = build_release_commit_message(source, manifest);
	let release_record_path =
		validate_release_record_file(root, source, manifest, update_release_json)?;
	let mut tracked_paths = tracked_paths;
	tracked_paths.push(release_record_path);
	tracked_paths.sort();
	tracked_paths.dedup();
	Ok(PreparedReleaseCommit {
		message,
		tracked_paths,
	})
}

pub(crate) async fn commit_release(
	root: &Path,
	context: &CliContext,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
	no_verify: bool,
	update_release_json: bool,
	stage_all: bool,
) -> MonochangeResult<CommitReleaseReport> {
	let prepared = prepare_release_commit(root, context, source, manifest, update_release_json)?;
	if !context.dry_run {
		// patch-coverage:ignore-start -- exercised by end-to-end release PR flows; branch delegates to covered git helpers.
		if stage_all {
			git_stage_all(root).await?;
		} else {
			git_stage_paths(root, &prepared.tracked_paths).await?;
		}
		// patch-coverage:ignore-end
		git_commit_paths(root, &prepared.message, no_verify).await?;
	}
	Ok(CommitReleaseReport {
		subject: prepared.message.subject,
		body: prepared.message.body.unwrap_or_default(),
		commit: if context.dry_run {
			None
		} else {
			Some(git_head_commit(root).await?)
		},
		tracked_paths: prepared.tracked_paths,
		dry_run: context.dry_run,
		status: if context.dry_run {
			"dry_run".to_string()
		} else {
			"completed".to_string()
		},
	})
}

pub(crate) async fn hosted_commit_release(
	root: &Path,
	context: &CliContext,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
	update_release_json: bool,
	options: &HostedCommitOptions,
) -> MonochangeResult<CommitReleaseReport> {
	let prepared = prepare_release_commit(root, context, source, manifest, update_release_json)?;
	let body = prepared.message.body.clone().unwrap_or_default();
	let request = build_hosted_commit_request(root, &prepared, context.dry_run).await?;
	let response = if context.dry_run {
		HostedCommitResponse {
			commit: None,
			status: Some("dry_run".to_string()),
		}
	} else {
		let options = options.clone();
		tokio::task::spawn_blocking(move || send_hosted_commit_request(&request, &options))
			.await
			.unwrap_or_else(|error| panic!("hosted CommitRelease request task panicked: {error}"))?
	};
	Ok(CommitReleaseReport {
		subject: prepared.message.subject,
		body,
		commit: response.commit,
		tracked_paths: prepared.tracked_paths,
		dry_run: context.dry_run,
		status: response.status.unwrap_or_else(|| "completed".to_string()),
	})
}

async fn build_hosted_commit_request(
	root: &Path,
	prepared: &PreparedReleaseCommit,
	dry_run: bool,
) -> MonochangeResult<HostedCommitRequest> {
	let repository = std::env::var("GITHUB_REPOSITORY").map_err(|_| {
		MonochangeError::Config(
			"hosted CommitRelease requires GITHUB_REPOSITORY (for example `owner/repo`)"
				.to_string(),
		)
	})?;
	let head_ref = std::env::var("GITHUB_HEAD_REF").ok();
	let ref_name = std::env::var("GITHUB_REF_NAME").ok();
	build_hosted_commit_request_for_github(
		root,
		prepared,
		dry_run,
		&repository,
		head_ref.as_deref(),
		ref_name.as_deref(),
	)
	.await
}

async fn build_hosted_commit_request_for_github(
	root: &Path,
	prepared: &PreparedReleaseCommit,
	dry_run: bool,
	repository: &str,
	head_ref: Option<&str>,
	ref_name: Option<&str>,
) -> MonochangeResult<HostedCommitRequest> {
	let (owner, repository) = repository.split_once('/').ok_or_else(|| {
		MonochangeError::Config("GITHUB_REPOSITORY must use `owner/repo` format".to_string())
	})?;
	let branch = head_ref
		.filter(|value| !value.is_empty())
		.or(ref_name)
		.filter(|value| !value.is_empty())
		.map_or_else(
			|| git_current_branch(root).unwrap_or_else(|_| "HEAD".to_string()),
			ToString::to_string,
		);
	let files = prepared
		.tracked_paths
		.iter()
		.map(|path| hosted_commit_file(root, path))
		.collect::<MonochangeResult<Vec<_>>>()?;
	Ok(HostedCommitRequest {
		provider: "github",
		owner: owner.to_string(),
		repository: repository.to_string(),
		branch,
		base_commit: git_head_commit(root).await?,
		subject: prepared.message.subject.clone(),
		body: prepared.message.body.clone().unwrap_or_default(),
		files,
		dry_run,
	})
}

fn hosted_commit_file(root: &Path, path: &Path) -> MonochangeResult<HostedCommitFile> {
	let full_path = root.join(path);
	let content = if full_path.exists() {
		Some(fs::read_to_string(&full_path).map_err(|error| {
			MonochangeError::Io(format!(
				"read hosted commit file `{}`: {error}",
				path.display()
			))
		})?)
	} else {
		None
	};
	Ok(HostedCommitFile {
		path: path.to_string_lossy().replace('\\', "/"),
		content,
	})
}

fn send_hosted_commit_request(
	request: &HostedCommitRequest,
	options: &HostedCommitOptions,
) -> MonochangeResult<HostedCommitResponse> {
	let base_url = options
		.url
		.as_deref()
		.unwrap_or("https://monochange.dev")
		.trim_end_matches('/');
	let token = hosted_commit_bearer_token(options)?;
	let response = reqwest::blocking::Client::new()
		.post(format!("{base_url}/api/release-commits"))
		.bearer_auth(token)
		.json(&request)
		.send()
		.map_err(|error| {
			MonochangeError::Config(format!("hosted CommitRelease request failed: {error}"))
		})?;
	let status = response.status();
	#[rustfmt::skip]
	let text = response.text().map_err(|error| MonochangeError::Config(format!("hosted CommitRelease response read failed: {error}")))?;
	if !status.is_success() {
		return Err(MonochangeError::Config(format!(
			"hosted CommitRelease failed with HTTP {status}: {text}"
		)));
	}
	serde_json::from_str(&text).map_err(|error| {
		MonochangeError::Config(format!(
			"hosted CommitRelease response was invalid JSON: {error}"
		))
	})
}

fn hosted_commit_bearer_token(options: &HostedCommitOptions) -> MonochangeResult<String> {
	match options.auth {
		monochange_core::HostedCommitAuth::Token => monochange_token(),
		monochange_core::HostedCommitAuth::Oidc => github_actions_oidc_token(options),
		monochange_core::HostedCommitAuth::Auto => {
			if std::env::var_os("ACTIONS_ID_TOKEN_REQUEST_URL").is_some() {
				github_actions_oidc_token(options)
			} else {
				monochange_token()
			}
		}
	}
}

fn monochange_token() -> MonochangeResult<String> {
	std::env::var("MONOCHANGE_TOKEN").map_err(|_| {
		MonochangeError::Config(
			"hosted CommitRelease token auth requires MONOCHANGE_TOKEN".to_string(),
		)
	})
}

#[derive(Deserialize)]
struct GithubActionsOidcResponse {
	value: String,
}

fn github_actions_oidc_token(options: &HostedCommitOptions) -> MonochangeResult<String> {
	let request_url = std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL").map_err(|_| {
		MonochangeError::Config(
			"hosted CommitRelease OIDC auth requires ACTIONS_ID_TOKEN_REQUEST_URL".to_string(),
		)
	})?;
	let request_token = std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN").map_err(|_| {
		MonochangeError::Config(
			"hosted CommitRelease OIDC auth requires ACTIONS_ID_TOKEN_REQUEST_TOKEN".to_string(),
		)
	})?;
	let audience = options.oidc_audience.as_deref().unwrap_or("monochange.dev");
	let separator = if request_url.contains('?') { '&' } else { '?' };
	let url = format!("{request_url}{separator}audience={audience}");
	#[rustfmt::skip]
	let response = reqwest::blocking::Client::new().get(url).bearer_auth(request_token).send().map_err(|error| MonochangeError::Config(format!("GitHub Actions OIDC request failed: {error}")))?;
	let status = response.status();
	#[rustfmt::skip]
	let text = response.text().map_err(|error| MonochangeError::Config(format!("GitHub Actions OIDC response read failed: {error}")))?;
	if !status.is_success() {
		return Err(MonochangeError::Config(format!(
			"GitHub Actions OIDC request failed with HTTP {status}: {text}"
		)));
	}
	let token: GithubActionsOidcResponse = serde_json::from_str(&text).map_err(|error| {
		MonochangeError::Config(format!(
			"GitHub Actions OIDC response was invalid JSON: {error}"
		))
	})?;
	Ok(token.value)
}

fn git_current_branch(root: &Path) -> MonochangeResult<String> {
	let output = ProcessCommand::new("git")
		.current_dir(root)
		.args(["rev-parse", "--abbrev-ref", "HEAD"])
		.output()
		.map_err(|error| {
			MonochangeError::Config(format!("failed to resolve current branch: {error}"))
		})?;
	if !output.status.success() {
		return Err(MonochangeError::Config(
			"failed to resolve current branch".to_string(),
		));
	}
	Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
#[path = "__tests__/release_artifacts_tests.rs"]
mod tests;
