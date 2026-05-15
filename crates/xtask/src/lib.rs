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
use serde_json::Map;
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
pub fn run_release(update_mode: bool, include_versioned: bool) -> Result<(), String> {
	let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	let workspace_dir = crate_dir.parent().unwrap().parent().unwrap();
	let schemas_dir = workspace_dir.join("crates/monochange_schema/schemas");
	let docs_schemas_dir = workspace_dir.join("docs/src/schemas");
	let schema_version_path = workspace_dir.join("crates/monochange_schema/SCHEMA_VERSION");
	let version = expected_schema_version(workspace_dir)?;
	run_with_paths(
		update_mode,
		SchemaMode::Release { include_versioned },
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
	/// Maintain release schema assets.
	Release { include_versioned: bool },
}

struct GeneratedFile {
	path: PathBuf,
	contents: String,
}

const CURRENT_ARTIFACT_FIXTURE_COUNT: usize = 10;

#[derive(Clone, Debug)]
struct ReleaseRecordVariant {
	seed: u64,
	provider_kind: String,
	owner: String,
	repo: String,
	with_host: bool,
	command: String,
	release_target_count: usize,
	version_format: String,
	bump: Option<String>,
	change_type: Option<String>,
	with_caused_by: bool,
	with_changeset_context: bool,
	released_package_count: usize,
	changed_file_count: usize,
	with_changelogs: bool,
	with_publications: bool,
}

#[derive(Clone, Debug)]
struct ConfigVariant {
	provider_kind: String,
	owner: String,
	repo: String,
	with_host: bool,
	ecosystem: String,
	package_count: usize,
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

	let artifacts_dir = schemas_dir.join("artifacts");
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
			path: docs_schemas_dir.join("release-record.schema.json"),
			contents: release_json,
		},
		GeneratedFile {
			path: docs_schemas_dir.join("monochange.schema.json"),
			contents: config_json,
		},
	];
	files.extend(artifact_files(&artifacts_dir, "current", version));

	let include_versioned = match mode {
		SchemaMode::Current => false,
		SchemaMode::Release { include_versioned } => include_versioned,
	};
	if include_versioned {
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
		files.extend(artifact_files(&artifacts_dir, version, version));
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

fn artifact_files(
	artifacts_dir: &Path,
	version_dir: &str,
	schema_version: &str,
) -> Vec<GeneratedFile> {
	let release_variants = current_release_record_variants();
	let config_variants = current_config_variants();
	let artifacts_version_dir = artifacts_dir.join(version_dir);
	let mut files = Vec::with_capacity(CURRENT_ARTIFACT_FIXTURE_COUNT * 2);
	for (index, variant) in release_variants.iter().enumerate() {
		let name = format!("{:02}.json", index + 1);
		files.push(GeneratedFile {
			path: artifacts_version_dir.join("release-record").join(&name),
			contents: release_record_artifact_fixture(schema_version, index + 1, variant),
		});
	}
	for (index, variant) in config_variants.iter().enumerate() {
		let name = format!("{:02}.json", index + 1);
		files.push(GeneratedFile {
			path: artifacts_version_dir.join("monochange").join(&name),
			contents: config_artifact_fixture(index + 1, variant),
		});
	}
	files
}

fn current_release_record_variants() -> Vec<ReleaseRecordVariant> {
	let snake_case_id = prop::collection::vec(prop::char::range('a', 'z'), 3..=12)
		.prop_map(|chars| chars.into_iter().collect::<String>());
	let snake_case_id2 = prop::collection::vec(prop::char::range('a', 'z'), 3..=12)
		.prop_map(|chars| chars.into_iter().collect::<String>());
	let strategy = (
		(
			any::<u64>(),
			prop::sample::select(&["github", "gitlab", "gitea", "forgejo"][..]),
			snake_case_id,
			snake_case_id2,
			any::<bool>(),
		),
		(
			prop::sample::select(
				&[
					"mc release --commit",
					"mc release --dry-run",
					"mc step:release",
				][..],
			),
			1_usize..=3,
			prop::sample::select(&["primary", "namespaced"][..]),
			prop::option::of(prop::sample::select(
				&["none", "patch", "minor", "major"][..],
			)),
			prop::option::of(prop::sample::select(
				&["fix", "feature", "breaking", "refactor", "docs", "chore"][..],
			)),
		),
		(
			any::<bool>(),
			any::<bool>(),
			1_usize..=5,
			1_usize..=5,
			any::<bool>(),
			any::<bool>(),
		),
	)
		.prop_map(
			|(
				(seed, provider_kind, owner, repo, with_host),
				(command, release_target_count, version_format, bump, change_type),
				(
					with_caused_by,
					with_changeset_context,
					released_package_count,
					changed_file_count,
					with_changelogs,
					with_publications,
				),
			)| {
				ReleaseRecordVariant {
					seed,
					provider_kind: provider_kind.to_string(),
					owner,
					repo,
					with_host,
					command: command.to_string(),
					release_target_count,
					version_format: version_format.to_string(),
					bump: bump.map(String::from),
					change_type: change_type.map(String::from),
					with_caused_by,
					with_changeset_context,
					released_package_count,
					changed_file_count,
					with_changelogs,
					with_publications,
				}
			},
		);
	let mut runner = TestRunner::new_with_rng(
		Config::default(),
		TestRng::from_seed(RngAlgorithm::ChaCha, b"monochange-current-artifact-seed"),
	);
	(0..CURRENT_ARTIFACT_FIXTURE_COUNT)
		.map(|_| {
			strategy
				.new_tree(&mut runner)
				.unwrap_or_else(|error| {
					panic!("generate current release record artifact fixture variant: {error}")
				})
				.current()
		})
		.collect()
}

fn current_config_variants() -> Vec<ConfigVariant> {
	let snake_case_id = prop::collection::vec(prop::char::range('a', 'z'), 3..=12)
		.prop_map(|chars| chars.into_iter().collect::<String>());
	let snake_case_id2 = prop::collection::vec(prop::char::range('a', 'z'), 3..=12)
		.prop_map(|chars| chars.into_iter().collect::<String>());
	let strategy = (
		prop::sample::select(&["github", "gitlab", "gitea", "forgejo"][..]),
		snake_case_id,
		snake_case_id2,
		any::<bool>(),
		prop::sample::select(&["cargo", "npm", "deno", "dart", "flutter", "python", "go"][..]),
		1_usize..=5,
	)
		.prop_map(
			|(provider_kind, owner, repo, with_host, ecosystem, package_count)| {
				ConfigVariant {
					provider_kind: provider_kind.to_string(),
					owner,
					repo,
					with_host,
					ecosystem: ecosystem.to_string(),
					package_count,
				}
			},
		);
	let mut runner = TestRunner::new_with_rng(
		Config::default(),
		TestRng::from_seed(RngAlgorithm::ChaCha, b"monochange-current-artifact-seed"),
	);
	(0..CURRENT_ARTIFACT_FIXTURE_COUNT)
		.map(|_| {
			strategy
				.new_tree(&mut runner)
				.unwrap_or_else(|error| {
					panic!("generate current config artifact fixture variant: {error}")
				})
				.current()
		})
		.collect()
}

fn release_record_artifact_fixture(
	version: &str,
	fixture_index: usize,
	variant: &ReleaseRecordVariant,
) -> String {
	let mut root = Map::new();
	root.insert("schemaVersion".to_string(), json!(version));
	root.insert("kind".to_string(), json!("monochange.releaseRecord"));
	root.insert(
		"createdAt".to_string(),
		json!(format!("2026-01-{fixture_index:02}T00:00:00Z")),
	);
	root.insert("command".to_string(), json!(&variant.command));

	// release targets
	let mut release_targets = Vec::new();
	for i in 0..variant.release_target_count {
		let mut target = Map::new();
		let (id, kind): (String, &str) = if i == 0 {
			("main".to_string(), "group")
		} else {
			(format!("{}_{}", variant.owner, i), "package")
		};
		target.insert("id".to_string(), json!(id));
		target.insert("kind".to_string(), json!(kind));
		let ver = format!("{version}.{fixture_index}");
		target.insert("version".to_string(), json!(ver));
		let vfmt = if i == 0 {
			"primary"
		} else {
			&variant.version_format
		};
		target.insert("versionFormat".to_string(), json!(vfmt));
		target.insert("tag".to_string(), json!(true));
		target.insert("release".to_string(), json!(true));
		let tag_name = if id == "main" {
			format!("v{version}.{fixture_index}")
		} else {
			format!("{id}/v{version}.{fixture_index}")
		};
		target.insert("tagName".to_string(), json!(tag_name));
		target.insert("members".to_string(), json!(Vec::<String>::new()));
		release_targets.push(Value::Object(target));
	}
	root.insert("releaseTargets".to_string(), json!(release_targets));

	// released packages
	let released_packages: Vec<String> = (0..variant.released_package_count)
		.map(|i| format!("{}_{}", variant.owner, i))
		.collect();
	root.insert("releasedPackages".to_string(), json!(released_packages));

	// changed files
	let changed_files: Vec<String> = (0..variant.changed_file_count)
		.map(|i| format!("crates/{}_{}/src/lib.rs", variant.owner, i))
		.collect();
	root.insert("changedFiles".to_string(), json!(changed_files));

	// provider — always present
	{
		let mut provider = Map::new();
		provider.insert("kind".to_string(), json!(&variant.provider_kind));
		provider.insert("owner".to_string(), json!(&variant.owner));
		provider.insert("repo".to_string(), json!(&variant.repo));
		if variant.with_host {
			provider.insert(
				"host".to_string(),
				json!(format!("{}.example.com", variant.provider_kind)),
			);
		} else {
			provider.insert("host".to_string(), Value::Null);
		}
		root.insert("provider".to_string(), Value::Object(provider));
	}

	// version (nullable)
	root.insert(
		"version".to_string(),
		json!(format!("{version}.{fixture_index}")),
	);

	// versions object
	let mut versions = Map::new();
	versions.insert(
		"main".to_string(),
		json!(format!("{version}.{fixture_index}")),
	);
	for i in 0..variant.released_package_count.min(3) {
		versions.insert(
			format!("{}_{}", variant.owner, i),
			json!(format!("{version}.{fixture_index}")),
		);
	}
	root.insert("versions".to_string(), Value::Object(versions));

	// changesets — targets is an array of PreparedChangesetTarget objects
	let mut changeset = Map::new();
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

	// Build the changeset target entry
	let mut cs_target = Map::new();
	cs_target.insert("id".to_string(), json!(format!("{}_0", variant.owner)));
	cs_target.insert("kind".to_string(), json!("package"));
	cs_target.insert(
		"origin".to_string(),
		json!(format!(".changeset/schema-artifact-{fixture_index}.md")),
	);
	if let Some(ref bump) = variant.bump {
		cs_target.insert("bump".to_string(), json!(bump));
	}
	if let Some(ref change_type) = variant.change_type {
		cs_target.insert("changeType".to_string(), json!(change_type));
	}
	if variant.with_caused_by {
		cs_target.insert(
			"causedBy".to_string(),
			json!([format!("{}-schema-compat", variant.owner)]),
		);
	}
	cs_target.insert(
		"evidenceRefs".to_string(),
		json!([format!("crates/{}_0/src/lib.rs", variant.owner)]),
	);

	changeset.insert("targets".to_string(), json!([Value::Object(cs_target)]));

	if variant.with_changeset_context {
		let mut context = Map::new();
		context.insert("provider".to_string(), json!(&variant.provider_kind));
		context.insert(
			"host".to_string(),
			json!(format!("{}.example.com", variant.provider_kind)),
		);
		changeset.insert("context".to_string(), Value::Object(context));
	}
	root.insert("changesets".to_string(), json!([Value::Object(changeset)]));

	// changelogs — ReleaseManifestChangelog objects
	if variant.with_changelogs {
		let mut changelog = Map::new();
		changelog.insert("ownerId".to_string(), json!("main"));
		changelog.insert("ownerKind".to_string(), json!("group"));
		changelog.insert(
			"path".to_string(),
			json!(format!("fixtures/changelog-{fixture_index}.md")),
		);
		changelog.insert("format".to_string(), json!("monochange"));
		let mut notes = Map::new();
		notes.insert(
			"title".to_string(),
			json!(format!("v{version}.{fixture_index}")),
		);
		notes.insert(
			"summary".to_string(),
			json!([format!("Release {fixture_index}")]),
		);
		notes.insert("sections".to_string(), json!([{ "title": "Bug Fixes", "entries": [format!("fix: schema artifact {fixture_index}")], "collapsed": false }]));
		changelog.insert("notes".to_string(), Value::Object(notes));
		changelog.insert(
			"rendered".to_string(),
			json!(format!(
				"## Bug Fixes\n- fix: schema artifact {fixture_index}"
			)),
		);
		root.insert("changelogs".to_string(), json!([Value::Object(changelog)]));
	} else {
		root.insert("changelogs".to_string(), json!(Vec::<String>::new()));
	}

	// packagePublications
	if variant.with_publications {
		let publications: Vec<Value> = (0..variant.released_package_count)
			.map(|i| {
				let mut pub_entry = Map::new();
				pub_entry.insert("ecosystem".to_string(), json!("cargo"));
				pub_entry.insert(
					"package".to_string(),
					json!(format!("{}_{}", variant.owner, i)),
				);
				pub_entry.insert(
					"version".to_string(),
					json!(format!("{version}.{fixture_index}")),
				);
				Value::Object(pub_entry)
			})
			.collect();
		root.insert("packagePublications".to_string(), json!(publications));
	} else {
		root.insert(
			"packagePublications".to_string(),
			json!(Vec::<String>::new()),
		);
	}

	// deletedChangesets
	root.insert(
		"deletedChangesets".to_string(),
		json!([format!(".changeset/schema-artifact-{fixture_index}.md")]),
	);

	// updatedChangelogs
	root.insert(
		"updatedChangelogs".to_string(),
		json!([format!("fixtures/changelog-{fixture_index}.md")]),
	);

	serde_json::to_string_pretty(&Value::Object(root))
		.unwrap_or_else(|error| panic!("serialize release-record artifact fixture: {error}"))
}

fn config_artifact_fixture(_fixture_index: usize, variant: &ConfigVariant) -> String {
	let mut source = Map::new();
	source.insert("provider".to_string(), json!(&variant.provider_kind));
	source.insert("owner".to_string(), json!(&variant.owner));
	source.insert("repo".to_string(), json!(&variant.repo));
	if variant.with_host {
		source.insert(
			"host".to_string(),
			json!(format!("{}.example.com", variant.provider_kind)),
		);
	}

	let mut defaults = Map::new();
	defaults.insert("package_type".to_string(), json!(&variant.ecosystem));

	let mut packages = Map::new();
	for i in 0..variant.package_count {
		let pkg_name = format!("{}_{}", variant.owner, i);
		let mut pkg = Map::new();
		pkg.insert("path".to_string(), json!(format!("crates/{pkg_name}")));
		packages.insert(pkg_name, Value::Object(pkg));
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
