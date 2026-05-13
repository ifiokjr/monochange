use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::BumpSeverity;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::Config;
use proptest::test_runner::RngAlgorithm;
use proptest::test_runner::TestRng;
use proptest::test_runner::TestRunner;
use serde_json::Value;
use serde_json::json;

/// Post-process a generated JSON schema by adding `$id`, `title`, and `description`.
pub fn post_process(schema: &mut serde_json::Value, id: &str, title: &str, description: &str) {
	let Some(obj) = schema.as_object_mut() else {
		return;
	};
	obj.insert("$id".to_string(), serde_json::Value::String(id.to_string()));
	obj.insert(
		"title".to_string(),
		serde_json::Value::String(title.to_string()),
	);
	obj.insert(
		"description".to_string(),
		serde_json::Value::String(description.to_string()),
	);
}

/// Post-process a release-record schema.
pub fn post_process_release(
	schema: &mut serde_json::Value,
	id: &str,
	title: &str,
	schema_version: &str,
) {
	post_process(
		schema,
		id,
		title,
		"Durable commit-embedded release record schema for monochange release records.",
	);
	let Some(obj) = schema.as_object_mut() else {
		return;
	};
	obj.insert(
		"additionalProperties".to_string(),
		serde_json::Value::Bool(false),
	);
	let Some(props) = schema
		.pointer_mut("/properties")
		.and_then(|value| value.as_object_mut())
	else {
		return;
	};
	if let Some(schema_version_obj) = props
		.get_mut("schemaVersion")
		.and_then(|schema_version| schema_version.as_object_mut())
	{
		schema_version_obj.insert(
			"default".to_string(),
			serde_json::Value::String(schema_version.to_string()),
		);
	}
	if let Some(kind_obj) = props.get_mut("kind").and_then(|kind| kind.as_object_mut()) {
		kind_obj.remove("default");
		kind_obj.insert(
			"const".to_string(),
			serde_json::Value::String(monochange_schema::release_record::KIND.to_string()),
		);
	}
}

/// Post-process a config schema by adding additionalProperties: false to all $defs objects.
pub fn post_process_config(schema: &mut serde_json::Value, id: &str, title: &str) {
	post_process(
		schema,
		id,
		title,
		"JSON Schema for monochange.toml workspace configuration files.",
	);

	// Walk $defs and add additionalProperties: false to all object definitions with properties
	#[allow(clippy::option_map_unit_fn)]
	schema
		.pointer_mut("/$defs")
		.and_then(|v| v.as_object_mut())
		.map(|defs| {
			for (_key, def) in defs.iter_mut() {
				if let Some(obj) = def.as_object_mut()
					&& obj.contains_key("properties")
				{
					obj.insert(
						"additionalProperties".to_string(),
						serde_json::Value::Bool(false),
					);
				}
			}
		});
}

/// Generate current schema JSON strings and write them to disk (update_mode) or compare to disk (check mode).
///
/// Returns `Ok(())` on success, or an error message describing the mismatch.
pub fn run(update_mode: bool) -> Result<(), String> {
	let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	let workspace_dir = crate_dir.parent().unwrap().parent().unwrap();
	let schemas_dir = workspace_dir.join("crates/monochange_schema/schemas");
	let docs_schemas_dir = workspace_dir.join("docs/src/schemas");
	let schema_version_path = workspace_dir.join("crates/monochange_schema/SCHEMA_VERSION");
	let version = current_schema_version(&schema_version_path)?;
	run_with_paths(
		update_mode,
		SchemaMode::Current,
		&schemas_dir,
		&docs_schemas_dir,
		&schema_version_path,
		&version,
	)
}

