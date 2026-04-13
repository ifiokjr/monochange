use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::thread::JoinHandle;
use std::time::Instant;

#[cfg(feature = "cargo")]
use monochange_cargo::discover_cargo_packages;
#[cfg(feature = "cargo")]
use monochange_cargo::load_configured_cargo_package;
use monochange_config::apply_version_groups;
use monochange_config::build_changeset_load_context;
use monochange_config::load_change_signals;
use monochange_config::load_changeset_contents_with_context;
use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::CliCommandDefinition;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
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
use monochange_dart::discover_dart_packages;
#[cfg(feature = "dart")]
use monochange_dart::load_configured_dart_package;
#[cfg(feature = "deno")]
use monochange_deno::discover_deno_packages;
#[cfg(feature = "deno")]
use monochange_deno::load_configured_deno_package;
#[cfg(feature = "npm")]
use monochange_npm::discover_npm_packages;
#[cfg(feature = "npm")]
use monochange_npm::load_configured_npm_package;
use serde_json::json;
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

fn render_cli_step_toml(rendered: &mut String, step: &monochange_core::CliStepDefinition) {
	let step_type = step.kind_name();
	writeln!(rendered, "type = {}", render_toml_string(step_type))
		.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
	if let Some(when) = step.when() {
		writeln!(rendered, "when = {}", render_toml_string(when))
			.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
	}
	match step {
		monochange_core::CliStepDefinition::Command {
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
		monochange_core::CliStepDefinition::Validate { inputs, .. }
		| monochange_core::CliStepDefinition::Discover { inputs, .. }
		| monochange_core::CliStepDefinition::CreateChangeFile { inputs, .. }
		| monochange_core::CliStepDefinition::PrepareRelease { inputs, .. }
		| monochange_core::CliStepDefinition::CommitRelease { inputs, .. }
		| monochange_core::CliStepDefinition::PublishRelease { inputs, .. }
		| monochange_core::CliStepDefinition::OpenReleaseRequest { inputs, .. }
		| monochange_core::CliStepDefinition::CommentReleasedIssues { inputs, .. }
		| monochange_core::CliStepDefinition::AffectedPackages { inputs, .. }
		| monochange_core::CliStepDefinition::DiagnoseChangesets { inputs, .. }
		| monochange_core::CliStepDefinition::RetargetRelease { inputs, .. } => {
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
	writeln!(
		rendered,
		"inputs = {}",
		render_step_inputs_inline_table(inputs)
	)
	.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
}

fn render_step_inputs_inline_table(
	inputs: &BTreeMap<String, monochange_core::CliStepInputValue>,
) -> String {
	format!(
		"{{ {} }}",
		inputs
			.iter()
			.map(|(name, value)| format!("{name} = {}", render_step_input_value(value)))
			.collect::<Vec<_>>()
			.join(", ")
	)
}

fn render_step_input_value(value: &monochange_core::CliStepInputValue) -> String {
	match value {
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
/// `monochange_core` or `monochange_config`, update `monochange.init.toml`
/// to document the new options.  See the `product-rules.md` agent rule
/// "keep init template in sync".
const INIT_TEMPLATE: &str = include_str!("monochange.init.toml");

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

fn discover_packages(root: &Path) -> MonochangeResult<Vec<PackageRecord>> {
	let mut packages = Vec::new();
	#[cfg(feature = "cargo")]
	packages.extend(discover_cargo_packages(root)?.packages);
	#[cfg(feature = "npm")]
	packages.extend(discover_npm_packages(root)?.packages);
	#[cfg(feature = "deno")]
	packages.extend(discover_deno_packages(root)?.packages);
	#[cfg(feature = "dart")]
	packages.extend(discover_dart_packages(root)?.packages);
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

#[allow(clippy::match_same_arms)]
fn package_type_for_ecosystem(ecosystem: Ecosystem) -> PackageType {
	match ecosystem {
		Ecosystem::Cargo => PackageType::Cargo,
		Ecosystem::Npm => PackageType::Npm,
		Ecosystem::Deno => PackageType::Deno,
		Ecosystem::Dart => PackageType::Dart,
		Ecosystem::Flutter => PackageType::Flutter,
		_ => PackageType::Cargo,
	}
}

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
	#[rustfmt::skip]
	let cargo_executions = resolve_lockfile_command_executions(root, &configuration.cargo.lockfile_commands, packages.iter().any(|package| package.ecosystem == Ecosystem::Cargo && released_versions.contains_key(&package.id)))?;
	#[cfg(feature = "npm")]
	#[rustfmt::skip]
	let npm_executions = resolve_lockfile_command_executions(root, &configuration.npm.lockfile_commands, packages.iter().any(|package| package.ecosystem == Ecosystem::Npm && released_versions.contains_key(&package.id)))?;
	#[cfg(feature = "deno")]
	#[rustfmt::skip]
	let deno_executions = resolve_lockfile_command_executions(root, &configuration.deno.lockfile_commands, packages.iter().any(|package| package.ecosystem == Ecosystem::Deno && released_versions.contains_key(&package.id)))?;
	#[cfg(feature = "dart")]
	#[rustfmt::skip]
	let dart_executions = resolve_lockfile_command_executions(root, &configuration.dart.lockfile_commands, packages.iter().any(|package| matches!(package.ecosystem, Ecosystem::Dart | Ecosystem::Flutter) && released_versions.contains_key(&package.id)))?;
	let mut executions = Vec::new();
	#[cfg(feature = "cargo")]
	executions.extend(cargo_executions);
	#[cfg(feature = "npm")]
	executions.extend(npm_executions);
	#[cfg(feature = "deno")]
	executions.extend(deno_executions);
	#[cfg(feature = "dart")]
	executions.extend(dart_executions);
	Ok(dedup_lockfile_command_executions(executions))
}

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

#[tracing::instrument(skip_all)]
#[must_use = "the discovery result must be checked"]
pub fn discover_workspace(root: &Path) -> MonochangeResult<DiscoveryReport> {
	let configuration = load_workspace_configuration(root)?;
	let mut warnings = Vec::new();
	let mut packages = Vec::new();

	#[cfg(all(feature = "cargo", feature = "npm", feature = "deno", feature = "dart"))]
	{
		let ((cargo_discovery, npm_discovery), (deno_discovery, dart_discovery)) = rayon::join(
			|| {
				rayon::join(
					|| discover_cargo_packages(root),
					|| discover_npm_packages(root),
				)
			},
			|| {
				rayon::join(
					|| discover_deno_packages(root),
					|| discover_dart_packages(root),
				)
			},
		);
		for discovery in [
			cargo_discovery?,
			npm_discovery?,
			deno_discovery?,
			dart_discovery?,
		] {
			warnings.extend(discovery.warnings);
			packages.extend(discovery.packages);
		}
	}

	#[cfg(not(all(feature = "cargo", feature = "npm", feature = "deno", feature = "dart")))]
	{
		#[cfg(feature = "cargo")]
		{
			let d = discover_cargo_packages(root)?;
			warnings.extend(d.warnings);
			packages.extend(d.packages);
		}
		#[cfg(feature = "npm")]
		{
			let d = discover_npm_packages(root)?;
			warnings.extend(d.warnings);
			packages.extend(d.packages);
		}
		#[cfg(feature = "deno")]
		{
			let d = discover_deno_packages(root)?;
			warnings.extend(d.warnings);
			packages.extend(d.packages);
		}
		#[cfg(feature = "dart")]
		{
			let d = discover_dart_packages(root)?;
			warnings.extend(d.warnings);
			packages.extend(d.packages);
		}
	}

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

	let mut packages = Vec::new();
	for package_definition in &configuration.packages {
		let path = root.join(&package_definition.path);
		let package = match package_definition.package_type {
			#[cfg(feature = "cargo")]
			PackageType::Cargo => load_configured_cargo_package(root, &path)?,
			#[cfg(not(feature = "cargo"))]
			PackageType::Cargo => {
				return Err(MonochangeError::Config(
					"the `cargo` feature must be enabled to load Cargo packages".to_string(),
				));
			}
			#[cfg(feature = "npm")]
			PackageType::Npm => load_configured_npm_package(root, &path)?,
			#[cfg(not(feature = "npm"))]
			PackageType::Npm => {
				return Err(MonochangeError::Config(
					"the `npm` feature must be enabled to load npm packages".to_string(),
				));
			}
			#[cfg(feature = "deno")]
			PackageType::Deno => load_configured_deno_package(root, &path)?,
			#[cfg(not(feature = "deno"))]
			PackageType::Deno => {
				return Err(MonochangeError::Config(
					"the `deno` feature must be enabled to load Deno packages".to_string(),
				));
			}
			#[cfg(feature = "dart")]
			PackageType::Dart | PackageType::Flutter => load_configured_dart_package(root, &path)?,
			#[cfg(not(feature = "dart"))]
			PackageType::Dart | PackageType::Flutter => {
				return Err(MonochangeError::Config(
					"the `dart` feature must be enabled to load Dart packages".to_string(),
				));
			}
			_ => {
				return Err(MonochangeError::Config(format!(
					"unsupported package type `{}` for `{}`",
					package_definition.package_type.as_str(),
					package_definition.id,
				)));
			}
		}
		.ok_or_else(|| {
			MonochangeError::Discovery(format!(
				"configured package `{}` at {} could not be discovered",
				package_definition.id,
				package_definition.path.display()
			))
		})?;
		packages.push(package);
	}

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
) -> MonochangeResult<Vec<String>> {
	if change_type.is_none() && version.is_none() && bump == BumpSeverity::None {
		return Err(MonochangeError::Config(format!(
			"target `{target_id}` must not use a `none` bump without also declaring `type` or `version`"
		)));
	}
	let mut lines = Vec::new();
	let target_key = render_changeset_target_key(target_id);
	if let Some(change_type) = change_type.filter(|value| !value.trim().is_empty()) {
		let default_bump = change_type_default_bump(configuration, target_id, change_type)
			.ok_or_else(|| {
				MonochangeError::Config(format!(
					"target `{target_id}` uses unknown change type `{change_type}`"
				))
			})?;
		if version.is_none() && bump == default_bump {
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
		return Ok(lines);
	}
	if let Some(version) = version {
		lines.push(format!("{target_key}:"));
		if bump != BumpSeverity::None {
			lines.push(format!("  bump: {bump}"));
		}
		lines.push(format!("  version: \"{version}\""));
		return Ok(lines);
	}
	lines.push(format!("{target_key}: {bump}"));
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

#[must_use = "the prepared release result must be checked"]
pub fn prepare_release(root: &Path, dry_run: bool) -> MonochangeResult<PreparedRelease> {
	// The public API returns structured release state, not rendered diffs.
	// Building unified diffs for large lockfiles can dominate wall time, so skip
	// that work here unless a caller explicitly asks for the richer execution
	// report via `prepare_release_execution`.
	prepare_release_execution_with_file_diffs(root, dry_run, false)
		.map(|execution| execution.prepared_release)
}

#[cfg(test)]
#[tracing::instrument(skip_all, fields(dry_run))]
pub(crate) fn prepare_release_execution(
	root: &Path,
	dry_run: bool,
) -> MonochangeResult<PreparedReleaseExecution> {
	prepare_release_execution_with_file_diffs(root, dry_run, true)
}

#[tracing::instrument(skip_all, fields(dry_run, build_file_diffs))]
pub(crate) fn prepare_release_execution_with_file_diffs(
	root: &Path,
	dry_run: bool,
	build_file_diffs: bool,
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
			discover_changeset_paths(root)
		})?;
	tracing::debug!(count = changeset_paths.len(), "discovered changesets");

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
	if let Some(source) = configuration.source.as_ref().filter(|_| dry_run) {
		apply_source_changeset_context_with_timing(
			&mut phase_timings,
			source,
			dry_run,
			changesets
				.as_mut()
				.unwrap_or_else(|| panic!("changesets should exist for dry-run annotation")),
		);
	}
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
		((versioned_file_updates_result, release_targets_result), lockfile_commands_result),
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
							capture_prepare_phase("build release targets", || {
								Ok(build_release_targets(
									&configuration,
									&discovery.packages,
									&plan,
									&changeset_paths,
								))
							})
						},
					)
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
		release_targets_result.1,
		lockfile_commands_result.1,
	]);
	let changelog_targets = changelog_targets_result.0?;
	let manifest_updates = manifest_updates_result.0?;
	let versioned_file_updates = versioned_file_updates_result.0?;
	let release_targets = release_targets_result.0?;
	let lockfile_commands = lockfile_commands_result.0?;
	let changesets = if let Some(handle) = background_changeset_context {
		join_source_changeset_context_task(&mut phase_timings, handle)?
	} else {
		changesets
			.take()
			.unwrap_or_else(|| panic!("changesets should be available after local planning"))
	};
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
					.release_targets(&release_targets)
					.build(),
			)
		})?;
	let changelog_file_updates = changelog_updates
		.iter()
		.map(|update| update.file.clone())
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
		#[rustfmt::skip]
		let materialized_updates = materialize_lockfile_command_updates_with_timing(&mut phase_timings, root, &base_updates, &lockfile_commands)?;
		materialized_updates
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

