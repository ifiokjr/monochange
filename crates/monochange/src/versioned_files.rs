use regex::Regex;

use super::*;

pub(crate) struct VersionedFileUpdateContext<'a> {
	pub(crate) package_by_config_id: BTreeMap<&'a str, &'a PackageRecord>,
	pub(crate) package_by_native_name: BTreeMap<&'a str, &'a PackageRecord>,
	pub(crate) current_versions_by_native_name: BTreeMap<String, String>,
	pub(crate) released_versions_by_native_name: BTreeMap<String, String>,
	pub(crate) configuration: &'a monochange_core::WorkspaceConfiguration,
}

#[derive(Debug)]
pub(crate) enum CachedDocument {
	Json(serde_json::Value),
	Yaml(serde_yaml_ng::Mapping),
	Text(String),
	Bytes(Vec<u8>),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum VersionedFileKind {
	Cargo(monochange_cargo::CargoVersionedFileKind),
	Npm(monochange_npm::NpmVersionedFileKind),
	Deno(monochange_deno::DenoVersionedFileKind),
	Dart(monochange_dart::DartVersionedFileKind),
}

pub(crate) fn versioned_file_kind(
	ecosystem_type: monochange_core::EcosystemType,
	path: &Path,
) -> Option<VersionedFileKind> {
	match ecosystem_type {
		monochange_core::EcosystemType::Cargo => {
			monochange_cargo::supported_versioned_file_kind(path).map(VersionedFileKind::Cargo)
		}
		monochange_core::EcosystemType::Npm => {
			monochange_npm::supported_versioned_file_kind(path).map(VersionedFileKind::Npm)
		}
		monochange_core::EcosystemType::Deno => {
			monochange_deno::supported_versioned_file_kind(path).map(VersionedFileKind::Deno)
		}
		monochange_core::EcosystemType::Dart => {
			monochange_dart::supported_versioned_file_kind(path).map(VersionedFileKind::Dart)
		}
	}
}

fn dedup_versioned_file_definitions(
	versioned_files: Vec<VersionedFileDefinition>,
) -> Vec<VersionedFileDefinition> {
	let mut seen = BTreeSet::<String>::new();
	let mut deduped = Vec::new();
	for definition in versioned_files {
		let key = format!(
			"{}::{:?}::{:?}::{:?}::{:?}::{:?}",
			definition.path,
			definition.ecosystem_type,
			definition.prefix,
			definition.fields,
			definition.name,
			definition.regex,
		);
		if seen.insert(key) {
			deduped.push(definition);
		}
	}
	deduped
}

pub(crate) fn build_versioned_file_updates(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	if configuration.packages.is_empty() && configuration.groups.is_empty() {
		return Ok(Vec::new());
	}
	let released_versions_by_record_id = released_versions_by_record_id(plan);
	let package_by_config_id = packages
		.iter()
		.filter_map(|package| {
			package
				.metadata
				.get("config_id")
				.map(|config_id| (config_id.as_str(), package))
		})
		.collect::<BTreeMap<_, _>>();
	let package_by_native_name = packages
		.iter()
		.map(|package| (package.name.as_str(), package))
		.collect::<BTreeMap<_, _>>();
	let current_versions_by_native_name = packages
		.iter()
		.filter_map(|package| {
			package
				.current_version
				.as_ref()
				.map(|version| (package.name.clone(), version.to_string()))
		})
		.collect::<BTreeMap<_, _>>();
	let released_versions_by_config_id = packages
		.iter()
		.filter_map(|package| {
			package.metadata.get("config_id").and_then(|config_id| {
				released_versions_by_record_id
					.get(&package.id)
					.map(|version| (config_id.clone(), version.clone()))
			})
		})
		.collect::<BTreeMap<_, _>>();
	let released_versions_by_native_name = packages
		.iter()
		.filter_map(|package| {
			released_versions_by_record_id
				.get(&package.id)
				.map(|version| (package.name.clone(), version.clone()))
		})
		.collect::<BTreeMap<_, _>>();
	let shared_release_version = shared_release_version(plan);
	let context = VersionedFileUpdateContext {
		package_by_config_id,
		package_by_native_name,
		current_versions_by_native_name,
		released_versions_by_native_name,
		configuration,
	};
	let mut updates = BTreeMap::<PathBuf, CachedDocument>::new();

	for package_definition in &configuration.packages {
		let Some(version) = released_versions_by_config_id.get(&package_definition.id) else {
			continue;
		};
		let matched_package = context
			.package_by_config_id
			.get(package_definition.id.as_str());
		let dep_names = if let Some(name) = matched_package.map(|package| package.name.clone()) {
			vec![name]
		} else {
			vec![package_definition.id.clone()]
		};
		let effective_versioned_files = package_definition.versioned_files.clone();
		for versioned_file in dedup_versioned_file_definitions(effective_versioned_files) {
			let effective_dep_names = if let Some(override_name) = &versioned_file.name {
				vec![override_name.clone()]
			} else {
				dep_names.clone()
			};
			apply_versioned_file_definition(
				root,
				&mut updates,
				&versioned_file,
				version,
				shared_release_version.as_ref(),
				&effective_dep_names,
				&context,
			)?;
		}
	}

	for group_definition in &configuration.groups {
		let Some(group_version) = plan
			.groups
			.iter()
			.find(|group| group.group_id == group_definition.id)
			.and_then(|group| group.planned_version.as_ref())
			.map(ToString::to_string)
		else {
			continue;
		};
		// For groups, collect all member native names
		let group_dep_names = group_definition
			.packages
			.iter()
			.map(|member_id| {
				context
					.package_by_config_id
					.get(member_id.as_str())
					.map_or_else(|| member_id.clone(), |package| package.name.clone())
			})
			.collect::<Vec<_>>();
		for versioned_file in &group_definition.versioned_files {
			apply_versioned_file_definition(
				root,
				&mut updates,
				versioned_file,
				&group_version,
				Some(&group_version),
				&group_dep_names,
				&context,
			)?;
		}
	}

	apply_inferred_lockfile_updates(
		root,
		configuration,
		packages,
		plan,
		shared_release_version.as_ref(),
		&context,
		&mut updates,
	)?;

	updates
		.into_iter()
		.map(|(path, document)| serialize_cached_document(&path, document))
		.collect()
}

fn apply_inferred_lockfile_updates(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
	shared_release_version: Option<&String>,
	context: &VersionedFileUpdateContext<'_>,
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
) -> MonochangeResult<()> {
	let released_versions = released_versions_by_record_id(plan);
	let mut dep_names_by_lockfile =
		BTreeMap::<PathBuf, (monochange_core::EcosystemType, BTreeSet<String>)>::new();

	for package in packages
		.iter()
		.filter(|package| released_versions.contains_key(&package.id))
	{
		let Some(ecosystem_type) =
			inferred_lockfile_ecosystem_type(configuration, package.ecosystem)
		else {
			continue;
		};
		for lockfile_path in inferred_lockfile_paths(package) {
			let relative_lockfile = root_relative(root, &lockfile_path);
			let (_, dep_names) = dep_names_by_lockfile
				.entry(relative_lockfile)
				.or_insert_with(|| (ecosystem_type, BTreeSet::new()));
			dep_names.insert(package.name.clone());
		}
	}

	for (lockfile_path, (ecosystem_type, dep_names)) in dep_names_by_lockfile {
		let definition = VersionedFileDefinition {
			path: lockfile_path.display().to_string(),
			ecosystem_type: Some(ecosystem_type),
			prefix: None,
			fields: None,
			name: None,
			regex: None,
		};
		// Supported lockfiles can be rewritten directly from the release plan.
		// That keeps normal `mc release` runs on the fast path instead of paying
		// package-manager startup and dependency-resolution costs for every bump.
		apply_versioned_file_definition(
			root,
			updates,
			&definition,
			"",
			shared_release_version,
			&dep_names.into_iter().collect::<Vec<_>>(),
			context,
		)?;
	}

	Ok(())
}

fn inferred_lockfile_ecosystem_type(
	configuration: &monochange_core::WorkspaceConfiguration,
	ecosystem: Ecosystem,
) -> Option<monochange_core::EcosystemType> {
	match ecosystem {
		Ecosystem::Cargo if configuration.cargo.lockfile_commands.is_empty() => {
			Some(monochange_core::EcosystemType::Cargo)
		}
		Ecosystem::Npm if configuration.npm.lockfile_commands.is_empty() => {
			Some(monochange_core::EcosystemType::Npm)
		}
		Ecosystem::Deno if configuration.deno.lockfile_commands.is_empty() => {
			Some(monochange_core::EcosystemType::Deno)
		}
		Ecosystem::Dart | Ecosystem::Flutter if configuration.dart.lockfile_commands.is_empty() => {
			Some(monochange_core::EcosystemType::Dart)
		}
		_ => None,
	}
}

fn inferred_lockfile_paths(package: &PackageRecord) -> Vec<PathBuf> {
	match package.ecosystem {
		Ecosystem::Cargo => monochange_cargo::discover_lockfiles(package),
		Ecosystem::Npm => monochange_npm::discover_lockfiles(package),
		Ecosystem::Deno => monochange_deno::discover_lockfiles(package),
		Ecosystem::Dart | Ecosystem::Flutter => monochange_dart::discover_lockfiles(package),
	}
}

fn render_cached_document_bytes(
	_path: &Path,
	document: CachedDocument,
) -> MonochangeResult<Vec<u8>> {
	match document {
		CachedDocument::Json(value) => {
			let mut rendered = serde_json::to_string_pretty(&value)
				.map_err(|error| MonochangeError::Config(error.to_string()))?;
			rendered.push('\n');
			Ok(rendered.into_bytes())
		}
		CachedDocument::Yaml(mapping) => {
			serde_yaml_ng::to_string(&mapping)
				.map(String::into_bytes)
				.map_err(|error| MonochangeError::Config(error.to_string()))
		}
		CachedDocument::Text(contents) => Ok(contents.into_bytes()),
		CachedDocument::Bytes(contents) => Ok(contents),
	}
}

pub(crate) fn render_cached_document_text(
	path: &Path,
	document: CachedDocument,
) -> MonochangeResult<String> {
	String::from_utf8(render_cached_document_bytes(path, document)?).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse {} as text: {error}",
			path.display()
		))
	})
}

