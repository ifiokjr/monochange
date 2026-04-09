use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_cargo::discover_cargo_packages;
use monochange_config::apply_version_groups;
use monochange_config::load_change_signals;
use monochange_config::load_changeset_file;
use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::PackageType;
use monochange_core::ReleasePlan;
use monochange_core::SourceProvider;
use monochange_dart::discover_dart_packages;
use monochange_deno::discover_deno_packages;
use monochange_github as github_provider;
use monochange_npm::discover_npm_packages;
use serde_json::json;
use typed_builder::TypedBuilder;

use crate::interactive;
use crate::*;

pub(crate) fn init_workspace(root: &Path, force: bool) -> MonochangeResult<PathBuf> {
	let path = monochange_config::config_path(root);
	if path.exists() && !force {
		return Err(MonochangeError::Config(format!(
			"{} already exists; rerun with --force to overwrite it",
			path.display()
		)));
	}

	let content = render_annotated_init_config(root)?;
	fs::write(&path, content).map_err(|error| {
		MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
	})?;
	Ok(path)
}

/// The minijinja template for `mc init`, loaded at compile time.
///
/// SYNC: when configuration options are added, removed, or changed in
/// `monochange_core` or `monochange_config`, update `monochange.init.toml`
/// to document the new options.  See the `product-rules.md` agent rule
/// "keep init template in sync".
const INIT_TEMPLATE: &str = include_str!("monochange.init.toml");

/// Render a fully annotated `monochange.toml` from the init template with
/// discovered packages injected as context.
fn render_annotated_init_config(root: &Path) -> MonochangeResult<String> {
	let packages = discover_packages(root)?;
	let mut template_packages = Vec::new();
	let mut package_ids = Vec::<String>::new();
	let mut name_counts = BTreeMap::<String, usize>::new();

	for package in &packages {
		let count = name_counts.entry(package.name.clone()).or_default();
		*count += 1;
		let id = if *count == 1 {
			package.name.clone()
		} else {
			format!("{}-{}", package.name, package.ecosystem.as_str())
		};
		package_ids.push(id.clone());
		let manifest_dir = package.manifest_path.parent().unwrap_or(root).to_path_buf();
		let relative_dir = root_relative(root, &manifest_dir);
		let pkg_type = package_type_for_ecosystem(package.ecosystem);
		let changelog = detect_default_changelog(root, &manifest_dir);
		let type_str = match pkg_type {
			PackageType::Cargo => "cargo",
			PackageType::Npm => "npm",
			PackageType::Deno => "deno",
			PackageType::Dart => "dart",
			PackageType::Flutter => "flutter",
		};
		let mut entry = BTreeMap::new();
		entry.insert("id", json!(id));
		entry.insert("path", json!(relative_dir.display().to_string()));
		entry.insert("type", json!(type_str));
		if let Some(cl) = changelog {
			entry.insert("changelog", json!(cl.display().to_string()));
		}
		template_packages.push(json!(entry));
	}

	let has_cargo = packages.iter().any(|p| p.ecosystem == Ecosystem::Cargo);
	let has_npm = packages.iter().any(|p| p.ecosystem == Ecosystem::Npm);
	let has_deno = packages.iter().any(|p| p.ecosystem == Ecosystem::Deno);
	let has_dart = packages
		.iter()
		.any(|p| p.ecosystem == Ecosystem::Dart || p.ecosystem == Ecosystem::Flutter);

	let package_ids_toml = package_ids
		.iter()
		.map(|id| format!("\"{id}\""))
		.collect::<Vec<_>>()
		.join(", ");

	let context = json!({
		"packages": template_packages,
		"has_group": package_ids.len() > 1,
		"package_ids_toml": package_ids_toml,
		"has_cargo": has_cargo,
		"has_npm": has_npm,
		"has_deno": has_deno,
		"has_dart": has_dart,
	});

	let jinja_context = minijinja::Value::from_serialize(&context);
	let rendered = render_jinja_template(INIT_TEMPLATE, &jinja_context)?;

	// Collapse runs of 3+ blank lines down to 2 (one visual blank line)
	let mut collapsed = String::with_capacity(rendered.len());
	let mut consecutive_blanks = 0u32;
	for line in rendered.lines() {
		if line.trim().is_empty() {
			consecutive_blanks += 1;
			if consecutive_blanks <= 2 {
				collapsed.push('\n');
			}
		} else {
			consecutive_blanks = 0;
			collapsed.push_str(line);
			collapsed.push('\n');
		}
	}

	Ok(collapsed.trim_start().to_string())
}