fn apply_source_changeset_context_with_timing(
	phase_timings: &mut Vec<StepPhaseTiming>,
	source: &SourceConfiguration,
	dry_run: bool,
	changesets: &mut [PreparedChangeset],
) {
	let label = changeset_context_phase_label(source, dry_run);
	let started_at = Instant::now();
	apply_source_changeset_context(source, dry_run, changesets);
	record_prepare_phase_timing(phase_timings, label, started_at);
}

fn spawn_source_changeset_context_task(
	source: SourceConfiguration,
	dry_run: bool,
	mut changesets: Vec<PreparedChangeset>,
) -> JoinHandle<(Vec<PreparedChangeset>, StepPhaseTiming)> {
	std::thread::spawn(move || {
		let label = changeset_context_phase_label(&source, dry_run);
		let started_at = Instant::now();
		apply_source_changeset_context(&source, dry_run, &mut changesets);
		(
			changesets,
			StepPhaseTiming {
				label,
				duration: started_at.elapsed(),
			},
		)
	})
}

fn join_source_changeset_context_task(
	phase_timings: &mut Vec<StepPhaseTiming>,
	handle: JoinHandle<(Vec<PreparedChangeset>, StepPhaseTiming)>,
) -> MonochangeResult<Vec<PreparedChangeset>> {
	let (changesets, timing) = handle.join().map_err(|_| {
		MonochangeError::Io("background changeset context enrichment panicked".to_string())
	})?;
	phase_timings.push(timing);
	Ok(changesets)
}