pub(crate) fn serialize_cached_document(
	path: &Path,
	document: CachedDocument,
) -> MonochangeResult<FileUpdate> {
	Ok(FileUpdate {
		path: path.to_path_buf(),
		content: render_cached_document_bytes(path, document)?,
	})
}

pub(crate) fn read_cached_text_document(
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
	path: &Path,
) -> MonochangeResult<String> {
	if let Some(cached) = updates.remove(path) {
		return render_cached_document_text(path, cached);
	}
	let contents = fs::read(path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;
	String::from_utf8(contents).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse {} as text: {error}",
			path.display()
		))
	})
}

pub(crate) fn read_cached_document(
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
	path: &Path,
	ecosystem_type: monochange_core::EcosystemType,
) -> MonochangeResult<CachedDocument> {
	if let Some(cached) = updates.remove(path) {
		return Ok(cached);
	}
	let Some(kind) = versioned_file_kind(ecosystem_type, path) else {
		return Err(MonochangeError::Config(format!(
			"unsupported versioned file `{}` for ecosystem `{}`",
			path.display(),
			match ecosystem_type {
				monochange_core::EcosystemType::Cargo => "cargo",
				monochange_core::EcosystemType::Npm => "npm",
				monochange_core::EcosystemType::Deno => "deno",
				monochange_core::EcosystemType::Dart => "dart",
			},
		)));
	};
	let contents = fs::read(path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;
	let text_contents = String::from_utf8(contents.clone())
		.map_err(|error| {
			MonochangeError::Config(format!(
				"failed to parse {} as text: {error}",
				path.display()
			))
		})
		.ok();
	match kind {
		VersionedFileKind::Cargo(kind) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			monochange_cargo::update_versioned_file_text(
				&contents,
				kind,
				&[],
				None,
				None,
				&BTreeMap::new(),
				&BTreeMap::new(),
			)
			.map_err(|error| {
				MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
			})?;
			Ok(CachedDocument::Text(contents))
		}
		VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::PnpmLock) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			monochange_npm::update_pnpm_lock_text(&contents, &BTreeMap::new()).map_err(
				|error| {
					MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
				},
			)?;
			Ok(CachedDocument::Text(contents))
		}
		VersionedFileKind::Dart(monochange_dart::DartVersionedFileKind::Lock) => {
			let Some(contents) = text_contents.as_ref() else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			let mapping =
				serde_yaml_ng::from_str::<serde_yaml_ng::Mapping>(contents).map_err(|error| {
					MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
				})?;
			Ok(CachedDocument::Yaml(mapping))
		}
		VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::BunLock) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			Ok(CachedDocument::Text(contents))
		}
		VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::BunLockBinary) => {
			Ok(CachedDocument::Bytes(contents))
		}
		VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::Manifest)
		| VersionedFileKind::Deno(monochange_deno::DenoVersionedFileKind::Manifest)
		| VersionedFileKind::Dart(monochange_dart::DartVersionedFileKind::Manifest) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			if kind == VersionedFileKind::Dart(monochange_dart::DartVersionedFileKind::Manifest) {
				monochange_dart::update_manifest_text(&contents, None, &[], &BTreeMap::new())
					.map_err(|error| {
						MonochangeError::Config(format!(
							"failed to parse {}: {error}",
							path.display()
						))
					})?;
			} else {
				monochange_core::update_json_manifest_text(&contents, None, &[], &BTreeMap::new())
					.map_err(|error| {
						MonochangeError::Config(format!(
							"failed to parse {}: {error}",
							path.display()
						))
					})?;
			}
			Ok(CachedDocument::Text(contents))
		}
		VersionedFileKind::Npm(_) | VersionedFileKind::Deno(_) => {
			let Some(contents) = text_contents.as_ref() else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			let value = serde_json::from_str::<serde_json::Value>(contents).map_err(|error| {
				MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
			})?;
			Ok(CachedDocument::Json(value))
		}
	}
}