/// Generate release schema JSON strings, including immutable versioned files.
pub fn run_release(update_mode: bool) -> Result<(), String> {
	let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	let workspace_dir = crate_dir.parent().unwrap().parent().unwrap();
	let schemas_dir = workspace_dir.join("crates/monochange_schema/schemas");
	let docs_schemas_dir = workspace_dir.join("docs/src/schemas");
	let schema_version_path = workspace_dir.join("crates/monochange_schema/SCHEMA_VERSION");
	let version = expected_schema_version(workspace_dir)?;
	run_with_paths(
		update_mode,
		SchemaMode::Release,
		&schemas_dir,
		&docs_schemas_dir,
		&schema_version_path,
		&version,
	)
}

/// Selects which generated schema assets are maintained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemaMode {
	/// Maintain moving current aliases and deterministic current fixtures only.
	Current,
	/// Maintain current aliases plus immutable versioned release artifacts.
	Release,
}

struct GeneratedFile {
	path: PathBuf,
	contents: String,
}

const CURRENT_ARTIFACT_FIXTURE_COUNT: usize = 10;

#[derive(Clone, Debug)]
struct ArtifactVariant {
	seed: u8,
	owner: &'static str,
	repo: &'static str,
	package_count: usize,
	dry_run: bool,
	with_host: bool,
}

/// Core schema generation logic with configurable output directories.
pub fn run_with_paths(
	update_mode: bool,
	mode: SchemaMode,
	schemas_dir: &Path,
	docs_schemas_dir: &Path,
	schema_version_path: &Path,
	version: &str,
) -> Result<(), String> {
	let generated_files = schema_files(schemas_dir, docs_schemas_dir, version, mode);
	let schema_version_contents = schema_version_file_contents(version);

	if update_mode {
		if let Some(parent) = schema_version_path.parent() {
			fs::create_dir_all(parent)
				.map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
		}
		fs::write(schema_version_path, schema_version_contents).map_err(|error| {
			format!("Could not write {}: {error}", schema_version_path.display())
		})?;
		for generated_file in &generated_files {
			write_generated_file(generated_file)?;
		}
		remove_stale_artifact_files(schemas_dir)?;
		println!("Schemas updated successfully.");
		return Ok(());
	}

	let mut errors = Vec::new();
	if let Err(error) = check_text_files(&[(schema_version_path, schema_version_contents.as_str())])
	{
		errors.push(error);
	}

	let schema_checks = generated_files
		.iter()
		.map(|file| (file.path.as_path(), file.contents.as_str()))
		.collect::<Vec<_>>();
	if let Err(error) = check_schemas(&schema_checks) {
		errors.push(error);
	}
	if let Err(error) = check_stale_artifact_files_absent(schemas_dir) {
		errors.push(error);
	}

	if errors.is_empty() {
		println!("Schemas are up to date.");
		Ok(())
	} else {
		Err(errors.join("\n"))
	}
}

