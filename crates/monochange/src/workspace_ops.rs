use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::time::Instant;

#[cfg(feature = "cargo")]
use monochange_cargo::CargoAdapter;
use monochange_cargo::discover_cargo_packages;
use monochange_config::apply_version_groups;
use monochange_config::build_changeset_load_context;
use monochange_config::load_change_signals;
use monochange_config::load_changeset_contents_with_context;
use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::CliCommandDefinition;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::EcosystemRegistry;
use monochange_core::LockfileCommandDefinition;
use monochange_core::LockfileCommandExecution;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::PackageType;
use monochange_core::ReleasePlan;
use monochange_core::SourceConfiguration;
use monochange_core::default_cli_commands;
#[cfg(feature = "dart")]
use monochange_dart::DartAdapter;
use monochange_deno::DenoAdapter;
use monochange_go::GoAdapter;
#[cfg(feature = "npm")]
use monochange_npm::NpmAdapter;
use monochange_python::PythonAdapter;
use serde_json::json;
use tokio::task::JoinHandle;
use typed_builder::TypedBuilder;

use crate::interactive;
use crate::*;

/// Result of initializing a workspace with `mc init`.
///
/// Contains the paths to generated configuration and workflow files.
pub(crate) struct InitWorkspaceResult {
	/// Path to the generated monochange.toml configuration file
	pub config_path: PathBuf,
	/// Paths to any generated workflow files (e.g., GitHub Actions)
	pub workflow_paths: Vec<PathBuf>,
}

impl InitWorkspaceResult {
	/// Returns a human-readable summary of what was written
	pub fn summary(&self) -> String {
		let mut lines = vec![format!("wrote {}", self.config_path.display())];
		for path in &self.workflow_paths {
			lines.push(format!("wrote {}", path.display()));
		}
		lines.join("\n")
	}
}

/// Initialize a new monochange workspace.
///
/// Generates a starter `monochange.toml` with detected packages and groups.
/// When `provider` is specified, also configures source provider automation
/// and generates workflow files appropriate for that provider.
///
/// # Arguments
///
/// * `root` - Repository root directory
/// * `force` - Overwrite existing configuration if true
/// * `provider` - Optional source provider ("github", "gitlab", or "gitea")
///
/// # Errors
///
/// Returns an error if:
/// * Configuration already exists and force is false
/// * Writing configuration or workflow files fails
#[must_use = "the initialization result must be checked"]
pub(crate) fn init_workspace(
	root: &Path,
	force: bool,
	provider: Option<&str>,
) -> MonochangeResult<InitWorkspaceResult> {
	let path = monochange_config::config_path(root);

	if path.exists() && !force {
		return Err(MonochangeError::Config(format!(
			"{} already exists; rerun with --force to overwrite it",
			path.display()
		)));
	}

	let remote = provider.and_then(|_| detect_remote_owner_repo(root));
	let content = render_annotated_init_config(root, provider, remote.as_ref())?;

	fs::write(&path, &content).map_err(|error| {
		MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
	})?;

	let workflow_paths = if provider == Some("github") {
		write_github_workflows(root)?
	} else {
		Vec::new()
	};

	Ok(InitWorkspaceResult {
		config_path: path,
		workflow_paths,
	})
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct RemoteInfo {
	pub owner: String,
	pub repo: String,
}

/// Parse the git remote URL to extract owner and repo.
///
/// Supports SSH (`git@host:owner/repo.git`) and HTTPS
/// (`https://host/owner/repo.git`) formats.
pub(crate) fn detect_remote_owner_repo(root: &Path) -> Option<RemoteInfo> {
	let output = ProcessCommand::new("git")
		.current_dir(root)
		.args(["remote", "get-url", "origin"])
		.output()
		.ok()?;
	if !output.status.success() {
		return None;
	}
	let url = String::from_utf8(output.stdout).ok()?.trim().to_string();
	parse_remote_url(&url)
}

pub(crate) fn parse_remote_url(url: &str) -> Option<RemoteInfo> {
	// Extract the "owner/repo" portion from various URL formats.
	let owner_repo = if let Some(rest) = url.strip_prefix("git@") {
		// git@github.com:owner/repo.git
		rest.split_once(':').map(|(_, path)| path.to_string())
	} else if url.starts_with("https://") || url.starts_with("http://") {
		// https://github.com/owner/repo.git
		url.split_once("//")
			.and_then(|(_, rest)| rest.split_once('/'))
			.map(|(_, path)| path.to_string())
	} else if url.starts_with("ssh://") {
		// ssh://git@github.com/owner/repo.git
		url.strip_prefix("ssh://")
			.and_then(|rest| rest.split_once('/'))
			.map(|(_, path)| path.to_string())
	} else {
		None
	}?;

	let owner_repo = owner_repo.strip_suffix(".git").unwrap_or(&owner_repo);
	let (owner, repo) = owner_repo.split_once('/')?;

	if owner.is_empty() || repo.is_empty() || repo.contains('/') {
		return None;
	}

	Some(RemoteInfo {
		owner: owner.to_string(),
		repo: repo.to_string(),
	})
}

const CHANGESET_POLICY_WORKFLOW: &str = include_str!("templates/changeset-policy.yml");
const RELEASE_WORKFLOW: &str = include_str!("templates/release.yml");

fn write_github_workflows(root: &Path) -> MonochangeResult<Vec<PathBuf>> {
	let workflows_dir = root.join(".github/workflows");
	fs::create_dir_all(&workflows_dir).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create {}: {error}",
			workflows_dir.display()
		))
	})?;

	let mut paths = Vec::new();
	for (name, content) in [
		("changeset-policy.yml", CHANGESET_POLICY_WORKFLOW),
		("release.yml", RELEASE_WORKFLOW),
	] {
		let path = workflows_dir.join(name);
		fs::write(&path, content).map_err(|error| {
			MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
		})?;
		paths.push(path);
	}

	Ok(paths)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PopulateWorkspaceResult {
	pub path: PathBuf,
	pub added_commands: Vec<String>,
}

#[must_use = "the population result must be checked"]
pub(crate) fn populate_workspace(root: &Path) -> MonochangeResult<PopulateWorkspaceResult> {
	let path = monochange_config::config_path(root);

	if !path.exists() {
		return Err(MonochangeError::Config(format!(
			"{} does not exist; run `mc init` first or create a monochange.toml before running `mc populate`",
			path.display()
		)));
	}

	let contents = match fs::read_to_string(&path) {
		Ok(contents) => contents,
		Err(error) => {
			return Err(MonochangeError::Io(format!(
				"failed to read {}: {error}",
				path.display()
			)));
		}
	};

	let existing = existing_cli_command_names(&contents, &path)?;
	let missing = default_cli_commands()
		.into_iter()
		.filter(|command| !existing.contains(&command.name))
		.collect::<Vec<_>>();

	if missing.is_empty() {
		return Ok(PopulateWorkspaceResult {
			path,
			added_commands: Vec::new(),
		});
	}

	let mut updated = contents.trim_end().to_string();
	if !updated.is_empty() {
		updated.push_str("\n\n");
	}
	updated.push_str(&render_cli_commands_toml(&missing));
	updated.push('\n');

	if let Err(error) = fs::write(&path, updated) {
		return Err(MonochangeError::Io(format!(
			"failed to write {}: {error}",
			path.display()
		)));
	}

	Ok(PopulateWorkspaceResult {
		path,
		added_commands: missing.into_iter().map(|command| command.name).collect(),
	})
}