pub(crate) fn resolve_versioned_prefix(
	definition: &VersionedFileDefinition,
	context: &VersionedFileUpdateContext<'_>,
) -> String {
	if let Some(prefix) = &definition.prefix {
		return prefix.clone();
	}
	let ecosystem_type = definition
		.ecosystem_type
		.expect("typed versioned_files should always have an ecosystem type");
	let ecosystem_prefix = match ecosystem_type {
		monochange_core::EcosystemType::Cargo => {
			context
				.configuration
				.cargo
				.dependency_version_prefix
				.clone()
		}
		monochange_core::EcosystemType::Npm => {
			context.configuration.npm.dependency_version_prefix.clone()
		}
		monochange_core::EcosystemType::Deno => {
			context.configuration.deno.dependency_version_prefix.clone()
		}
		monochange_core::EcosystemType::Dart => {
			context.configuration.dart.dependency_version_prefix.clone()
		}
	};
	ecosystem_prefix.unwrap_or_else(|| ecosystem_type.default_prefix().to_string())
}

pub(crate) fn expand_versioned_file_fields(
	definition: &VersionedFileDefinition,
	dep_names: &[String],
) -> Vec<String> {
	let ecosystem_type = definition
		.ecosystem_type
		.expect("typed versioned_files should always have an ecosystem type");
	let field_templates = definition.fields.as_ref().map_or_else(
		|| {
			ecosystem_type
				.default_fields()
				.iter()
				.map(ToString::to_string)
				.collect::<Vec<_>>()
		},
		Clone::clone,
	);
	let mut fields = Vec::new();
	for field_template in field_templates {
		if field_template.contains("{{ name }}") {
			fields.extend(
				dep_names
					.iter()
					.map(|name| field_template.replace("{{ name }}", name)),
			);
			continue;
		}
		if field_template.contains("{{name}}") {
			fields.extend(
				dep_names
					.iter()
					.map(|name| field_template.replace("{{name}}", name)),
			);
			continue;
		}
		fields.push(field_template);
	}
	fields
}