fn schema_files(
	schemas_dir: &Path,
	docs_schemas_dir: &Path,
	version: &str,
	mode: SchemaMode,
) -> Vec<GeneratedFile> {
	let release_schema = monochange_core::schema::release_record();
	let mut release_value = release_schema.to_value();
	post_process_release(
		&mut release_value,
		"https://monochange.github.io/monochange/schemas/release-record.schema.json",
		"monochange release record",
		version,
	);

	let config_schema = monochange_config::schema::workspace_configuration();
	let mut config_value = config_schema.to_value();
	post_process_config(
		&mut config_value,
		"https://monochange.github.io/monochange/schemas/monochange.schema.json",
		"monochange configuration",
	);

	let release_json = serde_json::to_string_pretty(&release_value).unwrap();
	let config_json = serde_json::to_string_pretty(&config_value).unwrap();
	let migration_changelog_json = format!(
		"{}\n",
		monochange_schema::migration_changelog::to_json_pretty().unwrap()
	);

	let artifacts_dir = schemas_dir.join("artifacts");
	let current_artifacts_dir = artifacts_dir.join("current");
	let mut files = vec![
		GeneratedFile {
			path: schemas_dir.join("release-record.schema.json"),
			contents: release_json.clone(),
		},
		GeneratedFile {
			path: schemas_dir.join("monochange.schema.json"),
			contents: config_json.clone(),
		},
		GeneratedFile {
			path: schemas_dir.join("migration-changelog.json"),
			contents: migration_changelog_json,
		},
		GeneratedFile {
			path: docs_schemas_dir.join("release-record.schema.json"),
			contents: release_json,
		},
		GeneratedFile {
			path: docs_schemas_dir.join("monochange.schema.json"),
			contents: config_json,
		},
	];
	files.extend(current_artifact_files(&current_artifacts_dir, version));

	if mode == SchemaMode::Release {
		let mut release_versioned_value = release_value.clone();
		let mut config_versioned_value = config_value.clone();
		post_process_release(
			&mut release_versioned_value,
			&format!(
				"https://monochange.github.io/monochange/schemas/release-record.v{version}.schema.json"
			),
			"monochange release record",
			version,
		);
		post_process_config(
			&mut config_versioned_value,
			&format!(
				"https://monochange.github.io/monochange/schemas/monochange.v{version}.schema.json"
			),
			"monochange configuration",
		);
		files.extend([
			GeneratedFile {
				path: docs_schemas_dir.join(format!("release-record.v{version}.schema.json")),
				contents: serde_json::to_string_pretty(&release_versioned_value).unwrap(),
			},
			GeneratedFile {
				path: docs_schemas_dir.join(format!("monochange.v{version}.schema.json")),
				contents: serde_json::to_string_pretty(&config_versioned_value).unwrap(),
			},
		]);
	}

	files
}

fn current_artifact_files(current_artifacts_dir: &Path, version: &str) -> Vec<GeneratedFile> {
	let variants = current_artifact_variants();
	let mut files = Vec::with_capacity(CURRENT_ARTIFACT_FIXTURE_COUNT * 2);
	for (index, variant) in variants.iter().enumerate() {
		let name = format!("{:02}.json", index + 1);
		files.push(GeneratedFile {
			path: current_artifacts_dir.join("release-record").join(&name),
			contents: release_record_artifact_fixture(version, index + 1, variant),
		});
		files.push(GeneratedFile {
			path: current_artifacts_dir.join("monochange").join(name),
			contents: config_artifact_fixture(index + 1, variant),
		});
	}
	files
}

fn current_artifact_variants() -> Vec<ArtifactVariant> {
	let strategy = (
		0u8..=u8::MAX,
		prop::sample::select(&["monochange", "aipi", "schema-lab", "release-tools"]),
		prop::sample::select(&["monochange", "workspace", "release-kit", "schema-fixtures"]),
		1usize..=3,
		any::<bool>(),
		any::<bool>(),
	)
		.prop_map(|(seed, owner, repo, package_count, dry_run, with_host)| {
			ArtifactVariant {
				seed,
				owner,
				repo,
				package_count,
				dry_run,
				with_host,
			}
		});
	let mut runner = TestRunner::new_with_rng(
		Config::default(),
		TestRng::from_seed(RngAlgorithm::ChaCha, b"monochange-current-artifact-seed"),
	);
	(0..CURRENT_ARTIFACT_FIXTURE_COUNT)
		.map(|_| {
			strategy
				.new_tree(&mut runner)
				.unwrap_or_else(|error| {
					panic!("generate current artifact fixture variant: {error}")
				})
				.current()
		})
		.collect()
}