fn existing_cli_command_names(contents: &str, path: &Path) -> MonochangeResult<BTreeSet<String>> {
	if contents.trim().is_empty() {
		return Ok(BTreeSet::new());
	}

	let document = toml::from_str::<toml::Value>(contents).map_err(|error| {
		MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
	})?;

	Ok(document
		.get("cli")
		.and_then(toml::Value::as_table)
		.map(|table| table.keys().cloned().collect())
		.unwrap_or_default())
}

pub(crate) fn render_cli_commands_toml(commands: &[CliCommandDefinition]) -> String {
	let mut rendered = String::new();
	for (index, command) in commands.iter().enumerate() {
		if index > 0 {
			rendered.push_str("\n\n");
		}
		render_cli_command_toml(&mut rendered, command);
	}
	rendered
}

fn render_cli_command_toml(rendered: &mut String, command: &CliCommandDefinition) {
	writeln!(rendered, "[cli.{}]", command.name)
		.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
	if let Some(help_text) = &command.help_text {
		write_toml_key_value(rendered, "help_text", &render_toml_string(help_text));
	}
	for input in &command.inputs {
		rendered.push('\n');
		writeln!(rendered, "[[cli.{}.inputs]]", command.name)
			.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
		render_cli_input_toml(rendered, input);
	}
	for step in &command.steps {
		rendered.push('\n');
		writeln!(rendered, "[[cli.{}.steps]]", command.name)
			.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
		render_cli_step_toml(rendered, step);
	}
}

fn write_toml_key_value(rendered: &mut String, key: &str, value: &str) {
	writeln!(rendered, "{key} = {value}")
		.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
}

fn render_cli_input_toml(rendered: &mut String, input: &monochange_core::CliInputDefinition) {
	write_toml_key_value(rendered, "name", &render_toml_string(&input.name));
	write_toml_key_value(
		rendered,
		"type",
		&render_toml_string(match input.kind {
			monochange_core::CliInputKind::String => "string",
			monochange_core::CliInputKind::StringList => "string_list",
			monochange_core::CliInputKind::Path => "path",
			monochange_core::CliInputKind::Choice => "choice",
			monochange_core::CliInputKind::Boolean => "boolean",
		}),
	);
	input.help_text.iter().for_each(|help_text| {
		write_toml_key_value(rendered, "help_text", &render_toml_string(help_text));
	});
	if input.required {
		write_toml_key_value(rendered, "required", "true");
	}
	if let Some(default) = &input.default {
		write_toml_key_value(rendered, "default", &render_toml_string(default));
	}
	if !input.choices.is_empty() {
		write_toml_key_value(rendered, "choices", &render_toml_array(&input.choices));
	}
	if let Some(short) = input.short {
		write_toml_key_value(rendered, "short", &render_toml_string(&short.to_string()));
	}
}

fn render_cli_step_toml(rendered: &mut String, step: &CliStepDefinition) {
	let step_type = step.kind_name();
	writeln!(rendered, "type = {}", render_toml_string(step_type))
		.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
	if let Some(when) = step.when() {
		writeln!(rendered, "when = {}", render_toml_string(when))
			.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
	}
	match step {
		CliStepDefinition::Command {
			command,
			dry_run_command,
			shell,
			id,
			variables,
			inputs,
			..
		} => {
			writeln!(rendered, "command = {}", render_toml_string(command))
				.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
			if let Some(dry_run_command) = dry_run_command {
				writeln!(
					rendered,
					"dry_run_command = {}",
					render_toml_string(dry_run_command)
				)
				.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
			}
			match shell {
				monochange_core::ShellConfig::None => {}
				monochange_core::ShellConfig::Default => {
					writeln!(rendered, "shell = true")
						.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
				}
				monochange_core::ShellConfig::Custom(shell) => {
					writeln!(rendered, "shell = {}", render_toml_string(shell))
						.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
				}
			}
			if let Some(id) = id {
				write_toml_key_value(rendered, "id", &render_toml_string(id));
			}
			if let Some(variables) = variables {
				writeln!(
					rendered,
					"variables = {}",
					render_command_variables_inline_table(variables)
				)
				.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
			}
			render_step_inputs_toml(rendered, inputs);
		}
		_ => {
			render_step_inputs_toml(rendered, step.inputs());
		}
	}
}

fn render_step_inputs_toml(
	rendered: &mut String,
	inputs: &BTreeMap<String, monochange_core::CliStepInputValue>,
) {
	if inputs.is_empty() {
		return;
	}
	let rendered_inputs = if inputs
		.values()
		.all(|value| matches!(value, monochange_core::CliStepInputValue::Inherited))
	{
		render_toml_array(&inputs.keys().cloned().collect::<Vec<_>>())
	} else {
		render_step_inputs_inline_table(inputs)
	};
	writeln!(rendered, "inputs = {rendered_inputs}")
		.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
}

fn render_step_inputs_inline_table(
	inputs: &BTreeMap<String, monochange_core::CliStepInputValue>,
) -> String {
	format!(
		"{{ {} }}",
		inputs
			.iter()
			.map(|(name, value)| format!("{name} = {}", render_step_input_value(name, value)))
			.collect::<Vec<_>>()
			.join(", ")
	)
}

fn render_step_input_value(name: &str, value: &monochange_core::CliStepInputValue) -> String {
	match value {
		monochange_core::CliStepInputValue::Inherited => {
			render_toml_string(&format!("{{{{ inputs.{name} }}}}"))
		}
		monochange_core::CliStepInputValue::String(value) => render_toml_string(value),
		monochange_core::CliStepInputValue::Boolean(value) => value.to_string(),
		monochange_core::CliStepInputValue::List(values) => render_toml_array(values),
	}
}

fn render_command_variables_inline_table(
	variables: &BTreeMap<String, monochange_core::CommandVariable>,
) -> String {
	format!(
		"{{ {} }}",
		variables
			.iter()
			.map(|(name, value)| {
				format!(
					"{name} = {}",
					render_toml_string(match value {
						monochange_core::CommandVariable::Version => "version",
						monochange_core::CommandVariable::GroupVersion => "group_version",
						monochange_core::CommandVariable::ReleasedPackages => "released_packages",
						monochange_core::CommandVariable::ChangedFiles => "changed_files",
						monochange_core::CommandVariable::Changesets => "changesets",
					})
				)
			})
			.collect::<Vec<_>>()
			.join(", ")
	)
}