fn update_versioned_file_regex(
	contents: &str,
	pattern: &str,
	version: &str,
) -> MonochangeResult<String> {
	let regex = Regex::new(pattern).map_err(|error| {
		MonochangeError::Config(format!(
			"invalid versioned_files regex `{pattern}`: {error}"
		))
	})?;
	Ok(regex
		.replace_all(contents, |captures: &regex::Captures<'_>| {
			let whole_match = captures
				.get(0)
				.expect("regex replacement should always receive the full match");
			let version_match = captures
				.name("version")
				.expect("validated versioned_files regex should always capture `version`");
			let prefix = &whole_match.as_str()[..version_match.start() - whole_match.start()];
			let suffix = &whole_match.as_str()[version_match.end() - whole_match.start()..];
			format!("{prefix}{version}{suffix}")
		})
		.into_owned())
}

pub(crate) fn apply_versioned_file_definition(
	root: &Path,
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
	definition: &VersionedFileDefinition,
	owner_version: &str,
	shared_release_version: Option<&String>,
	dep_names: &[String],
	context: &VersionedFileUpdateContext<'_>,
) -> MonochangeResult<()> {
	if let Some(pattern) = &definition.regex {
		let glob_pattern = root.join(&definition.path).to_string_lossy().to_string();
		let matched_paths = glob::glob(&glob_pattern)
			.map_err(|error| {
				MonochangeError::Config(format!(
					"invalid glob pattern `{}`: {error}",
					definition.path
				))
			})?
			.collect::<Result<Vec<_>, _>>()
			.map_err(|error| MonochangeError::Config(error.to_string()))?;
		for resolved_path in matched_paths {
			let contents = read_cached_text_document(updates, &resolved_path)?;
			updates.insert(
				resolved_path,
				CachedDocument::Text(update_versioned_file_regex(
					&contents,
					pattern,
					owner_version,
				)?),
			);
		}
		return Ok(());
	}

	let ecosystem_type = definition.ecosystem_type.ok_or_else(|| {
		MonochangeError::Config(format!(
			"versioned file `{}` is missing an ecosystem type",
			definition.path
		))
	})?;
	let prefix = resolve_versioned_prefix(definition, context);
	let expanded_fields = expand_versioned_file_fields(definition, dep_names);
	let fields = expanded_fields
		.iter()
		.map(String::as_str)
		.collect::<Vec<_>>();
	let versioned_deps: BTreeMap<String, String> = dep_names
		.iter()
		.filter_map(|name| {
			context
				.released_versions_by_native_name
				.get(name)
				.map(|version| (name.clone(), format!("{prefix}{version}")))
		})
		.collect();
	let raw_versions: BTreeMap<String, String> = dep_names
		.iter()
		.filter_map(|name| {
			context
				.released_versions_by_native_name
				.get(name)
				.map(|version| (name.clone(), version.clone()))
		})
		.collect();
	if versioned_deps.is_empty() && raw_versions.is_empty() {
		return Ok(());
	}

	let glob_pattern = root.join(&definition.path).to_string_lossy().to_string();
	let matched_paths = glob::glob(&glob_pattern)
		.map_err(|error| {
			MonochangeError::Config(format!(
				"invalid glob pattern `{}`: {error}",
				definition.path
			))
		})?
		.collect::<Result<Vec<_>, _>>()
		.map_err(|error| MonochangeError::Config(error.to_string()))?;

	for resolved_path in matched_paths {
		let Some(kind) = versioned_file_kind(ecosystem_type, &resolved_path) else {
			return Err(MonochangeError::Config(format!(
				"versioned_files glob `{}` matched unsupported file `{}` for ecosystem `{}`; narrow the glob or change the `type`",
				definition.path,
				resolved_path.display(),
				match ecosystem_type {
					monochange_core::EcosystemType::Cargo => "cargo",
					monochange_core::EcosystemType::Npm => "npm",
					monochange_core::EcosystemType::Deno => "deno",
					monochange_core::EcosystemType::Dart => "dart",
				},
			)));
		};
		let package_paths_by_name = dep_names
			.iter()
			.filter_map(|name| {
				context
					.package_by_native_name
					.get(name.as_str())
					.map(|package| {
						(
							name.clone(),
							relative_to_root(
								resolved_path.parent().unwrap_or(root),
								package
									.manifest_path
									.parent()
									.unwrap_or(&package.workspace_root),
							)
							.unwrap_or_else(|| {
								package
									.manifest_path
									.parent()
									.unwrap_or(&package.workspace_root)
									.to_path_buf()
							}),
						)
					})
			})
			.collect::<BTreeMap<_, _>>();
		let mut document = read_cached_document(updates, &resolved_path, ecosystem_type)?;
		match (&mut document, kind) {
			(CachedDocument::Text(contents), VersionedFileKind::Cargo(kind)) => {
				*contents = monochange_cargo::update_versioned_file_text(
					contents,
					kind,
					&fields,
					Some(owner_version),
					shared_release_version.map(String::as_str),
					&versioned_deps,
					&raw_versions,
				)
				.map_err(|error| {
					MonochangeError::Config(format!(
						"failed to parse {}: {error}",
						resolved_path.display()
					))
				})?;
			}
			(CachedDocument::Text(contents), VersionedFileKind::Npm(kind)) => {
				if kind == monochange_npm::NpmVersionedFileKind::Manifest {
					*contents = monochange_core::update_json_manifest_text(
						contents,
						shared_release_version
							.map(String::as_str)
							.or(Some(owner_version)),
						&fields,
						&versioned_deps,
					)
					.map_err(|error| {
						MonochangeError::Config(format!(
							"failed to parse {}: {error}",
							resolved_path.display()
						))
					})?;
				} else if kind == monochange_npm::NpmVersionedFileKind::BunLock {
					*contents = monochange_npm::update_bun_lock(contents, &raw_versions);
				} else if kind == monochange_npm::NpmVersionedFileKind::PnpmLock {
					*contents = monochange_npm::update_pnpm_lock_text(contents, &raw_versions)
						.map_err(|error| {
							MonochangeError::Config(format!(
								"failed to parse {}: {error}",
								resolved_path.display()
							))
						})?;
				}
			}
			(
				CachedDocument::Json(value),
				VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::PackageLock),
			) => {
				monochange_npm::update_package_lock(value, &package_paths_by_name, &raw_versions);
			}
			(
				CachedDocument::Bytes(contents),
				VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::BunLockBinary),
			) => {
				let old_versions = dep_names
					.iter()
					.filter_map(|name| {
						context
							.current_versions_by_native_name
							.get(name)
							.map(|version| (name.clone(), version.clone()))
					})
					.collect::<BTreeMap<_, _>>();
				*contents =
					monochange_npm::update_bun_lock_binary(contents, &old_versions, &raw_versions);
			}
			(
				CachedDocument::Text(contents),
				VersionedFileKind::Deno(monochange_deno::DenoVersionedFileKind::Manifest),
			) => {
				*contents = monochange_core::update_json_manifest_text(
					contents,
					shared_release_version
						.map(String::as_str)
						.or(Some(owner_version)),
					&fields,
					&versioned_deps,
				)
				.map_err(|error| {
					MonochangeError::Config(format!(
						"failed to parse {}: {error}",
						resolved_path.display()
					))
				})?;
			}
			(
				CachedDocument::Json(value),
				VersionedFileKind::Deno(monochange_deno::DenoVersionedFileKind::Lock),
			) => {
				monochange_deno::update_lockfile(value, &raw_versions);
			}
			(
				CachedDocument::Text(contents),
				VersionedFileKind::Dart(monochange_dart::DartVersionedFileKind::Manifest),
			) => {
				*contents = monochange_dart::update_manifest_text(
					contents,
					shared_release_version
						.map(String::as_str)
						.or(Some(owner_version)),
					&fields,
					&versioned_deps,
				)
				.map_err(|error| {
					MonochangeError::Config(format!(
						"failed to parse {}: {error}",
						resolved_path.display()
					))
				})?;
			}
			(
				CachedDocument::Yaml(mapping),
				VersionedFileKind::Dart(monochange_dart::DartVersionedFileKind::Lock),
			) => {
				monochange_dart::update_pubspec_lock(mapping, &raw_versions);
			}
			_ => {}
		}
		updates.insert(resolved_path, document);
	}
	Ok(())
}

pub(crate) fn released_versions_by_record_id(plan: &ReleasePlan) -> BTreeMap<String, String> {
	plan.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
		.filter_map(|decision| {
			decision
				.planned_version
				.as_ref()
				.map(|version| (decision.package_id.clone(), version.to_string()))
		})
		.collect()
}