fn release_record_artifact_fixture(
	version: &str,
	fixture_index: usize,
	variant: &ArtifactVariant,
) -> String {
	let mut value: Value = serde_json::from_str(
		&monochange_schema::release_record::populated_artifact_json(version),
	)
	.unwrap_or_else(|error| panic!("parse release-record artifact fixture template: {error}"));
	let release_version = format!("{version}.{fixture_index}");
	let changed_file_name = format!("{fixture_index:02}.json");
	let command = if variant.dry_run {
		"mc release --dry-run"
	} else {
		"mc release --commit"
	};
	let packages = ["monochange", "monochange_core", "monochange_schema"];
	let released_packages = packages
		.iter()
		.take(variant.package_count)
		.copied()
		.collect::<Vec<_>>();
	let object = value
		.as_object_mut()
		.expect("release-record fixture object");
	object.insert(
		"createdAt".to_string(),
		json!(format!("2026-01-{fixture_index:02}T00:00:00Z")),
	);
	object.insert("command".to_string(), json!(command));
	object.insert("version".to_string(), json!(release_version));
	object.insert("releasedPackages".to_string(), json!(released_packages));
	object.insert(
		"changedFiles".to_string(),
		json!([
			"Cargo.toml",
			"crates/monochange_schema/Cargo.toml",
			format!(
				"crates/monochange_schema/schemas/artifacts/current/release-record/{changed_file_name}"
			),
		]),
	);
	object.insert(
		"updatedChangelogs".to_string(),
		json!([format!("fixtures/changelog-{fixture_index}.md")]),
	);
	object.insert(
		"deletedChangesets".to_string(),
		json!([format!(".changeset/schema-artifact-{fixture_index}.md")]),
	);
	if let Some(versions) = object.get_mut("versions").and_then(Value::as_object_mut) {
		versions.insert(
			"main".to_string(),
			json!(format!("{version}.{fixture_index}")),
		);
		versions.insert(
			"monochange_schema".to_string(),
			json!(format!("{version}.{fixture_index}")),
		);
	}
	if let Some(targets) = object
		.get_mut("releaseTargets")
		.and_then(Value::as_array_mut)
	{
		for target in targets {
			if let Some(target) = target.as_object_mut() {
				target.insert(
					"version".to_string(),
					json!(format!("{version}.{fixture_index}")),
				);
				if let Some(id) = target.get("id").and_then(Value::as_str) {
					let tag_name = if id == "main" {
						format!("v{version}.{fixture_index}")
					} else {
						format!("{id}/v{version}.{fixture_index}")
					};
					target.insert("tagName".to_string(), json!(tag_name));
				}
			}
		}
	}
	if let Some(changesets) = object.get_mut("changesets").and_then(Value::as_array_mut)
		&& let Some(changeset) = changesets.first_mut().and_then(Value::as_object_mut)
	{
		changeset.insert(
			"path".to_string(),
			json!(format!(".changeset/schema-artifact-{fixture_index}.md")),
		);
		changeset.insert(
			"summary".to_string(),
			json!(format!("Exercise schema artifact fixture {fixture_index}")),
		);
		changeset.insert(
			"details".to_string(),
			json!(format!(
				"Generated from deterministic proptest seed {}.",
				variant.seed
			)),
		);
	}
	if let Some(provider) = object.get_mut("provider").and_then(Value::as_object_mut) {
		provider.insert("owner".to_string(), json!(variant.owner));
		provider.insert("repo".to_string(), json!(variant.repo));
	}
	serde_json::to_string_pretty(&value)
		.unwrap_or_else(|error| panic!("serialize release-record artifact fixture: {error}"))
}

fn config_artifact_fixture(_fixture_index: usize, variant: &ArtifactVariant) -> String {
	let mut source = serde_json::Map::new();
	source.insert("provider".to_string(), json!("github"));
	source.insert("owner".to_string(), json!(variant.owner));
	source.insert("repo".to_string(), json!(variant.repo));
	if variant.with_host {
		source.insert("host".to_string(), json!("github.com"));
	}
	let mut defaults = serde_json::Map::new();
	defaults.insert("package_type".to_string(), json!("cargo"));
	let mut packages = serde_json::Map::new();
	let package_names = ["monochange", "monochange_core", "monochange_schema"];
	for pkg_name in package_names.iter().take(variant.package_count) {
		let mut pkg = serde_json::Map::new();
		pkg.insert("path".to_string(), json!(format!("crates/{pkg_name}")));
		packages.insert(pkg_name.to_string(), json!(pkg));
	}
	serde_json::to_string_pretty(&json!({
		"source": Value::Object(source),
		"defaults": Value::Object(defaults),
		"package": Value::Object(packages),
	}))
	.unwrap_or_else(|error| panic!("serialize config artifact fixture: {error}"))
}