fn apply_source_changeset_context(
	source: &SourceConfiguration,
	dry_run: bool,
	changesets: &mut [PreparedChangeset],
) {
	let adapter = hosted_sources::configured_hosted_source_adapter(source);
	if dry_run {
		adapter.annotate_changeset_context(source, changesets);
	} else {
		adapter.enrich_changeset_context(source, changesets);
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
mod workspace_ops_tests {
	#[cfg(unix)]
	use std::os::unix::fs::PermissionsExt;

	use monochange_core::ChangesetContext;
	use monochange_core::ChangesetRevision;
	use monochange_core::HostedActorRef;
	use monochange_core::HostedActorSourceKind;
	use monochange_core::HostedCommitRef;
	use monochange_core::HostingCapabilities;
	use monochange_core::HostingProviderKind;
	use monochange_core::PackageDefinition;
	use monochange_core::PreparedChangeset;
	use monochange_core::ProviderBotSettings;
	use monochange_core::ProviderMergeRequestSettings;
	use monochange_core::ProviderReleaseSettings;
	use monochange_core::ShellConfig;
	use monochange_core::SourceConfiguration;
	use monochange_core::SourceProvider;
	use monochange_core::VersionFormat;
	use monochange_core::WorkspaceConfiguration;

	use super::*;

	fn setup_workspace_ops_fixture() -> tempfile::TempDir {
		monochange_test_helpers::fs::setup_fixture_from(
			env!("CARGO_MANIFEST_DIR"),
			"workspace-ops/lockfile-command-helpers",
		)
	}

	fn setup_fixture(relative: &str) -> tempfile::TempDir {
		monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
	}

	#[cfg(unix)]
	fn make_executable(path: &Path) {
		let metadata = fs::metadata(path).unwrap_or_else(|error| panic!("metadata: {error}"));
		let mut permissions = metadata.permissions();
		permissions.set_mode(0o755);
		fs::set_permissions(path, permissions)
			.unwrap_or_else(|error| panic!("set permissions: {error}"));
	}

	#[cfg(not(unix))]
	fn make_executable(_path: &Path) {}

	fn sample_changeset_with_context() -> PreparedChangeset {
		PreparedChangeset {
			path: PathBuf::from(".changeset/feature.md"),
			summary: Some("feature".to_string()),
			details: None,
			targets: Vec::new(),
			context: Some(ChangesetContext {
				provider: HostingProviderKind::GenericGit,
				host: None,
				capabilities: HostingCapabilities::default(),
				introduced: Some(ChangesetRevision {
					actor: Some(HostedActorRef {
						provider: HostingProviderKind::GenericGit,
						host: None,
						id: None,
						login: Some("ifiokjr".to_string()),
						display_name: Some("Ifiok Jr.".to_string()),
						url: None,
						source: HostedActorSourceKind::CommitAuthor,
					}),
					commit: Some(HostedCommitRef {
						provider: HostingProviderKind::GenericGit,
						host: None,
						sha: "abc1234567890".to_string(),
						short_sha: "abc1234".to_string(),
						url: None,
						authored_at: None,
						committed_at: None,
						author_name: Some("Ifiok Jr.".to_string()),
						author_email: Some("ifiok@example.com".to_string()),
					}),
					review_request: None,
				}),
				last_updated: None,
				related_issues: Vec::new(),
			}),
		}
	}

	fn sample_source(provider: SourceProvider) -> SourceConfiguration {
		SourceConfiguration {
			provider,
			host: match provider {
				SourceProvider::Gitea => Some("https://codeberg.org".to_string()),
				SourceProvider::GitHub | SourceProvider::GitLab => None,
			},
			api_url: None,
			owner: match provider {
				SourceProvider::GitLab => "group".to_string(),
				_ => "org".to_string(),
			},
			repo: "monochange".to_string(),
			releases: ProviderReleaseSettings::default(),
			pull_requests: ProviderMergeRequestSettings::default(),
			bot: ProviderBotSettings::default(),
		}
	}

	fn workspace_configuration_with_lockfile_commands() -> WorkspaceConfiguration {
		WorkspaceConfiguration {
			root_path: PathBuf::from("."),
			defaults: monochange_core::WorkspaceDefaults::default(),
			release_notes: monochange_core::ReleaseNotesSettings::default(),
			packages: Vec::new(),
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
			cargo: monochange_core::EcosystemSettings {
				lockfile_commands: vec![LockfileCommandDefinition {
					command: "cargo metadata".to_string(),
					cwd: None,
					shell: ShellConfig::None,
				}],
				..monochange_core::EcosystemSettings::default()
			},
			npm: monochange_core::EcosystemSettings::default(),
			deno: monochange_core::EcosystemSettings::default(),
			dart: monochange_core::EcosystemSettings::default(),
		}
	}

	#[test]
	fn remap_workspace_path_rejects_paths_outside_the_workspace() {
		let fixture = setup_workspace_ops_fixture();
		let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let error = remap_workspace_path(
			fixture.path(),
			temp_root.path(),
			Path::new("/tmp/outside-workspace"),
		)
		.err()
		.unwrap_or_else(|| panic!("expected remap error"));
		assert!(error.to_string().contains("was outside workspace root"));
	}

	#[test]
	fn remap_workspace_path_maps_workspace_paths_into_the_temp_root() {
		let fixture = setup_workspace_ops_fixture();
		let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

		let remapped = remap_workspace_path(
			fixture.path(),
			temp_root.path(),
			&fixture.path().join("root.txt"),
		)
		.unwrap_or_else(|error| panic!("remap workspace path: {error}"));

		assert_eq!(remapped, temp_root.path().join("root.txt"));
	}

	#[test]
	fn init_and_populate_workspace_cover_common_error_and_noop_paths() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		fs::create_dir_all(root.join("monochange.toml"))
			.unwrap_or_else(|error| panic!("create blocking config dir: {error}"));
		let init_error = init_workspace(root, true)
			.err()
			.unwrap_or_else(|| panic!("expected init write error"));
		assert!(init_error.to_string().contains("failed to write"));

		let empty_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let missing_error = populate_workspace(empty_root.path())
			.err()
			.unwrap_or_else(|| panic!("expected missing config error"));
		assert!(missing_error.to_string().contains("run `mc init` first"));

		let populated_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::write(
			populated_root.path().join("monochange.toml"),
			render_cli_commands_toml(&default_cli_commands()),
		)
		.unwrap_or_else(|error| panic!("write populated config: {error}"));
		let populated = populate_workspace(populated_root.path())
			.unwrap_or_else(|error| panic!("populate no-op: {error}"));
		assert!(populated.added_commands.is_empty());
	}

	#[test]
	fn init_rendering_helpers_cover_duplicate_names_changelogs_and_package_types() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		fs::create_dir_all(root.join("crates/core/src")).unwrap();
		fs::create_dir_all(root.join("packages/core")).unwrap();
		fs::create_dir_all(root.join("apps/cli")).unwrap();
		fs::create_dir_all(root.join("apps/mobile")).unwrap();
		fs::write(
			root.join("Cargo.toml"),
			"[workspace]\nmembers = [\"crates/core\"]\n",
		)
		.unwrap();
		fs::write(
			root.join("crates/core/Cargo.toml"),
			"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
		)
		.unwrap();
		fs::write(
			root.join("packages/core/package.json"),
			r#"{ "name": "core", "version": "1.0.0" }"#,
		)
		.unwrap();
		fs::write(
			root.join("apps/cli/deno.json"),
			r#"{ "name": "@acme/cli", "version": "1.0.0" }"#,
		)
		.unwrap();
		fs::write(
			root.join("apps/mobile/pubspec.yaml"),
			"name: mobile\nversion: 1.0.0\nflutter:\n  uses-material-design: true\n",
		)
		.unwrap();
		fs::create_dir_all(root.join("packages/dart_pkg/lib")).unwrap();
		fs::write(
			root.join("packages/dart_pkg/pubspec.yaml"),
			"name: dart_pkg\nversion: 1.0.0\n",
		)
		.unwrap();
		fs::write(root.join("packages/core/changelog.md"), "# Changelog\n").unwrap();

		let rendered = render_annotated_init_config(root)
			.unwrap_or_else(|error| panic!("render annotated config: {error}"));
		assert!(rendered.contains("type = \"cargo\""));
		assert!(rendered.contains("type = \"npm\""));
		assert!(rendered.contains("type = \"deno\""));
		assert!(rendered.contains("type = \"dart\""));
		assert!(rendered.contains("type = \"flutter\""));
		assert!(rendered.contains("changelog = \"packages/core/changelog.md\""));
		assert!(rendered.contains("packages/core"));

		assert_eq!(
			detect_default_changelog(root, &root.join("packages/core")),
			Some(PathBuf::from("packages/core/changelog.md"))
		);
		assert_eq!(
			package_type_for_ecosystem(Ecosystem::Cargo),
			PackageType::Cargo
		);
		assert_eq!(package_type_for_ecosystem(Ecosystem::Npm), PackageType::Npm);
		assert_eq!(
			package_type_for_ecosystem(Ecosystem::Deno),
			PackageType::Deno
		);
		assert_eq!(
			package_type_for_ecosystem(Ecosystem::Dart),
			PackageType::Dart
		);
		assert_eq!(
			package_type_for_ecosystem(Ecosystem::Flutter),
			PackageType::Flutter
		);
	}

	#[test]
	fn validate_and_discover_release_workspace_cover_fallback_and_errors() {
		let empty_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::write(empty_root.path().join("monochange.toml"), "").unwrap();
		validate_cargo_workspace_version_groups(empty_root.path())
			.unwrap_or_else(|error| panic!("validate empty workspace: {error}"));

		let fixture = setup_fixture("monochange/release-base");
		let configuration = load_workspace_configuration(fixture.path())
			.unwrap_or_else(|error| panic!("load config: {error}"));
		let report = discover_release_workspace(fixture.path(), &configuration)
			.unwrap_or_else(|error| panic!("discover release workspace: {error}"));
		assert_eq!(report.packages.len(), 2);

		let fallback = discover_release_workspace(
			fixture.path(),
			&WorkspaceConfiguration {
				packages: Vec::new(),
				..configuration.clone()
			},
		)
		.unwrap_or_else(|error| panic!("discover fallback workspace: {error}"));
		assert!(!fallback.packages.is_empty());

		let broken = WorkspaceConfiguration {
			packages: vec![PackageDefinition {
				id: "missing".to_string(),
				path: PathBuf::from("missing"),
				package_type: PackageType::Cargo,
				changelog: None,
				extra_changelog_sections: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				version_format: VersionFormat::Primary,
			}],
			..configuration
		};
		let error = discover_release_workspace(fixture.path(), &broken)
			.err()
			.unwrap_or_else(|| panic!("expected configured package discovery error"));
		assert!(
			error.to_string().contains("failed to read")
				|| error.to_string().contains("could not be discovered")
		);

		let undetected_root =
			tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::create_dir_all(undetected_root.path().join("empty-package"))
			.unwrap_or_else(|error| panic!("create empty package dir: {error}"));
		let undetected = WorkspaceConfiguration {
			root_path: undetected_root.path().to_path_buf(),
			defaults: monochange_core::WorkspaceDefaults {
				package_type: Some(PackageType::Cargo),
				..monochange_core::WorkspaceDefaults::default()
			},
			release_notes: monochange_core::ReleaseNotesSettings::default(),
			packages: vec![PackageDefinition {
				id: "empty".to_string(),
				path: PathBuf::from("empty-package"),
				package_type: PackageType::Cargo,
				changelog: None,
				extra_changelog_sections: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				version_format: VersionFormat::Primary,
			}],
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
			cargo: monochange_core::EcosystemSettings::default(),
			npm: monochange_core::EcosystemSettings::default(),
			deno: monochange_core::EcosystemSettings::default(),
			dart: monochange_core::EcosystemSettings::default(),
		};
		let undetected_error = discover_release_workspace(undetected_root.path(), &undetected)
			.err()
			.unwrap_or_else(|| panic!("expected configured package none-discovered error"));
		assert!(
			undetected_error
				.to_string()
				.contains("could not be discovered")
				|| undetected_error.to_string().contains("failed to read")
		);

		let missing_manifest_root =
			tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::create_dir_all(missing_manifest_root.path().join("packages/pkg"))
			.unwrap_or_else(|error| panic!("create package dir: {error}"));
		let missing_manifest = WorkspaceConfiguration {
			root_path: missing_manifest_root.path().to_path_buf(),
			defaults: monochange_core::WorkspaceDefaults {
				package_type: Some(PackageType::Npm),
				..monochange_core::WorkspaceDefaults::default()
			},
			release_notes: monochange_core::ReleaseNotesSettings::default(),
			packages: vec![PackageDefinition {
				id: "pkg".to_string(),
				path: PathBuf::from("packages/pkg"),
				package_type: PackageType::Npm,
				changelog: None,
				extra_changelog_sections: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				version_format: VersionFormat::Primary,
			}],
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
			cargo: monochange_core::EcosystemSettings::default(),
			npm: monochange_core::EcosystemSettings::default(),
			deno: monochange_core::EcosystemSettings::default(),
			dart: monochange_core::EcosystemSettings::default(),
		};
		let missing_manifest_error =
			discover_release_workspace(missing_manifest_root.path(), &missing_manifest)
				.err()
				.unwrap_or_else(|| panic!("expected configured package discovery failure"));
		assert!(
			missing_manifest_error
				.to_string()
				.contains("could not be discovered")
				|| missing_manifest_error
					.to_string()
					.contains("failed to read")
		);

		let non_cargo_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::create_dir_all(non_cargo_root.path().join("packages/web"))
			.unwrap_or_else(|error| panic!("create npm package dir: {error}"));
		fs::write(
			non_cargo_root.path().join("monochange.toml"),
			r#"
[defaults]
package_type = "npm"

[package.web]
path = "packages/web"
"#,
		)
		.unwrap_or_else(|error| panic!("write npm-only monochange config: {error}"));
		fs::write(
			non_cargo_root.path().join("packages/web/package.json"),
			r#"{ "name": "web", "version": "1.0.0" }"#,
		)
		.unwrap_or_else(|error| panic!("write package.json: {error}"));
		validate_cargo_workspace_version_groups(non_cargo_root.path())
			.unwrap_or_else(|error| panic!("validate npm-only workspace: {error}"));
	}

	#[test]
	fn run_lockfile_command_reports_parse_spawn_and_exit_failures() {
		let fixture = setup_workspace_ops_fixture();
		for script in ["fail-no-stderr", "fail-stderr"] {
			make_executable(&fixture.path().join("tools/bin").join(script));
		}
		let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		copy_workspace_tree(fixture.path(), temp_root.path())
			.unwrap_or_else(|error| panic!("copy workspace: {error}"));
		for script in ["fail-no-stderr", "fail-stderr"] {
			make_executable(&temp_root.path().join("tools/bin").join(script));
		}

		let parse_error = run_lockfile_command(
			fixture.path(),
			temp_root.path(),
			&LockfileCommandExecution {
				command: "'".to_string(),
				cwd: fixture.path().to_path_buf(),
				shell: ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
		assert!(parse_error.to_string().contains("failed to parse command"));

		let empty_error = run_lockfile_command(
			fixture.path(),
			temp_root.path(),
			&LockfileCommandExecution {
				command: "   ".to_string(),
				cwd: fixture.path().to_path_buf(),
				shell: ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected empty command error"));
		assert!(
			empty_error
				.to_string()
				.contains("lockfile command must not be empty")
		);

		let spawn_error = run_lockfile_command(
			fixture.path(),
			temp_root.path(),
			&LockfileCommandExecution {
				command: "definitely-not-a-real-command".to_string(),
				cwd: fixture.path().to_path_buf(),
				shell: ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected spawn error"));
		assert!(
			spawn_error
				.to_string()
				.contains("failed to run lockfile command")
		);

		let no_stderr_error = run_lockfile_command(
			fixture.path(),
			temp_root.path(),
			&LockfileCommandExecution {
				command: temp_root
					.path()
					.join("tools/bin/fail-no-stderr")
					.display()
					.to_string(),
				cwd: fixture.path().to_path_buf(),
				shell: ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected nonzero exit error"));
		assert!(no_stderr_error.to_string().contains("exit status"));

		let stderr_error = run_lockfile_command(
			fixture.path(),
			temp_root.path(),
			&LockfileCommandExecution {
				command: temp_root
					.path()
					.join("tools/bin/fail-stderr")
					.display()
					.to_string(),
				cwd: fixture.path().to_path_buf(),
				shell: ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected stderr error"));
		assert!(stderr_error.to_string().contains("bad stderr"));
	}

	#[test]
	fn workspace_copy_and_diff_helpers_skip_git_and_capture_new_files() {
		let fixture = setup_workspace_ops_fixture();
		make_executable(&fixture.path().join("tools/bin/write-new-file"));
		let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		copy_workspace_tree(fixture.path(), temp_root.path())
			.unwrap_or_else(|error| panic!("copy workspace: {error}"));
		make_executable(&temp_root.path().join("tools/bin/write-new-file"));
		assert!(temp_root.path().join("root.txt").exists());
		assert!(!temp_root.path().join(".git").exists());

		run_lockfile_command(
			fixture.path(),
			temp_root.path(),
			&LockfileCommandExecution {
				command: temp_root
					.path()
					.join("tools/bin/write-new-file")
					.display()
					.to_string(),
				cwd: fixture.path().to_path_buf(),
				shell: ShellConfig::None,
			},
		)
		.unwrap_or_else(|error| panic!("write generated file: {error}"));

		let lockfile_cmd = LockfileCommandExecution {
			command: String::new(),
			cwd: fixture.path().to_path_buf(),
			shell: ShellConfig::None,
		};
		let updates =
			collect_workspace_file_updates(fixture.path(), temp_root.path(), &[], &[lockfile_cmd])
				.unwrap_or_else(|error| panic!("collect updates: {error}"));
		assert!(
			updates
				.iter()
				.any(|update| update.path.ends_with("generated.txt"))
		);

		let mut paths = BTreeSet::new();
		collect_workspace_files(fixture.path(), fixture.path(), &mut paths)
			.unwrap_or_else(|error| panic!("collect files: {error}"));
		assert!(paths.contains(Path::new("root.txt")));
		assert!(!paths.iter().any(|path| path.starts_with(".git")));
	}

	#[test]
	fn file_helpers_report_missing_and_invalid_paths() {
		let fixture = setup_workspace_ops_fixture();
		assert!(
			read_optional_file(&fixture.path().join("missing.txt"))
				.unwrap_or_else(|error| panic!("missing file lookup: {error}"))
				.is_none()
		);
		let read_error = read_optional_file(fixture.path())
			.err()
			.unwrap_or_else(|| panic!("expected directory read error"));
		assert!(read_error.to_string().contains("failed to read"));

		let collect_error = collect_workspace_files(
			fixture.path(),
			&fixture.path().join("missing-dir"),
			&mut BTreeSet::new(),
		)
		.err()
		.unwrap_or_else(|| panic!("expected collect files error"));
		assert!(collect_error.to_string().contains("failed to read"));
		let strip_prefix_error = strip_workspace_prefix(
			&fixture.path().join("root.txt"),
			Path::new("/tmp/other-root"),
		)
		.err()
		.unwrap_or_else(|| panic!("expected strip-prefix error"));
		assert!(
			strip_prefix_error
				.to_string()
				.contains("was outside workspace root")
		);

		let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let destination_file = temp_root.path().join("not-a-directory");
		fs::write(&destination_file, "file")
			.unwrap_or_else(|error| panic!("write destination file: {error}"));
		let copy_error = copy_workspace_tree(fixture.path(), &destination_file)
			.err()
			.unwrap_or_else(|| panic!("expected copy workspace error"));
		assert!(copy_error.to_string().contains("failed to create"));
		let missing_source_error =
			copy_workspace_tree(&fixture.path().join("missing-source"), temp_root.path())
				.err()
				.unwrap_or_else(|| panic!("expected missing source error"));
		assert!(missing_source_error.to_string().contains("failed to read"));

		let destination_dir = temp_root.path().join("destination-dir");
		fs::create_dir_all(&destination_dir)
			.unwrap_or_else(|error| panic!("create destination dir: {error}"));
		let source_root = temp_root.path().join("copy-source");
		fs::create_dir_all(source_root.join("nested"))
			.unwrap_or_else(|error| panic!("create source dir: {error}"));
		fs::write(source_root.join("nested/file.txt"), "file")
			.unwrap_or_else(|error| panic!("write source file: {error}"));
		fs::write(destination_dir.join("nested"), "blocking file")
			.unwrap_or_else(|error| panic!("write blocking file: {error}"));
		let parent_create_error = ensure_parent_directory(&destination_dir.join("nested/file.txt"))
			.err()
			.unwrap_or_else(|| panic!("expected parent create error"));
		assert!(parent_create_error.to_string().contains("failed to create"));

		let copy_target_root = temp_root.path().join("copy-target-root");
		fs::create_dir_all(&copy_target_root)
			.unwrap_or_else(|error| panic!("create copy target root: {error}"));
		fs::create_dir_all(copy_target_root.join("root.txt"))
			.unwrap_or_else(|error| panic!("create copy target dir collision: {error}"));
		let copy_file_error = copy_workspace_file(
			&fixture.path().join("root.txt"),
			&copy_target_root.join("root.txt"),
		)
		.err()
		.unwrap_or_else(|| panic!("expected copy file error"));
		assert!(copy_file_error.to_string().contains("failed to copy"));
	}

	#[test]
	fn snapshot_directory_files_captures_file_contents() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		fs::create_dir_all(root.join("pkg")).unwrap();
		fs::write(root.join("pkg/lock.json"), b"lockfile-content").unwrap();
		fs::write(root.join("pkg/other.txt"), b"other").unwrap();

		let mut snapshots = BTreeMap::new();
		snapshot_directory_files(root, &root.join("pkg"), &mut snapshots)
			.unwrap_or_else(|error| panic!("snapshot: {error}"));

		assert_eq!(snapshots.len(), 2);
		assert_eq!(
			snapshots.get(Path::new("pkg/lock.json")).map(Vec::as_slice),
			Some(b"lockfile-content".as_slice())
		);
	}

	#[test]
	fn snapshot_directory_files_skips_subdirectories() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		fs::create_dir_all(root.join("pkg/subdir")).unwrap();
		fs::write(root.join("pkg/top.txt"), b"top").unwrap();
		fs::write(root.join("pkg/subdir/nested.txt"), b"nested").unwrap();

		let mut snapshots = BTreeMap::new();
		snapshot_directory_files(root, &root.join("pkg"), &mut snapshots)
			.unwrap_or_else(|error| panic!("snapshot: {error}"));

		// Only top-level files, not nested.
		assert_eq!(snapshots.len(), 1);
		assert!(snapshots.contains_key(Path::new("pkg/top.txt")));
	}

	#[test]
	fn snapshot_and_change_file_helpers_cover_error_paths() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		let snapshot_error =
			snapshot_directory_files(root, &root.join("missing-dir"), &mut BTreeMap::new())
				.err()
				.unwrap_or_else(|| panic!("expected snapshot error"));
		assert!(snapshot_error.to_string().contains("failed to read"));

		let fixture = setup_fixture("monochange/release-base");
		let invalid_version_error = add_change_file(
			fixture.path(),
			AddChangeFileRequest::builder()
				.package_refs(&["core".to_string()])
				.bump(BumpSeverity::Patch)
				.reason("reason")
				.version(Some("not-a-version"))
				.build(),
		)
		.err()
		.unwrap_or_else(|| panic!("expected invalid version error"));
		assert!(
			invalid_version_error
				.to_string()
				.contains("invalid explicit version")
		);

		let blocking_parent = fixture.path().join("blocked");
		fs::write(&blocking_parent, "file").unwrap();
		let write_error = add_interactive_change_file(
			fixture.path(),
			&interactive::InteractiveChangeResult {
				targets: vec![interactive::InteractiveTarget {
					id: "core".to_string(),
					bump: BumpSeverity::Patch,
					version: None,
					change_type: None,
				}],
				reason: "reason".to_string(),
				details: None,
			},
			Some(&blocking_parent.join("feature.md")),
		)
		.err()
		.unwrap_or_else(|| panic!("expected interactive write error"));
		assert!(write_error.to_string().contains("failed to create"));

		let blocking_output = fixture.path().join(".changeset");
		let add_change_write_error = add_change_file(
			fixture.path(),
			AddChangeFileRequest::builder()
				.package_refs(&["core".to_string()])
				.bump(BumpSeverity::Patch)
				.reason("reason")
				.output(Some(blocking_output.as_path()))
				.build(),
		)
		.err()
		.unwrap_or_else(|| panic!("expected add_change_file write error"));
		assert!(
			add_change_write_error
				.to_string()
				.contains("failed to write")
		);

		let blocked_parent = fixture.path().join("parent-file");
		fs::write(&blocked_parent, "file").unwrap();
		let add_change_parent_error = add_change_file(
			fixture.path(),
			AddChangeFileRequest::builder()
				.package_refs(&["core".to_string()])
				.bump(BumpSeverity::Patch)
				.reason("reason")
				.output(Some(blocked_parent.join("feature.md").as_path()))
				.build(),
		)
		.err()
		.unwrap_or_else(|| panic!("expected add_change_file parent creation error"));
		assert!(
			add_change_parent_error
				.to_string()
				.contains("failed to create")
		);

		let interactive_write_error = add_interactive_change_file(
			fixture.path(),
			&interactive::InteractiveChangeResult {
				targets: vec![interactive::InteractiveTarget {
					id: "core".to_string(),
					bump: BumpSeverity::Patch,
					version: None,
					change_type: None,
				}],
				reason: "reason".to_string(),
				details: Some("extra details".to_string()),
			},
			Some(blocking_output.as_path()),
		)
		.err()
		.unwrap_or_else(|| panic!("expected add_interactive_change_file write error"));
		assert!(
			interactive_write_error
				.to_string()
				.contains("failed to write")
		);

		let interactive_default_output = add_interactive_change_file(
			fixture.path(),
			&interactive::InteractiveChangeResult {
				targets: vec![interactive::InteractiveTarget {
					id: "core".to_string(),
					bump: BumpSeverity::Patch,
					version: None,
					change_type: None,
				}],
				reason: "reason".to_string(),
				details: None,
			},
			None,
		)
		.unwrap_or_else(|error| panic!("write interactive default changeset: {error}"));
		assert!(interactive_default_output.starts_with(fixture.path().join(".changeset")));
		assert!(interactive_default_output.exists());
	}

	#[test]
	fn materialize_lockfile_updates_captures_in_place_changes() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();

		fs::create_dir_all(root.join("tools/bin")).unwrap();
		let script_path = root.join("tools/bin/update-lock");
		fs::write(
			&script_path,
			"#!/bin/sh\necho 'updated-lockfile' > \"$PWD/lock.txt\"\n",
		)
		.unwrap();
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
		}

		fs::write(root.join("lock.txt"), b"original").unwrap();
		fs::write(root.join("manifest.json"), b"old-version").unwrap();

		let base_updates = vec![FileUpdate {
			path: root.join("manifest.json"),
			content: b"new-version".to_vec(),
		}];
		let lockfile_commands = vec![LockfileCommandExecution {
			command: script_path.display().to_string(),
			cwd: root.to_path_buf(),
			shell: ShellConfig::None,
		}];

		let updates = materialize_lockfile_command_updates(root, &base_updates, &lockfile_commands)
			.unwrap_or_else(|error| panic!("materialize: {error}"));

		assert!(
			updates.iter().any(|u| u.path.ends_with("manifest.json")),
			"expected manifest update"
		);
		assert!(
			updates.iter().any(|u| u.path.ends_with("lock.txt")),
			"expected lockfile update"
		);
		let lock_content = fs::read_to_string(root.join("lock.txt")).unwrap();
		assert!(
			lock_content.contains("updated-lockfile"),
			"lockfile should be updated in-place"
		);
	}

	#[test]
	fn lockfile_update_helpers_cover_missing_dirs_unreadable_files_and_stderr_paths() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		let unreadable_dir = root.join("snapshots");
		fs::create_dir_all(&unreadable_dir).unwrap();
		let unreadable_file = unreadable_dir.join("blocked.txt");
		fs::write(&unreadable_file, "blocked").unwrap();
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o000)).unwrap();
		}
		let snapshot_error = snapshot_directory_files(root, &unreadable_dir, &mut BTreeMap::new())
			.err()
			.unwrap_or_else(|| panic!("expected unreadable snapshot file error"));
		assert!(snapshot_error.to_string().contains("failed to read"));
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o644)).unwrap();
		}

		fs::create_dir_all(root.join("tools/bin")).unwrap();
		let script_path = root.join("tools/bin/fail-with-stderr");
		fs::write(&script_path, "#!/bin/sh\necho 'bad stderr' >&2\nexit 4\n").unwrap();
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
		}
		let stderr_error = run_lockfile_command_in_place(
			root,
			&LockfileCommandExecution {
				command: script_path.display().to_string(),
				cwd: PathBuf::from("."),
				shell: ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected stderr failure for in-place lockfile command"));
		assert!(stderr_error.to_string().contains("bad stderr"));
	}

	#[test]
	fn run_lockfile_command_in_place_reports_failures() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();

		let parse_error = run_lockfile_command_in_place(
			root,
			&LockfileCommandExecution {
				command: "'".to_string(),
				cwd: root.to_path_buf(),
				shell: ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
		assert!(parse_error.to_string().contains("failed to parse command"));

		let empty_error = run_lockfile_command_in_place(
			root,
			&LockfileCommandExecution {
				command: "   ".to_string(),
				cwd: root.to_path_buf(),
				shell: ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected empty command error"));
		assert!(
			empty_error
				.to_string()
				.contains("lockfile command must not be empty")
		);

		let spawn_error = run_lockfile_command_in_place(
			root,
			&LockfileCommandExecution {
				command: "definitely-not-a-real-command".to_string(),
				cwd: root.to_path_buf(),
				shell: ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected spawn error"));
		assert!(
			spawn_error
				.to_string()
				.contains("failed to run lockfile command")
		);

		let error = run_lockfile_command_in_place(
			root,
			&LockfileCommandExecution {
				command: "false".to_string(),
				cwd: root.to_path_buf(),
				shell: ShellConfig::Default,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected error from failing command"));

		assert!(error.to_string().contains("failed"));
	}

	#[test]
	fn run_lockfile_command_variants_cover_success_and_shell_paths() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		fs::create_dir_all(root.join("tools/bin"))
			.unwrap_or_else(|error| panic!("create tools dir: {error}"));
		let success_script = root.join("tools/bin/succeed");
		fs::write(
			&success_script,
			"#!/bin/sh\necho shell-success > \"$PWD/shell-output.txt\"\n",
		)
		.unwrap_or_else(|error| panic!("write success script: {error}"));
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			fs::set_permissions(&success_script, fs::Permissions::from_mode(0o755))
				.unwrap_or_else(|error| panic!("chmod success script: {error}"));
		}

		run_lockfile_command_in_place(
			root,
			&LockfileCommandExecution {
				command: success_script.display().to_string(),
				cwd: root.to_path_buf(),
				shell: ShellConfig::None,
			},
		)
		.unwrap_or_else(|error| panic!("run in-place direct command: {error}"));
		assert!(root.join("shell-output.txt").exists());

		let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		copy_workspace_tree(root, temp_root.path())
			.unwrap_or_else(|error| panic!("copy workspace for test command: {error}"));
		let remapped_script = temp_root.path().join("tools/bin/succeed");
		run_lockfile_command(
			root,
			temp_root.path(),
			&LockfileCommandExecution {
				command: remapped_script.display().to_string(),
				cwd: root.to_path_buf(),
				shell: ShellConfig::Default,
			},
		)
		.unwrap_or_else(|error| panic!("run workspace lockfile command through shell: {error}"));
		assert!(temp_root.path().join("shell-output.txt").exists());
	}

	#[test]
	fn collect_workspace_file_updates_handles_outside_command_cwds() {
		let fixture = setup_workspace_ops_fixture();
		let temp_root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		copy_workspace_tree(fixture.path(), temp_root.path())
			.unwrap_or_else(|error| panic!("copy workspace: {error}"));

		let updates = collect_workspace_file_updates(
			fixture.path(),
			temp_root.path(),
			&[FileUpdate {
				path: fixture.path().join("Cargo.toml"),
				content: b"[workspace]\n".to_vec(),
			}],
			&[LockfileCommandExecution {
				command: "echo ignored".to_string(),
				cwd: PathBuf::from("/tmp/outside-workspace"),
				shell: ShellConfig::None,
			}],
		)
		.unwrap_or_else(|error| panic!("collect workspace file updates: {error}"));
		assert!(
			updates
				.iter()
				.all(|update| update.path.starts_with(fixture.path()))
		);
	}

	#[test]
	fn read_and_delete_changeset_helpers_report_missing_paths() {
		let error = read_changeset_source(Path::new("missing/changeset.md"))
			.err()
			.unwrap_or_else(|| panic!("expected read error"));
		assert!(
			error
				.to_string()
				.contains("failed to read missing/changeset.md")
		);

		let error = delete_changeset_file(Path::new("missing/changeset.md"))
			.err()
			.unwrap_or_else(|| panic!("expected delete error"));
		assert!(
			error
				.to_string()
				.contains("failed to delete missing/changeset.md")
		);
	}

	#[test]
	fn changeset_context_phase_label_covers_annotate_and_enrich_modes() {
		let gitlab = sample_source(SourceProvider::GitLab);
		let github = sample_source(SourceProvider::GitHub);
		assert_eq!(github.host, None);
		assert_eq!(
			changeset_context_phase_label(&gitlab, true),
			"annotate changeset context via gitlab"
		);
		assert_eq!(
			changeset_context_phase_label(&gitlab, false),
			"enrich changeset context via gitlab"
		);
	}

	#[test]
	fn warn_about_incomplete_cargo_lockfiles_returns_early_when_commands_are_configured() {
		let configuration = workspace_configuration_with_lockfile_commands();
		warn_about_incomplete_cargo_lockfiles(
			Path::new("."),
			&configuration,
			&[],
			&BTreeMap::new(),
		);
	}

	#[test]
	fn apply_source_changeset_context_dispatches_non_dry_gitlab_and_gitea_enrichment() {
		let mut gitlab_changesets = vec![sample_changeset_with_context()];
		apply_source_changeset_context(
			&sample_source(SourceProvider::GitLab),
			false,
			&mut gitlab_changesets,
		);
		let gitlab_context = gitlab_changesets
			.first()
			.and_then(|changeset| changeset.context.as_ref())
			.unwrap_or_else(|| panic!("expected GitLab context"));
		assert_eq!(gitlab_context.provider, HostingProviderKind::GitLab);
		assert_eq!(gitlab_context.host.as_deref(), Some("gitlab.com"));
		assert_eq!(
			gitlab_context
				.introduced
				.as_ref()
				.and_then(|revision| revision.commit.as_ref())
				.and_then(|commit| commit.url.as_deref()),
			Some("https://gitlab.com/group/monochange/-/commit/abc1234567890")
		);

		let mut gitea_changesets = vec![sample_changeset_with_context()];
		apply_source_changeset_context(
			&sample_source(SourceProvider::Gitea),
			false,
			&mut gitea_changesets,
		);
		let gitea_context = gitea_changesets
			.first()
			.and_then(|changeset| changeset.context.as_ref())
			.unwrap_or_else(|| panic!("expected Gitea context"));
		assert_eq!(gitea_context.provider, HostingProviderKind::Gitea);
		assert_eq!(gitea_context.host.as_deref(), Some("codeberg.org"));
		assert_eq!(
			gitea_context
				.introduced
				.as_ref()
				.and_then(|revision| revision.commit.as_ref())
				.and_then(|commit| commit.url.as_deref()),
			Some("https://codeberg.org/org/monochange/commit/abc1234567890")
		);
	}

	#[test]
	fn prepare_release_execution_materializes_configured_lockfile_commands() {
		let fixture = monochange_test_helpers::fs::setup_fixture_from(
			env!("CARGO_MANIFEST_DIR"),
			"monochange/cargo-lock-release",
		);
		make_executable(&fixture.path().join("tools/bin/cargo"));
		let config_path = fixture.path().join("monochange.toml");
		let mut config = fs::read_to_string(&config_path)
			.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
		config.push_str(
			r#"

[[ecosystems.cargo.lockfile_commands]]
command = "tools/bin/cargo"
cwd = "."
"#,
		);
		fs::write(&config_path, config)
			.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

		let prepared = prepare_release_execution_with_file_diffs(fixture.path(), false, false)
			.unwrap_or_else(|error| panic!("prepare release: {error}"));

		assert!(
			prepared
				.phase_timings
				.iter()
				.any(|phase| phase.label == "materialize lockfile command updates")
		);
		assert_eq!(
			fs::read_to_string(fixture.path().join("Cargo.lock"))
				.unwrap_or_else(|error| panic!("read Cargo.lock: {error}"))
				.trim(),
			"[[package]]\nname = \"workflow-core\"\nversion = \"1.1.0\""
		);
	}

	#[test]
	fn prepare_release_execution_tracks_gitlab_context_phase_timing() {
		let fixture = monochange_test_helpers::fs::setup_fixture_from(
			env!("CARGO_MANIFEST_DIR"),
			"monochange/release-base",
		);
		let config_path = fixture.path().join("monochange.toml");
		let mut config = fs::read_to_string(&config_path)
			.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
		config.push_str(
			r#"

[source]
provider = "gitlab"
owner = "group"
repo = "monochange"
"#,
		);
		fs::write(&config_path, config)
			.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

		let prepared = prepare_release_execution_with_file_diffs(fixture.path(), true, false)
			.unwrap_or_else(|error| panic!("prepare release: {error}"));

		assert!(
			prepared
				.phase_timings
				.iter()
				.any(|phase| phase.label == "annotate changeset context via gitlab")
		);
	}

	#[test]
	fn prepare_release_execution_tracks_github_background_context_phase_timing() {
		let fixture = monochange_test_helpers::fs::setup_fixture_from(
			env!("CARGO_MANIFEST_DIR"),
			"monochange/release-base",
		);
		let config_path = fixture.path().join("monochange.toml");
		let mut config = fs::read_to_string(&config_path)
			.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
		config.push_str(
			r#"

[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"
"#,
		);
		fs::write(&config_path, config)
			.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

		let prepared = prepare_release_execution_with_file_diffs(fixture.path(), false, false)
			.unwrap_or_else(|error| panic!("prepare release: {error}"));

		assert!(
			prepared
				.phase_timings
				.iter()
				.any(|phase| phase.label == "enrich changeset context via github")
		);
	}

	#[test]
	fn join_source_changeset_context_task_reports_background_panic() {
		let mut phase_timings = Vec::new();
		let handle = std::thread::spawn(|| -> (Vec<PreparedChangeset>, StepPhaseTiming) {
			panic!("boom");
		});

		let error = join_source_changeset_context_task(&mut phase_timings, handle)
			.err()
			.unwrap_or_else(|| panic!("expected join error"));

		assert!(
			error
				.to_string()
				.contains("background changeset context enrichment panicked"),
			"error: {error}"
		);
		assert!(phase_timings.is_empty());
	}
}