fn render_toml_array(values: &[String]) -> String {
	format!(
		"[{}]",
		values
			.iter()
			.map(|value| render_toml_string(value))
			.collect::<Vec<_>>()
			.join(", ")
	)
}

fn render_toml_string(value: &str) -> String {
	toml::Value::String(value.to_string()).to_string()
}

/// The minijinja template for `mc init`, loaded at compile time.
///
/// SYNC: when configuration options are added, removed, or changed in
/// `monochange_core` or `monochange_config`, update `monochange.toml.template`
/// to document the new options.  See the `product-rules.md` agent rule
/// "keep init template in sync".
const INIT_TEMPLATE: &str = include_str!("monochange.toml.template");

/// Render a fully annotated `monochange.toml` from the init template with
/// discovered packages injected as context.
fn render_annotated_init_config(
	root: &Path,
	provider: Option<&str>,
	remote: Option<&RemoteInfo>,
) -> MonochangeResult<String> {
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
			PackageType::Python => "python",
			PackageType::Go => "go",
			_ => unreachable!(),
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
	let has_python = packages.iter().any(|p| p.ecosystem == Ecosystem::Python);
	let has_go = packages.iter().any(|p| p.ecosystem == Ecosystem::Go);

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
		"has_python": has_python,
		"has_go": has_go,
		"provider": provider.unwrap_or(""),
		"owner": remote.map_or("your-org", |r| r.owner.as_str()),
		"repo": remote.map_or("your-repo", |r| r.repo.as_str()),
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

fn build_ecosystem_registry() -> EcosystemRegistry {
	let mut registry = EcosystemRegistry::new();
	#[cfg(feature = "cargo")]
	registry.push_adapter(Box::new(CargoAdapter));
	#[cfg(feature = "npm")]
	registry.push_adapter(Box::new(NpmAdapter));
	#[cfg(feature = "deno")]
	registry.push_adapter(Box::new(DenoAdapter));
	#[cfg(feature = "dart")]
	registry.push_adapter(Box::new(DartAdapter));
	#[cfg(feature = "python")]
	registry.push_adapter(Box::new(PythonAdapter));
	#[cfg(feature = "go")]
	registry.push_adapter(Box::new(GoAdapter));
	registry
}

fn discover_packages(root: &Path) -> MonochangeResult<Vec<PackageRecord>> {
	let result = build_ecosystem_registry().discover_all(root)?;
	let mut packages = result.packages;

	normalize_package_ids(root, &mut packages);
	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	Ok(packages)
}

fn normalize_package_ids(root: &Path, packages: &mut [PackageRecord]) {
	for package in packages {
		let Some(relative_manifest) = relative_to_root(root, &package.manifest_path) else {
			continue;
		};
		package.id = format!(
			"{}:{}",
			package.ecosystem.as_str(),
			relative_manifest.display()
		);
	}
}

fn detect_default_changelog(root: &Path, manifest_dir: &Path) -> Option<PathBuf> {
	let candidates = [
		manifest_dir.join("CHANGELOG.md"),
		manifest_dir.join("changelog.md"),
	];

	for candidate in candidates {
		if candidate.exists() {
			return Some(root_relative(root, &candidate));
		}
	}

	None
}

#[allow(clippy::match_same_arms)]
fn package_type_for_ecosystem(ecosystem: Ecosystem) -> PackageType {
	match ecosystem {
		Ecosystem::Cargo => PackageType::Cargo,
		Ecosystem::Npm => PackageType::Npm,
		Ecosystem::Deno => PackageType::Deno,
		Ecosystem::Dart => PackageType::Dart,
		Ecosystem::Flutter => PackageType::Flutter,
		Ecosystem::Python => PackageType::Python,
		Ecosystem::Go => PackageType::Go,
		_ => PackageType::Cargo,
	}
}

#[test]
fn package_type_for_ecosystem_maps_python() {
	assert_eq!(
		package_type_for_ecosystem(Ecosystem::Python),
		PackageType::Python
	);
	assert_eq!(PackageType::Python.as_str(), "python");
	assert_eq!(package_type_for_ecosystem(Ecosystem::Go), PackageType::Go);
	assert_eq!(PackageType::Go.as_str(), "go");
}

#[test]
fn render_annotated_init_config_includes_go_package_type() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(
		root.join("go.mod"),
		"module github.com/example/app\n\ngo 1.22\n",
	)
	.unwrap_or_else(|error| panic!("write go.mod: {error}"));

	let rendered = render_annotated_init_config(root, None, None)
		.unwrap_or_else(|error| panic!("render init config: {error}"));

	assert!(rendered.contains("type = \"go\""));
}

#[test]
fn render_annotated_init_config_includes_python_package_type() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(
		root.join("pyproject.toml"),
		"[project]\nname = \"python-app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write pyproject: {error}"));

	let rendered = render_annotated_init_config(root, None, None)
		.unwrap_or_else(|error| panic!("render init config: {error}"));
	assert!(rendered.contains("type = \"python\""), "{rendered}");
}