fn write_generated_file(generated_file: &GeneratedFile) -> Result<(), String> {
	if let Some(parent) = generated_file.path.parent() {
		fs::create_dir_all(parent)
			.map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
	}
	fs::write(&generated_file.path, &generated_file.contents)
		.map_err(|error| format!("Could not write {}: {error}", generated_file.path.display()))
}

fn remove_stale_artifact_files(schemas_dir: &Path) -> Result<(), String> {
	for path in stale_artifact_files(schemas_dir)? {
		if let Err(error) = fs::remove_file(&path)
			&& error.kind() != std::io::ErrorKind::NotFound
		{
			return Err(format!("Could not remove {}: {error}", path.display()));
		}
	}
	Ok(())
}

fn check_stale_artifact_files_absent(schemas_dir: &Path) -> Result<(), String> {
	let stale_files = stale_artifact_files(schemas_dir)?
		.into_iter()
		.filter(|path| path.exists())
		.map(|path| path.display().to_string())
		.collect::<Vec<_>>();
	if stale_files.is_empty() {
		Ok(())
	} else {
		Err(format!(
			"Stale artifact files should be removed: {}",
			stale_files.join(", ")
		))
	}
}

fn stale_artifact_files(schemas_dir: &Path) -> Result<Vec<PathBuf>, String> {
	let artifacts_dir = schemas_dir.join("artifacts");
	let current_artifacts_dir = artifacts_dir.join("current");
	let mut paths = vec![
		artifacts_dir.join("release-record.current.json"),
		artifacts_dir.join("monochange.current.json"),
		current_artifacts_dir.join("release-record.json"),
		current_artifacts_dir.join("monochange.json"),
	];
	if !artifacts_dir.exists() {
		return Ok(paths);
	}
	for entry in fs::read_dir(&artifacts_dir)
		.map_err(|error| format!("Could not read {}: {error}", artifacts_dir.display()))?
	{
		let path = entry
			.map_err(|error| format!("Could not read {}: {error}", artifacts_dir.display()))?
			.path();
		let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
			continue;
		};
		if (name.starts_with("release-record.v") || name.starts_with("monochange.v"))
			&& name.ends_with(".json")
		{
			paths.push(path);
		}
	}
	for kind in ["release-record", "monochange"] {
		let kind_dir = current_artifacts_dir.join(kind);
		if kind_dir.exists() {
			for entry in fs::read_dir(&kind_dir)
				.map_err(|error| format!("Could not read {}: {error}", kind_dir.display()))?
			{
				let path = entry
					.map_err(|error| format!("Could not read {}: {error}", kind_dir.display()))?
					.path();
				let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
					continue;
				};
				if let Some(stem) = name.strip_suffix(".json")
					&& stem.len() < 2
					&& stem.parse::<usize>().is_ok()
				{
					paths.push(path);
				}
			}
		}
	}
	Ok(paths)
}

fn current_schema_version(schema_version_path: &Path) -> Result<String, String> {
	let contents = fs::read_to_string(schema_version_path).map_err(|error| {
		format!(
			"Could not read current schema version from {}: {error}",
			schema_version_path.display()
		)
	})?;
	let version = contents.trim();
	if version.is_empty() {
		return Err(format!(
			"Current schema version file is empty: {}",
			schema_version_path.display()
		));
	}
	Ok(version.to_string())
}

