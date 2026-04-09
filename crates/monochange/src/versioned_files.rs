use super::*;

pub(crate) struct VersionedFileUpdateContext<'a> {
	pub(crate) package_by_record_id: BTreeMap<&'a str, &'a PackageRecord>,
	pub(crate) released_versions_by_native_name: BTreeMap<String, String>,
	pub(crate) configuration: &'a monochange_core::WorkspaceConfiguration,
}

#[derive(Debug)]
pub(crate) enum CachedDocument {
	Toml(Value),
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

fn auto_discovered_lockfile_definitions(
	root: &Path,
	package: &PackageRecord,
) -> Vec<VersionedFileDefinition> {
	let ecosystem_type = match package.ecosystem {
		Ecosystem::Cargo => monochange_core::EcosystemType::Cargo,
		Ecosystem::Npm => monochange_core::EcosystemType::Npm,
		Ecosystem::Deno => monochange_core::EcosystemType::Deno,
		Ecosystem::Dart | Ecosystem::Flutter => monochange_core::EcosystemType::Dart,
	};
	let discovered = match package.ecosystem {
		Ecosystem::Cargo => monochange_cargo::discover_lockfiles(package),
		Ecosystem::Npm => monochange_npm::discover_lockfiles(package),
		Ecosystem::Deno => monochange_deno::discover_lockfiles(package),
		Ecosystem::Dart | Ecosystem::Flutter => monochange_dart::discover_lockfiles(package),
	};
	discovered
		.into_iter()
		.filter_map(|path| {
			relative_to_root(root, &path).map(|relative_path| VersionedFileDefinition {
				path: relative_path.to_string_lossy().to_string(),
				ecosystem_type,
				prefix: None,
				fields: None,
				name: None,
			})
		})
		.collect()
}

fn dedup_versioned_file_definitions(
	versioned_files: Vec<VersionedFileDefinition>,
) -> Vec<VersionedFileDefinition> {
	let mut seen = BTreeSet::<String>::new();
	let mut deduped = Vec::new();
	for definition in versioned_files {
		let key = format!(
			"{}::{:?}::{:?}::{:?}::{:?}",
			definition.path,
			definition.ecosystem_type,
			definition.prefix,
			definition.fields,
			definition.name,
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
	let package_by_record_id = packages
		.iter()
		.map(|package| (package.id.as_str(), package))
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
		package_by_record_id,
		released_versions_by_native_name,
		configuration,
	};
	let mut updates = BTreeMap::<PathBuf, CachedDocument>::new();

	for package_definition in &configuration.packages {
		let Some(version) = released_versions_by_config_id.get(&package_definition.id) else {
			continue;
		};
		let matched_package = context
			.package_by_record_id
			.values()
			.find(|package| package.metadata.get("config_id") == Some(&package_definition.id));
		let dep_names = if let Some(name) = matched_package.map(|package| package.name.clone()) {
			vec![name]
		} else {
			vec![package_definition.id.clone()]
		};
		let mut effective_versioned_files = package_definition.versioned_files.clone();
		if let Some(package) = matched_package {
			effective_versioned_files.extend(auto_discovered_lockfile_definitions(root, package));
		}
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
					.package_by_record_id
					.values()
					.find(|package| package.metadata.get("config_id") == Some(member_id))
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

	updates
		.into_iter()
		.map(|(path, document)| serialize_cached_document(&path, document))
		.collect()
}

pub(crate) fn serialize_cached_document(
	path: &Path,
	document: CachedDocument,
) -> MonochangeResult<FileUpdate> {
	let content = match document {
		CachedDocument::Toml(value) => toml::to_string_pretty(&value)
			.map(String::into_bytes)
			.map_err(|error| MonochangeError::Config(error.to_string()))?,
		CachedDocument::Json(value) => {
			let mut rendered = serde_json::to_string_pretty(&value)
				.map_err(|error| MonochangeError::Config(error.to_string()))?;
			rendered.push('\n');
			rendered.into_bytes()
		}
		CachedDocument::Yaml(mapping) => serde_yaml_ng::to_string(&mapping)
			.map(String::into_bytes)
			.map_err(|error| MonochangeError::Config(error.to_string()))?,
		CachedDocument::Text(contents) => contents.into_bytes(),
		CachedDocument::Bytes(contents) => contents,
	};
	Ok(FileUpdate {
		path: path.to_path_buf(),
		content,
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
				"failed to parse {} as utf-8 text: {error}",
				path.display()
			))
		})
		.ok();
	match kind {
		VersionedFileKind::Cargo(_) => {
			let Some(contents) = text_contents.as_ref() else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			let value = toml::from_str::<Value>(contents).map_err(|error| {
				MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
			})?;
			Ok(CachedDocument::Toml(value))
		}
		VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::PnpmLock)
		| VersionedFileKind::Dart(_) => {
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

pub(crate) fn update_json_dependency_fields(
	value: &mut serde_json::Value,
	fields: &[&str],
	versioned_deps: &BTreeMap<String, String>,
) {
	for field in fields {
		if let Some(section) = value
			.get_mut(*field)
			.and_then(serde_json::Value::as_object_mut)
		{
			for (dep_name, dep_version) in versioned_deps {
				if section.contains_key(dep_name) {
					section.insert(
						dep_name.clone(),
						serde_json::Value::String(dep_version.clone()),
					);
				}
			}
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
	let ecosystem_prefix = match definition.ecosystem_type {
		monochange_core::EcosystemType::Cargo => context
			.configuration
			.cargo
			.dependency_version_prefix
			.clone(),
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
	ecosystem_prefix.unwrap_or_else(|| definition.ecosystem_type.default_prefix().to_string())
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
	let prefix = resolve_versioned_prefix(definition, context);
	let fields = definition.fields.as_deref().map_or_else(
		|| definition.ecosystem_type.default_fields().to_vec(),
		|fields| fields.iter().map(String::as_str).collect::<Vec<_>>(),
	);
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
		let Some(kind) = versioned_file_kind(definition.ecosystem_type, &resolved_path) else {
			return Err(MonochangeError::Config(format!(
				"versioned_files glob `{}` matched unsupported file `{}` for ecosystem `{}`; narrow the glob or change the `type`",
				definition.path,
				resolved_path.display(),
				match definition.ecosystem_type {
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
				context.package_by_record_id.values().find_map(|package| {
					(package.name == *name).then(|| {
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
			})
			.collect::<BTreeMap<_, _>>();
		let mut document =
			read_cached_document(updates, &resolved_path, definition.ecosystem_type)?;
		match (&mut document, kind) {
			(CachedDocument::Toml(value), VersionedFileKind::Cargo(kind)) => {
				monochange_cargo::update_versioned_file(
					value,
					kind,
					&fields,
					owner_version,
					shared_release_version,
					&versioned_deps,
					&raw_versions,
				);
			}
			(CachedDocument::Json(value), VersionedFileKind::Npm(kind)) => match kind {
				monochange_npm::NpmVersionedFileKind::Manifest => {
					monochange_npm::update_json_dependency_fields(value, &fields, &versioned_deps);
				}
				monochange_npm::NpmVersionedFileKind::PackageLock => {
					monochange_npm::update_package_lock(
						value,
						&package_paths_by_name,
						&raw_versions,
					);
				}
				monochange_npm::NpmVersionedFileKind::PnpmLock
				| monochange_npm::NpmVersionedFileKind::BunLock
				| monochange_npm::NpmVersionedFileKind::BunLockBinary => {}
			},
			(CachedDocument::Yaml(mapping), VersionedFileKind::Npm(kind)) => {
				if kind == monochange_npm::NpmVersionedFileKind::PnpmLock {
					monochange_npm::update_pnpm_lock(mapping, &raw_versions);
				}
			}
			(CachedDocument::Text(contents), VersionedFileKind::Npm(kind)) => {
				if kind == monochange_npm::NpmVersionedFileKind::BunLock {
					*contents = monochange_npm::update_bun_lock(contents, &raw_versions);
				}
			}
			(CachedDocument::Bytes(contents), VersionedFileKind::Npm(kind)) => {
				if kind == monochange_npm::NpmVersionedFileKind::BunLockBinary {
					let old_versions = dep_names
						.iter()
						.filter_map(|name| {
							context.package_by_record_id.values().find_map(|package| {
								(package.name == *name)
									.then_some(
										package
											.current_version
											.as_ref()
											.map(|version| (name.clone(), version.to_string())),
									)
									.flatten()
							})
						})
						.collect::<BTreeMap<_, _>>();
					*contents = monochange_npm::update_bun_lock_binary(
						contents,
						&old_versions,
						&raw_versions,
					);
				}
			}
			(CachedDocument::Json(value), VersionedFileKind::Deno(kind)) => match kind {
				monochange_deno::DenoVersionedFileKind::Manifest => {
					update_json_dependency_fields(value, &fields, &versioned_deps);
				}
				monochange_deno::DenoVersionedFileKind::Lock => {
					monochange_deno::update_lockfile(value, &raw_versions);
				}
			},
			(CachedDocument::Yaml(mapping), VersionedFileKind::Dart(kind)) => match kind {
				monochange_dart::DartVersionedFileKind::Manifest => {
					monochange_dart::update_dependency_fields(mapping, &fields, &versioned_deps);
				}
				monochange_dart::DartVersionedFileKind::Lock => {
					monochange_dart::update_pubspec_lock(mapping, &raw_versions);
				}
			},
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