// patch-coverage:ignore-start -- lockfile command availability matrix depends on optional ecosystem features and is covered by release tests.
pub(crate) fn build_lockfile_command_executions(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<LockfileCommandExecution>> {
	let released_versions = released_versions_by_record_id(plan);
	#[cfg(feature = "cargo")]
	warn_about_incomplete_cargo_lockfiles(root, configuration, packages, &released_versions);
	#[cfg(feature = "cargo")]
	let cargo_executions = resolve_lockfile_command_executions(
		root,
		&configuration.cargo.lockfile_commands,
		packages.iter().any(|package| {
			package.ecosystem == Ecosystem::Cargo && released_versions.contains_key(&package.id)
		}),
	)?;
	#[cfg(feature = "npm")]
	let npm_executions = resolve_lockfile_command_executions(
		root,
		&configuration.npm.lockfile_commands,
		packages.iter().any(|package| {
			package.ecosystem == Ecosystem::Npm && released_versions.contains_key(&package.id)
		}),
	)?;
	#[cfg(feature = "deno")]
	let deno_executions = resolve_lockfile_command_executions(
		root,
		&configuration.deno.lockfile_commands,
		packages.iter().any(|package| {
			package.ecosystem == Ecosystem::Deno && released_versions.contains_key(&package.id)
		}),
	)?;
	#[cfg(feature = "dart")]
	let dart_executions = resolve_lockfile_command_executions(
		root,
		&configuration.dart.lockfile_commands,
		packages.iter().any(|package| {
			matches!(package.ecosystem, Ecosystem::Dart | Ecosystem::Flutter)
				&& released_versions.contains_key(&package.id)
		}),
	)?;
	#[cfg(feature = "python")]
	let python_executions = resolve_lockfile_command_executions(
		root,
		&configuration.python.lockfile_commands,
		packages.iter().any(|package| {
			package.ecosystem == Ecosystem::Python && released_versions.contains_key(&package.id)
		}),
	)?;
	#[cfg(feature = "go")]
	let go_executions = resolve_lockfile_command_executions(
		root,
		&configuration.go.lockfile_commands,
		packages.iter().any(|package| {
			package.ecosystem == Ecosystem::Go && released_versions.contains_key(&package.id)
		}),
	)?;
	let mut executions = Vec::new();
	#[cfg(feature = "cargo")]
	executions.extend(cargo_executions);
	#[cfg(feature = "npm")]
	executions.extend(npm_executions);
	#[cfg(feature = "deno")]
	executions.extend(deno_executions);
	#[cfg(feature = "dart")]
	executions.extend(dart_executions);
	#[cfg(feature = "python")]
	executions.extend(python_executions);
	#[cfg(feature = "go")]
	executions.extend(go_executions);
	Ok(dedup_lockfile_command_executions(executions))
}
// patch-coverage:ignore-end

#[cfg(feature = "cargo")]
fn warn_about_incomplete_cargo_lockfiles(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	released_versions: &BTreeMap<String, String>,
) {
	if !configuration.cargo.lockfile_commands.is_empty() {
		return;
	}
	let released_packages = packages
		.iter()
		.filter(|package| {
			package.ecosystem == Ecosystem::Cargo && released_versions.contains_key(&package.id)
		})
		.collect::<Vec<_>>();
	if released_packages.is_empty() {
		return;
	}
	let cargo_packages = packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Cargo)
		.collect::<Vec<_>>();
	let mut warned_lockfiles = BTreeSet::new();
	for package in released_packages {
		for lockfile in monochange_cargo::discover_lockfiles(package) {
			let shared_packages = cargo_packages
				.iter()
				.copied()
				.filter(|candidate| {
					monochange_cargo::discover_lockfiles(candidate).contains(&lockfile)
				})
				.collect::<Vec<_>>();
			if !monochange_cargo::lockfile_requires_command_refresh(&lockfile, &shared_packages) {
				continue;
			}
			let relative_lockfile = root_relative(root, &lockfile);
			if warned_lockfiles.insert(relative_lockfile.clone()) {
				eprintln!(
					"warning: `{}` still looks incomplete after monochange rewrote it directly; run `cargo generate-lockfile`, `cargo check`, or configure `[ecosystems.cargo].lockfile_commands` if you want cargo to refresh it automatically",
					relative_lockfile.display()
				);
			}
		}
	}
}

fn resolve_lockfile_command_executions(
	root: &Path,
	configured_commands: &[LockfileCommandDefinition],
	has_released_packages: bool,
) -> MonochangeResult<Vec<LockfileCommandExecution>> {
	if !has_released_packages || configured_commands.is_empty() {
		return Ok(Vec::new());
	}
	configured_commands
		.iter()
		.map(|command| {
			let cwd = command
				.cwd
				.as_ref()
				.map_or_else(|| root.to_path_buf(), |cwd| resolve_config_path(root, cwd));
			Ok(LockfileCommandExecution {
				command: command.command.clone(),
				cwd,
				shell: command.shell.clone(),
			})
		})
		.collect()
}

fn dedup_lockfile_command_executions(
	executions: Vec<LockfileCommandExecution>,
) -> Vec<LockfileCommandExecution> {
	let mut seen = BTreeSet::new();
	let mut deduped = Vec::new();

	for execution in executions {
		let key = format!(
			"{}::{:?}::{}",
			execution.cwd.display(),
			execution.shell,
			execution.command,
		);

		if seen.insert(key) {
			deduped.push(execution);
		}
	}

	deduped
}

#[must_use = "the validation result must be checked"]
#[cfg(feature = "cargo")]
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

/// Discover all supported packages, dependency edges, and configured groups.
#[tracing::instrument(skip_all)]
#[must_use = "the discovery result must be checked"]
pub fn discover_workspace(root: &Path) -> MonochangeResult<DiscoveryReport> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = build_ecosystem_registry().discover_all(root)?;
	let mut warnings = discovery.warnings;
	let mut packages = discovery.packages;
	normalize_package_ids(root, &mut packages);
	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	let (version_groups, version_group_warnings) =
		apply_version_groups(&mut packages, &configuration)?;
	warnings.extend(version_group_warnings);
	let dependencies = materialize_dependency_edges(&packages);
	tracing::info!(
		packages = packages.len(),
		warnings = warnings.len(),
		"workspace discovery complete"
	);

	Ok(DiscoveryReport {
		workspace_root: root.to_path_buf(),
		packages,
		dependencies,
		version_groups,
		warnings,
	})
}

/// Discover just the packages that release planning actually needs.
///
/// Performance note:
/// the generic discovery command intentionally walks the full repository so
/// `mc discover` can surface every supported package it finds. That behavior is
/// expensive in monochange's own repo because fixtures contain many extra
/// manifests across multiple ecosystems. Release planning already has explicit
/// package definitions, so re-running whole-repo discovery turned `mc release
/// --dry-run` into mostly filesystem scanning.
///
/// This helper keeps the broad `discover_workspace()` behavior for discovery-
/// oriented commands while giving release planning a path that parses only the
/// configured package manifests. The comment is intentionally explicit so future
/// refactors do not “simplify” the code back to a full repo walk.
fn discover_release_workspace(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
) -> MonochangeResult<DiscoveryReport> {
	if configuration.packages.is_empty() {
		return discover_workspace(root);
	}

	use rayon::prelude::*;

	let registry = build_ecosystem_registry();
	let mut packages = configuration
		.packages
		.par_iter()
		.map(|package_definition| {
			let path = root.join(&package_definition.path);
			registry
				.load_configured(root, &path, package_definition.package_type.into())?
				.ok_or_else(|| {
					MonochangeError::Discovery(format!(
						"configured package `{}` at {} could not be discovered",
						package_definition.id,
						package_definition.path.display()
					))
				})
		})
		.collect::<MonochangeResult<Vec<_>>>()?;

	normalize_package_ids(root, &mut packages);
	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);
	let (version_groups, warnings) = apply_version_groups(&mut packages, configuration)?;
	let dependencies = materialize_dependency_edges(&packages);
	Ok(DiscoveryReport {
		workspace_root: root.to_path_buf(),
		packages,
		dependencies,
		version_groups,
		warnings,
	})
}