fn schema_version_file_contents(version: &str) -> String {
	format!("{version}\n")
}

/// Derive the expected public schema version from the next `monochange_schema` release.
pub fn expected_schema_version(workspace_dir: &Path) -> Result<String, String> {
	let package_version = schema_package_manifest_version(workspace_dir)?;
	let current_version = semver::Version::parse(&package_version).map_err(|error| {
		format!("Could not parse monochange_schema package version `{package_version}`: {error}")
	})?;
	let next_version = planned_schema_bump(workspace_dir)?.apply_to_version(&current_version);
	monochange_schema::SchemaVersion::from_package_version(&next_version.to_string())
		.map(|schema_version| schema_version.to_string())
		.map_err(|error| format!("Could not derive schema version from `{next_version}`: {error}"))
}

fn schema_package_manifest_version(workspace_dir: &Path) -> Result<String, String> {
	let manifest_path = workspace_dir.join("crates/monochange_schema/Cargo.toml");
	let manifest = fs::read_to_string(&manifest_path)
		.map_err(|error| format!("Could not read {}: {error}", manifest_path.display()))?;
	package_version_from_manifest(&manifest).ok_or_else(|| {
		format!(
			"Could not find [package] version in {}",
			manifest_path.display()
		)
	})
}

fn package_version_from_manifest(manifest: &str) -> Option<String> {
	let mut in_package = false;
	for line in manifest.lines() {
		let trimmed = line.trim();
		if trimmed == "[package]" {
			in_package = true;
			continue;
		}
		if trimmed.starts_with('[') {
			in_package = false;
			continue;
		}
		if !in_package {
			continue;
		}
		let Some((key, value)) = trimmed.split_once('=') else {
			continue;
		};
		if key.trim() == "version" {
			return Some(clean_changeset_scalar(value).to_string());
		}
	}
	None
}

fn planned_schema_bump(workspace_dir: &Path) -> Result<BumpSeverity, String> {
	let changeset_dir = workspace_dir.join(".changeset");
	let Ok(entries) = fs::read_dir(&changeset_dir) else {
		return Ok(BumpSeverity::None);
	};
	let mut bump = BumpSeverity::None;
	for entry in entries {
		let entry = entry.map_err(|error| {
			format!(
				"Could not read entry in {}: {error}",
				changeset_dir.display()
			)
		})?;
		let path = entry.path();
		if path.extension().and_then(|value| value.to_str()) != Some("md") {
			continue;
		}
		let contents = fs::read_to_string(&path)
			.map_err(|error| format!("Could not read {}: {error}", path.display()))?;
		bump = bump.max(changeset_bump_for_package(&contents, "monochange_schema"));
	}
	Ok(bump)
}

fn changeset_bump_for_package(contents: &str, package: &str) -> BumpSeverity {
	let normalized = contents.replace("\r\n", "\n").replace('\r', "\n");
	let Some(without_opening) = normalized.strip_prefix("---") else {
		return BumpSeverity::None;
	};
	let Some((frontmatter, _body)) = without_opening.split_once("\n---") else {
		return BumpSeverity::None;
	};

	let mut bump = BumpSeverity::None;
	let mut active_package = false;
	for line in frontmatter.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}

		if line.starts_with(' ') || line.starts_with('\t') {
			if active_package {
				bump = bump.max(nested_bump(trimmed));
			}
			continue;
		}

		active_package = false;
		let Some((raw_key, raw_value)) = trimmed.split_once(':') else {
			continue;
		};
		let key = clean_changeset_scalar(raw_key);
		if key != package {
			continue;
		}

		active_package = true;
		bump = bump.max(inline_bump(raw_value));
	}
	bump
}

fn nested_bump(line: &str) -> BumpSeverity {
	let Some((key, value)) = line.split_once(':') else {
		return BumpSeverity::None;
	};
	if clean_changeset_scalar(key) != "bump" {
		return BumpSeverity::None;
	}
	bump_from_text(clean_changeset_scalar(value))
}