fn discover_packages(root: &Path) -> MonochangeResult<Vec<PackageRecord>> {
	let mut packages = Vec::new();
	for discovery in [
		discover_cargo_packages(root)?,
		discover_npm_packages(root)?,
		discover_deno_packages(root)?,
		discover_dart_packages(root)?,
	] {
		packages.extend(discovery.packages);
	}
	normalize_package_ids(root, &mut packages);
	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);
	Ok(packages)
}

fn normalize_package_ids(root: &Path, packages: &mut [PackageRecord]) {
	for package in packages {
		if let Some(relative_manifest) = relative_to_root(root, &package.manifest_path) {
			package.id = format!(
				"{}:{}",
				package.ecosystem.as_str(),
				relative_manifest.display()
			);
		}
	}
}

fn detect_default_changelog(root: &Path, manifest_dir: &Path) -> Option<PathBuf> {
	for candidate in [
		manifest_dir.join("CHANGELOG.md"),
		manifest_dir.join("changelog.md"),
	] {
		if candidate.exists() {
			return Some(root_relative(root, &candidate));
		}
	}
	None
}

fn package_type_for_ecosystem(ecosystem: Ecosystem) -> PackageType {
	match ecosystem {
		Ecosystem::Cargo => PackageType::Cargo,
		Ecosystem::Npm => PackageType::Npm,
		Ecosystem::Deno => PackageType::Deno,
		Ecosystem::Dart => PackageType::Dart,
		Ecosystem::Flutter => PackageType::Flutter,
	}
}

pub(crate) fn validate_cargo_workspace_version_groups(root: &Path) -> MonochangeResult<()> {
	let configuration = load_workspace_configuration(root)?;
	if configuration.packages.is_empty() {
		return Ok(());
	}

	let mut packages = discover_cargo_packages(root)?.packages;
	if packages.is_empty() {
		return Ok(());
	}

	apply_version_groups(&mut packages, &configuration)?;
	monochange_cargo::validate_workspace_version_groups(&packages)
}

pub fn discover_workspace(root: &Path) -> MonochangeResult<DiscoveryReport> {
	let configuration = load_workspace_configuration(root)?;
	let mut warnings = Vec::new();
	let mut packages = Vec::new();

	for discovery in [
		discover_cargo_packages(root)?,
		discover_npm_packages(root)?,
		discover_deno_packages(root)?,
		discover_dart_packages(root)?,
	] {
		warnings.extend(discovery.warnings);
		packages.extend(discovery.packages);
	}

	normalize_package_ids(root, &mut packages);
	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	let (version_groups, version_group_warnings) =
		apply_version_groups(&mut packages, &configuration)?;
	warnings.extend(version_group_warnings);
	let dependencies = materialize_dependency_edges(&packages);

	Ok(DiscoveryReport {
		workspace_root: root.to_path_buf(),
		packages,
		dependencies,
		version_groups,
		warnings,
	})
}

#[derive(Clone, Copy, Debug, TypedBuilder)]
pub struct AddChangeFileRequest<'a> {
	pub package_refs: &'a [String],
	pub bump: BumpSeverity,
	pub reason: &'a str,
	#[builder(default)]
	pub version: Option<&'a str>,
	#[builder(default)]
	pub change_type: Option<&'a str>,
	#[builder(default)]
	pub details: Option<&'a str>,
	#[builder(default)]
	pub output: Option<&'a Path>,
}

pub fn add_change_file(
	root: &Path,
	request: AddChangeFileRequest<'_>,
) -> MonochangeResult<PathBuf> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;
	let packages = canonical_change_packages(
		root,
		request.package_refs,
		&configuration,
		&discovery.packages,
	)?;
	let output_path = request
		.output
		.map_or_else(|| default_change_path(root, &packages), Path::to_path_buf);
	if let Some(parent) = output_path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
	}

	if let Some(version) = request.version {
		semver::Version::parse(version).map_err(|error| {
			MonochangeError::Config(format!(
				"invalid explicit version `{version}` passed to `change`: {error}"
			))
		})?;
	}

	let content = render_changeset_markdown(
		&configuration,
		&packages,
		request.bump,
		request.version,
		request.reason,
		request.change_type,
		request.details,
	)?;
	fs::write(&output_path, content).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write {}: {error}",
			output_path.display()
		))
	})?;
	Ok(output_path)
}