/// Parameters for creating a `.changeset/*.md` file through the library API.
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
	pub caused_by: &'a [String],
	#[builder(default)]
	pub details: Option<&'a str>,
	#[builder(default)]
	pub output: Option<&'a Path>,
}

/// Create a changeset markdown file for one or more package or group ids.
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
		request.caused_by,
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
	_target_id: &str,
	change_type: &str,
) -> Option<BumpSeverity> {
	let changelog = &configuration.changelog;
	changelog.types.get(change_type).map(|typ| typ.bump)
}

fn render_changeset_target_key(target_id: &str) -> String {
	if target_id
		.chars()
		.all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
	{
		target_id.to_string()
	} else {
		format!(
			"\"{}\"",
			target_id.replace('\\', "\\\\").replace('"', "\\\"")
		)
	}
}

pub(crate) fn render_change_target_markdown(
	configuration: &monochange_core::WorkspaceConfiguration,
	target_id: &str,
	bump: BumpSeverity,
	version: Option<&str>,
	change_type: Option<&str>,
	caused_by: &[String],
) -> MonochangeResult<Vec<String>> {
	if change_type.is_none()
		&& version.is_none()
		&& bump == BumpSeverity::None
		&& caused_by.is_empty()
	{
		return Err(MonochangeError::Config(format!(
			"target `{target_id}` must not use a `none` bump without also declaring `type`, `version`, or `caused_by`"
		)));
	}

	let mut lines = Vec::new();
	let target_key = render_changeset_target_key(target_id);
	let caused_by = caused_by
		.iter()
		.map(|reference| {
			format!(
				"\"{}\"",
				reference.replace('\\', "\\\\").replace('"', "\\\"")
			)
		})
		.collect::<Vec<_>>();
	let forced_object_syntax = !caused_by.is_empty();

	// Handle explicit change type
	if let Some(change_type) = change_type.filter(|value| !value.trim().is_empty()) {
		let default_bump = change_type_default_bump(configuration, target_id, change_type)
			.ok_or_else(|| {
				MonochangeError::Config(format!(
					"target `{target_id}` uses unknown change type `{change_type}`"
				))
			})?;

		if !forced_object_syntax && version.is_none() && bump == default_bump {
			lines.push(format!("{target_key}: {change_type}"));
			return Ok(lines);
		}

		lines.push(format!("{target_key}:"));
		if bump != BumpSeverity::None {
			lines.push(format!("  bump: {bump}"));
		}
		lines.push(format!("  type: {change_type}"));
		if let Some(version) = version {
			lines.push(format!("  version: \"{version}\""));
		}
		if !caused_by.is_empty() {
			lines.push(format!("  caused_by: [{}]", caused_by.join(", ")));
		}
		return Ok(lines);
	}

	if let Some(version) = version {
		lines.push(format!("{target_key}:"));
		if bump != BumpSeverity::None {
			lines.push(format!("  bump: {bump}"));
		}
		lines.push(format!("  version: \"{version}\""));
		if !caused_by.is_empty() {
			lines.push(format!("  caused_by: [{}]", caused_by.join(", ")));
		}
		return Ok(lines);
	}

	if !caused_by.is_empty() {
		lines.push(format!("{target_key}:"));
		lines.push(format!("  bump: {bump}"));
		lines.push(format!("  caused_by: [{}]", caused_by.join(", ")));
		return Ok(lines);
	}

	lines.push(format!("{target_key}: {bump}"));

	Ok(lines)
}

fn render_interactive_target_markdown(
	configuration: &monochange_core::WorkspaceConfiguration,
	target: &interactive::InteractiveTarget,
	caused_by: &[String],
) -> MonochangeResult<Vec<String>> {
	render_change_target_markdown(
		configuration,
		&target.id,
		target.bump,
		target.version.as_deref(),
		target.change_type.as_deref(),
		caused_by,
	)
}

pub(crate) fn render_interactive_changeset_markdown(
	configuration: &monochange_core::WorkspaceConfiguration,
	result: &interactive::InteractiveChangeResult,
) -> MonochangeResult<String> {
	let mut lines = vec!["---".to_string()];

	for target in &result.targets {
		let target_lines =
			render_interactive_target_markdown(configuration, target, &result.caused_by)?;
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

/// Build a release plan from a single changeset file.
#[must_use = "the release plan result must be checked"]
pub fn plan_release(root: &Path, changes_path: &Path) -> MonochangeResult<ReleasePlan> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;
	let change_signals = load_change_signals(changes_path, &configuration, &discovery.packages)?;
	build_release_plan_from_signals(&configuration, &discovery, &change_signals)
}

#[tracing::instrument(skip_all)]
fn materialize_lockfile_command_updates(
	root: &Path,
	base_updates: &[FileUpdate],
	lockfile_commands: &[LockfileCommandExecution],
) -> MonochangeResult<Vec<FileUpdate>> {
	// Snapshot lockfile-adjacent files before running commands so we can
	// detect what the commands changed.
	let lockfile_dirs: Vec<PathBuf> = lockfile_commands
		.iter()
		.map(|cmd| cmd.cwd.clone())
		.collect();
	let mut before_snapshots = BTreeMap::new();
	for dir in &lockfile_dirs {
		let full_dir = root.join(dir);
		if full_dir.is_dir() {
			snapshot_directory_files(root, &full_dir, &mut before_snapshots)?;
		}
	}

	// Apply version updates in-place so lockfile commands see updated manifests.
	apply_file_updates(base_updates)?;

	// Run lockfile commands in the real workspace.
	for command in lockfile_commands {
		run_lockfile_command_in_place(root, command)?;
	}

	// Snapshot the same directories after commands ran.
	let mut after_snapshots = BTreeMap::new();
	for dir in &lockfile_dirs {
		let full_dir = root.join(dir);
		if full_dir.is_dir() {
			snapshot_directory_files(root, &full_dir, &mut after_snapshots)?;
		}
	}

	// Collect all updates: base updates + any lockfile changes.
	let mut all_updates = base_updates.to_vec();
	for (relative_path, after_content) in &after_snapshots {
		let before = before_snapshots.get(relative_path);
		if before != Some(after_content) {
			all_updates.push(FileUpdate {
				path: root.join(relative_path),
				content: after_content.clone(),
			});
		}
	}
	all_updates.sort_by(|a, b| a.path.cmp(&b.path));
	all_updates.dedup_by(|a, b| a.path == b.path);
	Ok(all_updates)
}

fn snapshot_directory_files(
	root: &Path,
	dir: &Path,
	snapshots: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> MonochangeResult<()> {
	let entries = fs::read_dir(dir).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", dir.display()))
	})?;

	for entry in entries {
		let entry = entry
			.map_err(|error| MonochangeError::Io(format!("directory entry error: {error}")))?;
		let path = entry.path();

		if path.is_file() {
			let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
			let content = fs::read(&path).map_err(|error| {
				MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
			})?;
			snapshots.insert(relative, content);
		}
	}

	Ok(())
}