fn inline_bump(value: &str) -> BumpSeverity {
	let value = value.trim();
	if value.is_empty() {
		return BumpSeverity::None;
	}
	let direct = bump_from_text(clean_changeset_scalar(value));
	if direct != BumpSeverity::None {
		return direct;
	}

	let inline_table = value.trim_start_matches('{').trim_end_matches('}');
	inline_table
		.split(',')
		.map(nested_bump)
		.max()
		.unwrap_or(BumpSeverity::None)
}

fn bump_from_text(value: &str) -> BumpSeverity {
	match clean_changeset_scalar(value) {
		"major" => BumpSeverity::Major,
		"minor" => BumpSeverity::Minor,
		"patch" => BumpSeverity::Patch,
		_ => BumpSeverity::None,
	}
}

fn clean_changeset_scalar(value: &str) -> &str {
	value
		.trim()
		.trim_matches(',')
		.trim_matches('"')
		.trim_matches('\'')
		.trim()
}

const COMMANDS_INVENTORY_START: &str = "<!-- xtask:commands:start -->";
const COMMANDS_INVENTORY_END: &str = "<!-- xtask:commands:end -->";

/// Check or update the generated command inventory in the monochange skill package.
pub fn run_skill_commands(update_mode: bool) -> Result<(), String> {
	let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	let workspace_dir = crate_dir.parent().unwrap().parent().unwrap();
	let commands_path = workspace_dir.join("packages/monochange__skill/skills/commands.md");
	run_skill_commands_with_paths(update_mode, workspace_dir, &commands_path)
}

/// Check or update a commands skill file against the CLI source and workspace config.
pub fn run_skill_commands_with_paths(
	update_mode: bool,
	workspace_dir: &Path,
	commands_path: &Path,
) -> Result<(), String> {
	let expected = render_current_commands_inventory(workspace_dir)?;
	let current = fs::read_to_string(commands_path)
		.map_err(|error| format!("Could not read {}: {error}", commands_path.display()))?;
	let updated = replace_commands_inventory(&current, &expected)?;

	if update_mode {
		fs::write(commands_path, updated)
			.map_err(|error| format!("Could not write {}: {error}", commands_path.display()))?;
		println!("Skill command inventory updated successfully.");
		return Ok(());
	}

	if current == updated {
		println!("Skill command inventory is up to date.");
		Ok(())
	} else {
		Err(format!(
			"Skill command inventory is out of date: {}\nRun `cargo xtask skill commands update`.",
			commands_path.display()
		))
	}
}

fn render_current_commands_inventory(workspace_dir: &Path) -> Result<String, String> {
	let cli_source_path = workspace_dir.join("crates/monochange/src/cli.rs");
	let cli_source = fs::read_to_string(&cli_source_path)
		.map_err(|error| format!("Could not read {}: {error}", cli_source_path.display()))?;
	let built_in = command_literals_from_cli_source(&cli_source);
	let configured = configured_command_names(workspace_dir)?;
	let step_commands = monochange_core::all_step_variants()
		.into_iter()
		.map(|step| format!("step:{}", step.step_kebab_name()))
		.collect::<BTreeSet<_>>();

	Ok(render_commands_inventory(
		&built_in,
		&configured,
		&step_commands,
	))
}

fn configured_command_names(workspace_dir: &Path) -> Result<BTreeSet<String>, String> {
	let configuration = monochange_config::load_workspace_configuration(workspace_dir)
		.map_err(|error| format!("Could not load monochange.toml: {error}"))?;
	Ok(configuration
		.cli
		.into_iter()
		.map(|command| command.name)
		.collect())
}