pub(crate) fn add_interactive_change_file(
	root: &Path,
	result: &interactive::InteractiveChangeResult,
	output: Option<&Path>,
) -> MonochangeResult<PathBuf> {
	let package_refs = result
		.targets
		.iter()
		.map(|target| target.id.clone())
		.collect::<Vec<_>>();
	let output_path = output.map_or_else(
		|| default_change_path(root, &package_refs),
		Path::to_path_buf,
	);
	if let Some(parent) = output_path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
	}

	let configuration = load_workspace_configuration(root)?;
	let content = render_interactive_changeset_markdown(&configuration, result)?;
	fs::write(&output_path, content).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write {}: {error}",
			output_path.display()
		))
	})?;
	Ok(output_path)
}

pub(crate) fn change_type_default_bump(
	configuration: &monochange_core::WorkspaceConfiguration,
	target_id: &str,
	change_type: &str,
) -> Option<BumpSeverity> {
	let sections = configuration
		.package_by_id(target_id)
		.map(|package| package.extra_changelog_sections.as_slice())
		.or_else(|| {
			configuration
				.group_by_id(target_id)
				.map(|group| group.extra_changelog_sections.as_slice())
		})?;
	sections.iter().find_map(|section| {
		section
			.types
			.iter()
			.any(|candidate| candidate.trim() == change_type)
			.then_some(section.default_bump.unwrap_or(BumpSeverity::None))
	})
}

pub(crate) fn render_change_target_markdown(
	configuration: &monochange_core::WorkspaceConfiguration,
	target_id: &str,
	bump: BumpSeverity,
	version: Option<&str>,
	change_type: Option<&str>,
) -> MonochangeResult<Vec<String>> {
	if change_type.is_none() && version.is_none() && bump == BumpSeverity::None {
		return Err(MonochangeError::Config(format!(
			"target `{target_id}` must not use a `none` bump without also declaring `type` or `version`"
		)));
	}
	let mut lines = Vec::new();
	if let Some(change_type) = change_type.filter(|value| !value.trim().is_empty()) {
		let default_bump = change_type_default_bump(configuration, target_id, change_type)
			.ok_or_else(|| {
				MonochangeError::Config(format!(
					"target `{target_id}` uses unknown change type `{change_type}`"
				))
			})?;
		if version.is_none() && bump == default_bump {
			lines.push(format!("{target_id}: {change_type}"));
			return Ok(lines);
		}
		lines.push(format!("{target_id}:"));
		if bump != BumpSeverity::None {
			lines.push(format!("  bump: {bump}"));
		}
		lines.push(format!("  type: {change_type}"));
		if let Some(version) = version {
			lines.push(format!("  version: \"{version}\""));
		}
		return Ok(lines);
	}
	if let Some(version) = version {
		lines.push(format!("{target_id}:"));
		if bump != BumpSeverity::None {
			lines.push(format!("  bump: {bump}"));
		}
		lines.push(format!("  version: \"{version}\""));
		return Ok(lines);
	}
	lines.push(format!("{target_id}: {bump}"));
	Ok(lines)
}

pub(crate) fn render_interactive_changeset_markdown(
	configuration: &monochange_core::WorkspaceConfiguration,
	result: &interactive::InteractiveChangeResult,
) -> MonochangeResult<String> {
	let mut lines = vec!["---".to_string()];
	for target in &result.targets {
		let id = &target.id;
		let version = target.version.as_deref();
		let change_type = target.change_type.as_deref();
		let target_lines =
			render_change_target_markdown(configuration, id, target.bump, version, change_type)?;
		lines.extend(target_lines);
	}
	lines.push("---".to_string());
	lines.push(String::new());
	lines.push(format!("# {}", result.reason));
	if let Some(details) = result
		.details
		.as_deref()
		.filter(|value| !value.trim().is_empty())
	{
		lines.push(String::new());
		lines.push(details.trim().to_string());
	}
	lines.push(String::new());
	Ok(lines.join("\n"))
}

pub fn plan_release(root: &Path, changes_path: &Path) -> MonochangeResult<ReleasePlan> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;
	let change_signals = load_change_signals(changes_path, &configuration, &discovery.packages)?;
	build_release_plan_from_signals(&configuration, &discovery, &change_signals)
}

pub fn prepare_release(root: &Path, dry_run: bool) -> MonochangeResult<PreparedRelease> {
	prepare_release_execution(root, dry_run).map(|execution| execution.prepared_release)
}