/// Run a lockfile command directly in the workspace (no temp copy).
fn run_lockfile_command_in_place(
	root: &Path,
	command: &LockfileCommandExecution,
) -> MonochangeResult<()> {
	let cwd = root.join(&command.cwd);
	let output = if let Some(shell_binary) = command.shell.shell_binary() {
		ProcessCommand::new(shell_binary)
			.arg("-c")
			.arg(&command.command)
			.current_dir(&cwd)
			.output()
	} else {
		let parts = shlex::split(&command.command).ok_or_else(|| {
			MonochangeError::Config(format!("failed to parse command `{}`", command.command))
		})?;
		let Some((program, args)) = parts.split_first() else {
			return Err(MonochangeError::Config(
				"lockfile command must not be empty".to_string(),
			));
		};
		ProcessCommand::new(program)
			.args(args)
			.current_dir(&cwd)
			.output()
	};

	let output = output.map_err(|error| {
		MonochangeError::Io(format!(
			"failed to run lockfile command `{}` in {}: {error}",
			command.command,
			root_relative(root, &command.cwd).display(),
		))
	})?;

	if output.status.success() {
		return Ok(());
	}

	let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
	let details = if stderr.is_empty() {
		format!("exit status {}", output.status)
	} else {
		stderr
	};

	Err(MonochangeError::Config(format!(
		"lockfile command `{}` failed in {}: {details}",
		command.command,
		root_relative(root, &command.cwd).display(),
	)))
}

#[cfg(test)]
use monochange_test_helpers::workspace_ops::collect_workspace_files;
#[cfg(test)]
use monochange_test_helpers::workspace_ops::copy_workspace_file;
#[cfg(test)]
use monochange_test_helpers::workspace_ops::copy_workspace_tree;
#[cfg(test)]
use monochange_test_helpers::workspace_ops::ensure_parent_directory;
#[cfg(test)]
use monochange_test_helpers::workspace_ops::read_optional_file;
#[cfg(test)]
use monochange_test_helpers::workspace_ops::remap_workspace_path;
#[cfg(test)]
use monochange_test_helpers::workspace_ops::run_lockfile_command;
#[cfg(test)]
use monochange_test_helpers::workspace_ops::strip_workspace_prefix;

#[cfg(test)]
fn collect_workspace_file_updates(
	root: &Path,
	temp_root: &Path,
	base_updates: &[FileUpdate],
	lockfile_commands: &[LockfileCommandExecution],
) -> MonochangeResult<Vec<FileUpdate>> {
	let normalized_root = monochange_core::normalize_path(root);
	let mut dirs_to_scan = BTreeSet::new();

	for update in base_updates {
		if let Some(parent) = update.path.parent() {
			let normalized = monochange_core::normalize_path(parent);
			if let Ok(relative) = normalized.strip_prefix(&normalized_root) {
				dirs_to_scan.insert(relative.to_path_buf());
			}
		}
	}

	for command in lockfile_commands {
		let normalized = monochange_core::normalize_path(&command.cwd);
		if let Ok(relative) = normalized.strip_prefix(&normalized_root) {
			dirs_to_scan.insert(relative.to_path_buf());
		} else {
			dirs_to_scan.insert(command.cwd.clone());
		}
	}

	// Always scan the changeset directory (files are deleted during release).
	dirs_to_scan.insert(PathBuf::from(".changeset"));

	let mut relative_paths = BTreeSet::new();

	for dir in &dirs_to_scan {
		let original_dir = root.join(dir);
		let temp_dir = temp_root.join(dir);
		if original_dir.is_dir() {
			collect_workspace_files(root, &original_dir, &mut relative_paths)?;
		}
		if temp_dir.is_dir() {
			collect_workspace_files(temp_root, &temp_dir, &mut relative_paths)?;
		}
	}

	let mut updates = Vec::new();

	for relative in relative_paths {
		let before = read_optional_file(&root.join(&relative))?;
		let after = read_optional_file(&temp_root.join(&relative))?;
		if let Some(content) = after.filter(|content| before.as_ref() != Some(content)) {
			updates.push(FileUpdate {
				path: root.join(relative),
				content,
			});
		}
	}

	updates.sort_by(|left, right| left.path.cmp(&right.path));

	Ok(updates)
}

/// Prepare a release, update versioned files, and return the structured result.
#[must_use = "the prepared release result must be checked"]
pub async fn prepare_release(root: &Path, dry_run: bool) -> MonochangeResult<PreparedRelease> {
	// The public API returns structured release state, not rendered diffs.
	// Building unified diffs for large lockfiles can dominate wall time, so skip
	// that work here unless a caller explicitly asks for the richer execution
	// report via `prepare_release_execution`.
	prepare_release_execution_with_file_diffs(root, dry_run, false, false)
		.await
		.map(|execution| execution.prepared_release)
}

#[cfg(test)]
#[tracing::instrument(skip_all, fields(dry_run))]
pub(crate) async fn prepare_release_execution(
	root: &Path,
	dry_run: bool,
) -> MonochangeResult<PreparedReleaseExecution> {
	prepare_release_execution_with_file_diffs(root, dry_run, true, false).await
}