fn command_literals_from_cli_source(source: &str) -> BTreeSet<String> {
	let mut commands = BTreeSet::new();
	let mut remaining = source;
	let needle = "Command::new(\"";

	while let Some((_, after_needle)) = remaining.split_once(needle) {
		let Some((command, after_command)) = after_needle.split_once('\"') else {
			break;
		};
		commands.insert(command.to_string());
		remaining = after_command;
	}

	commands
}

fn render_commands_inventory(
	built_in: &BTreeSet<String>,
	configured: &BTreeSet<String>,
	step_commands: &BTreeSet<String>,
) -> String {
	let mut inventory = String::new();
	inventory.push_str(COMMANDS_INVENTORY_START);
	inventory.push_str("\n\n");
	inventory.push_str(
		"This inventory is generated by `cargo xtask skill commands update` and checked by `cargo xtask skill commands check`.\n\n",
	);
	push_inventory_group(
		&mut inventory,
		"Command literals in `crates/monochange/src/cli.rs`",
		built_in,
	);
	push_inventory_group(
		&mut inventory,
		"Configured workflow commands in this repository's `monochange.toml`",
		configured,
	);
	push_inventory_group(
		&mut inventory,
		"Built-in `mc step:*` commands from `CliStepDefinition`",
		step_commands,
	);
	inventory.push_str(COMMANDS_INVENTORY_END);
	inventory
}

fn push_inventory_group(inventory: &mut String, title: &str, commands: &BTreeSet<String>) {
	inventory.push_str("### ");
	inventory.push_str(title);
	inventory.push_str("\n\n");

	for command in commands {
		inventory.push_str("- `");
		inventory.push_str(command);
		inventory.push_str("`\n");
	}
	inventory.push('\n');
}

fn replace_commands_inventory(current: &str, expected: &str) -> Result<String, String> {
	let Some((before, rest)) = current.split_once(COMMANDS_INVENTORY_START) else {
		return Err(format!(
			"Missing command inventory start marker `{COMMANDS_INVENTORY_START}`"
		));
	};
	let Some((_, after)) = rest.split_once(COMMANDS_INVENTORY_END) else {
		return Err(format!(
			"Missing command inventory end marker `{COMMANDS_INVENTORY_END}`"
		));
	};

	let mut updated = String::new();
	updated.push_str(before);
	updated.push_str(expected);
	updated.push_str(after);
	Ok(updated)
}

/// Compare expected text strings against files on disk.
fn check_text_files(paths: &[(&Path, &str)]) -> Result<(), String> {
	let mut errors = Vec::new();
	for (path, expected) in paths {
		if path.exists() {
			let existing = fs::read_to_string(path).unwrap();
			if existing != *expected {
				errors.push(format!("Generated file mismatch: {}", path.display()));
			}
		} else {
			errors.push(format!("Generated file missing: {}", path.display()));
		}
	}
	if errors.is_empty() {
		Ok(())
	} else {
		Err(errors.join("\n"))
	}
}

/// Compare expected schema JSON strings against files on disk.
fn check_schemas(paths: &[(&Path, &str)]) -> Result<(), String> {
	let mut errors = Vec::new();
	for (path, expected) in paths {
		if path.exists() {
			let existing = fs::read_to_string(path).unwrap();
			let existing_value: serde_json::Value = match serde_json::from_str(&existing) {
				Ok(v) => v,
				Err(_) => {
					errors.push(format!(
						"Schema mismatch (invalid JSON): {}",
						path.display()
					));
					continue;
				}
			};
			let expected_value: serde_json::Value = match serde_json::from_str(expected) {
				Ok(v) => v,
				Err(_) => {
					errors.push(format!(
						"Generated schema contains invalid JSON for {}",
						path.display()
					));
					continue;
				}
			};
			if existing_value != expected_value {
				errors.push(format!("Schema mismatch: {}", path.display()));
			}
		} else {
			errors.push(format!("Schema file missing: {}", path.display()));
		}
	}
	if errors.is_empty() {
		Ok(())
	} else {
		Err(errors.join("\n"))
	}
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