pub(crate) fn prepare_release_execution(
	root: &Path,
	dry_run: bool,
) -> MonochangeResult<PreparedReleaseExecution> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;
	let changeset_paths = discover_changeset_paths(root)?;
	let loaded_changesets = changeset_paths
		.iter()
		.map(|path| load_changeset_file(path, &configuration, &discovery.packages))
		.collect::<MonochangeResult<Vec<_>>>()?;
	let change_signals = loaded_changesets
		.iter()
		.flat_map(|changeset| changeset.signals.clone())
		.collect::<Vec<_>>();
	let mut changesets = build_prepared_changesets(root, &loaded_changesets);
	if let Some(source) = configuration
		.source
		.as_ref()
		.filter(|source| source.provider == SourceProvider::GitHub)
	{
		github_provider::enrich_changeset_context(source, &mut changesets);
	}
	let plan = build_release_plan_from_signals(&configuration, &discovery, &change_signals)?;
	let released_packages = released_package_names(&discovery.packages, &plan);
	if released_packages.is_empty() {
		return Err(MonochangeError::Config(
			"no releaseable packages were found in discovered changesets".to_string(),
		));
	}

	let changelog_targets = resolve_changelog_targets(&configuration, &discovery.packages)?;
	let cargo_updates = build_cargo_manifest_updates(&discovery.packages, &plan)?;
	let npm_updates = build_npm_manifest_updates(&discovery.packages, &plan)?;
	let dart_updates = build_dart_manifest_updates(&discovery.packages, &plan)?;
	let manifest_updates = [cargo_updates, npm_updates, dart_updates].concat();
	let versioned_file_updates =
		build_versioned_file_updates(root, &configuration, &discovery.packages, &plan)?;
	let release_targets =
		build_release_targets(&configuration, &discovery.packages, &plan, &changeset_paths);
	let changelog_updates = build_changelog_updates(
		ChangelogBuildContext::builder()
			.root(root)
			.configuration(&configuration)
			.packages(&discovery.packages)
			.plan(&plan)
			.change_signals(&change_signals)
			.changesets(&changesets)
			.changelog_targets(&changelog_targets)
			.release_targets(&release_targets)
			.build(),
	)?;
	let mut changed_files = manifest_updates
		.iter()
		.map(|update| root_relative(root, &update.path))
		.collect::<Vec<_>>();
	changed_files.extend(
		versioned_file_updates
			.iter()
			.map(|update| root_relative(root, &update.path)),
	);
	changed_files.extend(
		changelog_updates
			.iter()
			.map(|update| root_relative(root, &update.file.path)),
	);
	changed_files.sort();
	changed_files.dedup();
	let changelogs = changelog_updates
		.iter()
		.map(|update| PreparedChangelog {
			owner_id: update.owner_id.clone(),
			owner_kind: update.owner_kind,
			path: root_relative(root, &update.file.path),
			format: update.format,
			notes: update.notes.clone(),
			rendered: update.rendered.clone(),
		})
		.collect::<Vec<_>>();
	let updated_changelogs = changelogs
		.iter()
		.map(|update| update.path.clone())
		.collect::<Vec<_>>();
	let changelog_file_updates = changelog_updates
		.iter()
		.map(|update| update.file.clone())
		.collect::<Vec<_>>();
	let file_diffs = build_file_diff_previews(
		root,
		&[
			manifest_updates.clone(),
			versioned_file_updates.clone(),
			changelog_file_updates.clone(),
		]
		.concat(),
	)?;

	let version = shared_release_version(&plan);
	let group_version = shared_group_version(&plan);
	let mut deleted_changesets = Vec::new();
	if !dry_run {
		apply_file_updates(&manifest_updates)?;
		apply_file_updates(&versioned_file_updates)?;
		apply_file_updates(&changelog_file_updates)?;
		for path in &changeset_paths {
			fs::remove_file(path).map_err(|error| {
				MonochangeError::Io(format!("failed to delete {}: {error}", path.display()))
			})?;
			deleted_changesets.push(root_relative(root, path));
		}
	}

	Ok(PreparedReleaseExecution {
		prepared_release: PreparedRelease {
			plan,
			changeset_paths,
			changesets,
			released_packages,
			version,
			group_version,
			release_targets,
			changed_files,
			changelogs,
			updated_changelogs,
			deleted_changesets,
			dry_run,
		},
		file_diffs,
	})
}