#[tracing::instrument(skip_all, fields(dry_run, build_file_diffs))]
pub(crate) async fn prepare_release_execution_with_file_diffs(
	root: &Path,
	dry_run: bool,
	build_file_diffs: bool,
	allow_empty_changesets: bool,
) -> MonochangeResult<PreparedReleaseExecution> {
	let mut phase_timings = Vec::new();
	let configuration =
		measure_prepare_phase(&mut phase_timings, "load workspace configuration", || {
			load_workspace_configuration(root)
		})?;
	let discovery =
		measure_prepare_phase(&mut phase_timings, "discover release workspace", || {
			discover_release_workspace(root, &configuration)
		})?;
	let changeset_paths =
		measure_prepare_phase(&mut phase_timings, "discover changeset paths", || {
			discover_changeset_paths(root, allow_empty_changesets)
		})?;
	tracing::debug!(count = changeset_paths.len(), "discovered changesets");

	if changeset_paths.is_empty() && allow_empty_changesets {
		return Ok(PreparedReleaseExecution {
			prepared_release: PreparedRelease {
				plan: ReleasePlan {
					workspace_root: root.to_path_buf(),
					decisions: Vec::new(),
					groups: Vec::new(),
					warnings: Vec::new(),
					unresolved_items: Vec::new(),
					compatibility_evidence: Vec::new(),
				},
				changeset_paths,
				changesets: Vec::new(),
				released_packages: Vec::new(),
				package_publications: Vec::new(),
				version: None,
				group_version: None,
				release_targets: Vec::new(),
				changed_files: Vec::new(),
				changelogs: Vec::new(),
				updated_changelogs: Vec::new(),
				deleted_changesets: Vec::new(),
				dry_run,
			},
			file_diffs: Vec::new(),
			phase_timings,
		});
	}

	// Build the shared changeset lookup context once.
	//
	// This command is extremely sensitive to repeated per-file setup work because
	// it often parses every pending `.changeset/*.md` file in the repo. Reusing a
	// single context keeps package/group lookup costs flat instead of multiplying
	// them by the number of changesets.
	let changeset_context = build_changeset_load_context(&configuration, &discovery.packages);
	// Split changeset loading into two phases:
	// 1. read every tiny file in a simple sequential pass
	// 2. parse the already-loaded text in parallel
	//
	// Real-world profiling showed that letting worker threads both open and parse
	// dozens of tiny files inflated wall time because the workload became mostly
	// filesystem contention. Reading eagerly keeps I/O predictable, while the
	// second phase still benefits from parallel parsing once the bytes are in
	// memory.
	let changeset_sources =
		measure_prepare_phase(&mut phase_timings, "read changeset files", || {
			changeset_paths
				.iter()
				.map(|path| Ok((path.clone(), read_changeset_source(path)?)))
				.collect::<MonochangeResult<Vec<_>>>()
		})?;
	let loaded_changesets =
		measure_prepare_phase(&mut phase_timings, "parse changeset files", || {
			use rayon::prelude::*;
			changeset_sources
				.par_iter()
				.map(|(path, contents)| {
					load_changeset_contents_with_context(path, contents, &changeset_context)
				})
				.collect::<MonochangeResult<Vec<_>>>()
		})?;
	let change_signals = loaded_changesets
		.iter()
		.flat_map(|changeset| changeset.signals.clone())
		.collect::<Vec<_>>();
	let prepared_changesets =
		measure_prepare_phase(&mut phase_timings, "build prepared changesets", || {
			Ok(build_prepared_changesets(root, &loaded_changesets))
		})?;
	let mut changesets = Some(prepared_changesets);
	let background_changeset_context =
		configuration
			.source
			.as_ref()
			.filter(|_| !dry_run)
			.map(|source| {
				assert!(changesets.is_some());
				spawn_source_changeset_context_task(
					source.clone(),
					dry_run,
					changesets.take().unwrap_or_default(),
				)
			});
	// patch-coverage:ignore-start -- dry-run hosted-source annotation is covered through adapter-specific tests.
	if let Some(source) = configuration.source.as_ref().filter(|_| dry_run) {
		apply_source_changeset_context_with_timing(
			&mut phase_timings,
			source,
			dry_run,
			changesets
				.as_mut()
				.unwrap_or_else(|| panic!("changesets should exist for dry-run annotation")),
		)
		.await;
	}
	// patch-coverage:ignore-end
	let plan = measure_prepare_phase(&mut phase_timings, "build release plan", || {
		build_release_plan_from_signals(&configuration, &discovery, &change_signals)
	})?;
	let released_packages = released_package_names(&discovery.packages, &plan);
	tracing::debug!(
		count = released_packages.len(),
		"identified released packages"
	);
	if released_packages.is_empty() {
		return Err(MonochangeError::Config(
			"no releaseable packages were found in discovered changesets".to_string(),
		));
	}

	let (
		(changelog_targets_result, manifest_updates_result),
		(versioned_file_updates_result, lockfile_commands_result),
	) = rayon::join(
		|| {
			rayon::join(
				|| {
					capture_prepare_phase("resolve changelog targets", || {
						resolve_changelog_targets(&configuration, &discovery.packages)
					})
				},
				|| {
					capture_prepare_phase("build manifest updates", || {
						build_manifest_updates_parallel(&discovery.packages, &plan)
					})
				},
			)
		},
		|| {
			rayon::join(
				|| {
					capture_prepare_phase("build versioned file updates", || {
						build_versioned_file_updates(
							root,
							&configuration,
							&discovery.packages,
							&plan,
						)
					})
				},
				|| {
					capture_prepare_phase("build lockfile refresh plan", || {
						build_lockfile_command_executions(
							root,
							&configuration,
							&discovery.packages,
							&plan,
						)
					})
				},
			)
		},
	);
	phase_timings.extend([
		changelog_targets_result.1,
		manifest_updates_result.1,
		versioned_file_updates_result.1,
		lockfile_commands_result.1,
	]);
	let changelog_targets = changelog_targets_result.0?;
	let manifest_updates = manifest_updates_result.0?;
	let versioned_file_updates = versioned_file_updates_result.0?;
	let release_targets = measure_async_prepare_phase(
		&mut phase_timings,
		"build release targets",
		build_release_targets(&configuration, &discovery.packages, &plan, &changeset_paths),
	)
	.await;
	let lockfile_commands = lockfile_commands_result.0?;
	let package_publications =
		build_package_publication_targets(&configuration, &discovery.packages, &plan);
	let changesets = if let Some(handle) = background_changeset_context {
		join_source_changeset_context_task(&mut phase_timings, handle).await?
	} else {
		changesets
			.take()
			.unwrap_or_else(|| panic!("changesets should be available after local planning"))
	};
	let changelog_release_targets = release_targets
		.iter()
		.map(|target| {
			monochange_changelog::ReleaseTarget {
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
		.collect::<Vec<_>>();
	let changelog_updates =
		measure_prepare_phase(&mut phase_timings, "build changelog updates", || {
			build_changelog_updates(
				ChangelogBuildContext::builder()
					.root(root)
					.configuration(&configuration)
					.packages(&discovery.packages)
					.plan(&plan)
					.change_signals(&change_signals)
					.changesets(&changesets)
					.changelog_targets(&changelog_targets)
					.release_targets(&changelog_release_targets)
					.build(),
			)
		})?;
	let changelog_file_updates = changelog_updates
		.iter()
		.map(|update| {
			FileUpdate {
				path: update.file.path.clone(),
				content: update.file.content.clone(),
			}
		})
		.collect::<Vec<_>>();
	let base_updates = [
		manifest_updates.clone(),
		versioned_file_updates.clone(),
		changelog_file_updates.clone(),
	]
	.concat();
	tracing::debug!(
		manifest_updates = manifest_updates.len(),
		lockfile_commands = lockfile_commands.len(),
		"built manifest and lockfile updates"
	);
	let file_updates = if lockfile_commands.is_empty() || dry_run {
		// During dry-run, skip the expensive workspace copy and lockfile
		// command execution. The base updates already contain all version
		// file and changelog changes; lockfile diffs are omitted from the
		// preview but this avoids copying the entire workspace to a temp
		// directory (which can take minutes for large repos).
		base_updates.clone()
	} else {
		materialize_lockfile_command_updates_with_timing(
			&mut phase_timings,
			root,
			&base_updates,
			&lockfile_commands,
		)?
	};
	let mut changed_files = file_updates
		.iter()
		.map(|update| root_relative(root, &update.path))
		.collect::<Vec<_>>();
	changed_files.sort();
	changed_files.dedup();
	let changelogs = changelog_updates
		.iter()
		.map(|update| {
			PreparedChangelog {
				owner_id: update.owner_id.clone(),
				owner_kind: update.owner_kind,
				path: root_relative(root, &update.file.path),
				format: update.format,
				notes: update.notes.clone(),
				rendered: update.rendered.clone(),
			}
		})
		.collect::<Vec<_>>();
	let updated_changelogs = changelogs
		.iter()
		.map(|update| update.path.clone())
		.collect::<Vec<_>>();
	// Diff rendering is far more expensive than preparing the release itself on
	// large workspaces because unified diffs need to read and compare every
	// changed file, including giant lockfiles. Only pay that cost when a caller
	// explicitly needs human-readable diff previews.
	let file_diffs = if build_file_diffs {
		measure_prepare_phase(&mut phase_timings, "build file diff previews", || {
			build_file_diff_previews(root, &file_updates)
		})?
	} else {
		Vec::new()
	};

	let version = shared_release_version(&plan);
	let group_version = shared_group_version(&plan);
	let mut deleted_changesets = Vec::new();
	if !dry_run {
		measure_prepare_phase(&mut phase_timings, "apply release changes", || {
			// When lockfile commands ran, materialize_lockfile_command_updates
			// already applied base_updates in-place. Only apply when we
			// skipped that path (no lockfile commands).
			if lockfile_commands.is_empty() {
				apply_file_updates(&file_updates)?;
			}
			for path in &changeset_paths {
				delete_changeset_file(path)?;
				deleted_changesets.push(root_relative(root, path));
			}
			Ok(())
		})?;
	}

	tracing::info!(
		changed_files = changed_files.len(),
		dry_run,
		"release preparation complete"
	);

	Ok(PreparedReleaseExecution {
		prepared_release: PreparedRelease {
			plan,
			changeset_paths,
			changesets,
			released_packages,
			package_publications,
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
		phase_timings,
	})
}

fn measure_prepare_phase<T>(
	phase_timings: &mut Vec<StepPhaseTiming>,
	label: impl Into<String>,
	action: impl FnOnce() -> MonochangeResult<T>,
) -> MonochangeResult<T> {
	let label = label.into();
	let started_at = Instant::now();
	let result = action();
	record_prepare_phase_timing(phase_timings, label, started_at);
	result
}

async fn measure_async_prepare_phase<T>(
	phase_timings: &mut Vec<StepPhaseTiming>,
	label: impl Into<String>,
	future: impl Future<Output = T>,
) -> T {
	let label = label.into();
	let started_at = Instant::now();
	let result = future.await;
	record_prepare_phase_timing(phase_timings, label, started_at);
	result
}

fn capture_prepare_phase<T>(
	label: impl Into<String>,
	action: impl FnOnce() -> MonochangeResult<T>,
) -> (MonochangeResult<T>, StepPhaseTiming) {
	let label = label.into();
	let started_at = Instant::now();
	let result = action();
	(
		result,
		StepPhaseTiming {
			label,
			duration: started_at.elapsed(),
		},
	)
}

fn record_prepare_phase_timing(
	phase_timings: &mut Vec<StepPhaseTiming>,
	label: impl Into<String>,
	started_at: Instant,
) {
	phase_timings.push(StepPhaseTiming {
		label: label.into(),
		duration: started_at.elapsed(),
	});
}

fn read_changeset_source(path: &Path) -> MonochangeResult<String> {
	fs::read_to_string(path)
		.map_err(|error| MonochangeError::Io(format!("failed to read {}: {error}", path.display())))
}

fn delete_changeset_file(path: &Path) -> MonochangeResult<()> {
	fs::remove_file(path).map_err(|error| {
		MonochangeError::Io(format!("failed to delete {}: {error}", path.display()))
	})
}

fn changeset_context_phase_label(source: &SourceConfiguration, dry_run: bool) -> String {
	if dry_run {
		format!("annotate changeset context via {}", source.provider)
	} else {
		format!("enrich changeset context via {}", source.provider)
	}
}

async fn apply_source_changeset_context_with_timing(
	phase_timings: &mut Vec<StepPhaseTiming>,
	source: &SourceConfiguration,
	dry_run: bool,
	changesets: &mut [PreparedChangeset],
) {
	let label = changeset_context_phase_label(source, dry_run);
	let started_at = Instant::now();
	apply_source_changeset_context(source, dry_run, changesets).await;
	record_prepare_phase_timing(phase_timings, label, started_at);
}

fn spawn_source_changeset_context_task(
	source: SourceConfiguration,
	dry_run: bool,
	mut changesets: Vec<PreparedChangeset>,
) -> JoinHandle<(Vec<PreparedChangeset>, StepPhaseTiming)> {
	tokio::spawn(async move {
		let label = changeset_context_phase_label(&source, dry_run);
		let started_at = Instant::now();
		apply_source_changeset_context(&source, dry_run, &mut changesets).await;
		(
			changesets,
			StepPhaseTiming {
				label,
				duration: started_at.elapsed(),
			},
		)
	})
}

async fn join_source_changeset_context_task(
	phase_timings: &mut Vec<StepPhaseTiming>,
	handle: JoinHandle<(Vec<PreparedChangeset>, StepPhaseTiming)>,
) -> MonochangeResult<Vec<PreparedChangeset>> {
	let (changesets, timing) = handle.await.map_err(|_| {
		MonochangeError::Io("background changeset context enrichment panicked".to_string())
	})?;
	phase_timings.push(timing);
	Ok(changesets)
}

async fn apply_source_changeset_context(
	source: &SourceConfiguration,
	dry_run: bool,
	changesets: &mut [PreparedChangeset],
) {
	let adapter = hosted_sources::configured_hosted_source_adapter(source);
	if dry_run {
		adapter.annotate_changeset_context(source, changesets);
	} else {
		adapter.enrich_changeset_context(source, changesets).await;
	}
}

fn materialize_lockfile_command_updates_with_timing(
	phase_timings: &mut Vec<StepPhaseTiming>,
	root: &Path,
	base_updates: &[FileUpdate],
	lockfile_commands: &[LockfileCommandExecution],
) -> MonochangeResult<Vec<FileUpdate>> {
	let started_at = Instant::now();
	let result = materialize_lockfile_command_updates(root, base_updates, lockfile_commands);
	record_prepare_phase_timing(
		phase_timings,
		"materialize lockfile command updates",
		started_at,
	);
	result
}

#[cfg(test)]
#[path = "__tests__/workspace_ops_tests.rs"]
mod workspace_ops_tests;
