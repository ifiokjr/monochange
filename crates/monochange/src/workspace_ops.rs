use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use monochange_cargo::discover_cargo_packages;
use monochange_config::apply_version_groups;
use monochange_config::load_change_signals;
use monochange_config::load_changeset_file;
use monochange_config::load_workspace_configuration;
use monochange_core::default_cli_commands;
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PopulateWorkspaceResult {
	pub path: PathBuf,
	pub added_commands: Vec<String>,
}

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
		write_toml_key_value(rendered, "help_text", render_toml_string(help_text));
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

fn write_toml_key_value(rendered: &mut String, key: &str, value: String) {
	writeln!(rendered, "{key} = {value}")
		.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
}

fn render_cli_input_toml(rendered: &mut String, input: &monochange_core::CliInputDefinition) {
	write_toml_key_value(rendered, "name", render_toml_string(&input.name));
	write_toml_key_value(
		rendered,
		"type",
		render_toml_string(match input.kind {
			monochange_core::CliInputKind::String => "string",
			monochange_core::CliInputKind::StringList => "string_list",
			monochange_core::CliInputKind::Path => "path",
			monochange_core::CliInputKind::Choice => "choice",
			monochange_core::CliInputKind::Boolean => "boolean",
		}),
	);
	input.help_text.iter().for_each(|help_text| {
		write_toml_key_value(rendered, "help_text", render_toml_string(help_text));
	});
	if input.required {
		write_toml_key_value(rendered, "required", "true".to_string());
	}
	if let Some(default) = &input.default {
		write_toml_key_value(rendered, "default", render_toml_string(default));
	}
	if !input.choices.is_empty() {
		write_toml_key_value(rendered, "choices", render_toml_array(&input.choices));
	}
	if let Some(short) = input.short {
		write_toml_key_value(rendered, "short", render_toml_string(&short.to_string()));
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
		monochange_core::CliStepDefinition::RenderReleaseManifest { path, inputs, .. } => {
			path.iter().for_each(|path| {
				write_toml_key_value(
					rendered,
					"path",
					render_toml_string(&path.display().to_string()),
				);
			});
			render_step_inputs_toml(rendered, inputs);
		}
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
			id.iter()
				.for_each(|id| write_toml_key_value(rendered, "id", render_toml_string(id)));
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

pub(crate) fn build_lockfile_command_executions(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<LockfileCommandExecution>> {
	let released_versions = released_versions_by_record_id(plan);
	#[rustfmt::skip]
	let cargo_executions = resolve_lockfile_command_executions(root, &configuration.cargo.lockfile_commands, packages.iter().filter(|package| package.ecosystem == Ecosystem::Cargo && released_versions.contains_key(&package.id)).collect(), monochange_cargo::default_lockfile_commands)?;
	#[rustfmt::skip]
	let npm_executions = resolve_lockfile_command_executions(root, &configuration.npm.lockfile_commands, packages.iter().filter(|package| package.ecosystem == Ecosystem::Npm && released_versions.contains_key(&package.id)).collect(), monochange_npm::default_lockfile_commands)?;
	#[rustfmt::skip]
	let deno_executions = resolve_lockfile_command_executions(root, &configuration.deno.lockfile_commands, packages.iter().filter(|package| package.ecosystem == Ecosystem::Deno && released_versions.contains_key(&package.id)).collect(), monochange_deno::default_lockfile_commands)?;
	#[rustfmt::skip]
	let dart_executions = resolve_lockfile_command_executions(root, &configuration.dart.lockfile_commands, packages.iter().filter(|package| matches!(package.ecosystem, Ecosystem::Dart | Ecosystem::Flutter) && released_versions.contains_key(&package.id)).collect(), monochange_dart::default_lockfile_commands)?;
	let mut executions = cargo_executions;
	executions.extend(npm_executions);
	executions.extend(deno_executions);
	executions.extend(dart_executions);
	Ok(dedup_lockfile_command_executions(executions))
}

fn resolve_lockfile_command_executions(
	root: &Path,
	configured_commands: &[LockfileCommandDefinition],
	released_packages: Vec<&PackageRecord>,
	infer_defaults: fn(&PackageRecord) -> Vec<LockfileCommandExecution>,
) -> MonochangeResult<Vec<LockfileCommandExecution>> {
	if released_packages.is_empty() {
		return Ok(Vec::new());
	}
	if configured_commands.is_empty() {
		return Ok(released_packages
			.into_iter()
			.flat_map(infer_defaults)
			.collect());
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

fn remap_workspace_path(root: &Path, temp_root: &Path, path: &Path) -> MonochangeResult<PathBuf> {
	let normalized_root = monochange_core::normalize_path(root);
	let normalized_path = monochange_core::normalize_path(path);
	let relative = normalized_path
		.strip_prefix(&normalized_root)
		.map_err(|error| {
			MonochangeError::Config(format!(
				"path `{}` was outside workspace root `{}`: {error}",
				path.display(),
				root.display(),
			))
		})?;
	Ok(temp_root.join(relative))
}

fn run_lockfile_command(
	root: &Path,
	temp_root: &Path,
	command: &LockfileCommandExecution,
) -> MonochangeResult<()> {
	let cwd = remap_workspace_path(root, temp_root, &command.cwd)?;
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
	Err(MonochangeError::Discovery(format!(
		"lockfile command `{}` failed in {}: {details}",
		command.command,
		root_relative(root, &command.cwd).display(),
	)))
}

fn collect_workspace_file_updates(
	root: &Path,
	temp_root: &Path,
	base_updates: &[FileUpdate],
	lockfile_commands: &[LockfileCommandExecution],
) -> MonochangeResult<Vec<FileUpdate>> {
	// Instead of walking the entire workspace tree, only scan directories
	// that could have been modified: directories containing explicitly
	// updated files and lockfile command working directories.
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

fn read_optional_file(path: &Path) -> MonochangeResult<Option<Vec<u8>>> {
	match fs::read(path) {
		Ok(contents) => Ok(Some(contents)),
		Err(error)
			if matches!(
				error.kind(),
				std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
			) =>
		{
			Ok(None)
		}
		Err(error) => Err(MonochangeError::Io(format!(
			"failed to read {}: {error}",
			path.display()
		))),
	}
}

fn entry_file_type(entry: &fs::DirEntry, path: &Path) -> MonochangeResult<fs::FileType> {
	entry
		.file_type()
		.map_err(|error| MonochangeError::Io(format!("failed to stat {}: {error}", path.display())))
}

fn strip_workspace_prefix<'a>(path: &'a Path, root: &Path) -> MonochangeResult<&'a Path> {
	path.strip_prefix(root).map_err(|error| {
		MonochangeError::Config(format!(
			"path `{}` was outside workspace root `{}`: {error}",
			path.display(),
			root.display()
		))
	})
}

#[rustfmt::skip]
fn ensure_parent_directory(path: &Path) -> MonochangeResult<()> {
	if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|error| MonochangeError::Io(format!("failed to create {}: {error}", parent.display())))?; }
	Ok(())
}

fn copy_workspace_file(source: &Path, destination: &Path) -> MonochangeResult<()> {
	fs::copy(source, destination).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to copy {} to {}: {error}",
			source.display(),
			destination.display()
		))
	})?;
	Ok(())
}

#[rustfmt::skip]
fn collect_workspace_files(root: &Path, current: &Path, relative_paths: &mut BTreeSet<PathBuf>) -> MonochangeResult<()> {
	for entry in fs::read_dir(current).map_err(|error| MonochangeError::Io(format!("failed to read {}: {error}", current.display())))? {
		let entry = entry.map_err(|error| MonochangeError::Io(format!("directory entry error: {error}")))?;
		let path = entry.path();
		if path.file_name().is_some_and(|name| name == ".git") { continue; }
		let file_type = entry_file_type(&entry, &path)?;
		if file_type.is_dir() { collect_workspace_files(root, &path, relative_paths)?; continue; }
		if file_type.is_file() { relative_paths.insert(strip_workspace_prefix(&path, root)?.to_path_buf()); }
	}
	Ok(())
}

#[rustfmt::skip]
fn copy_workspace_tree(source: &Path, destination: &Path) -> MonochangeResult<()> {
	fs::create_dir_all(destination).map_err(|error| MonochangeError::Io(format!("failed to create {}: {error}", destination.display())))?;
	for entry in fs::read_dir(source).map_err(|error| MonochangeError::Io(format!("failed to read {}: {error}", source.display())))? {
		let entry = entry.map_err(|error| MonochangeError::Io(format!("directory entry error: {error}")))?;
		let source_path = entry.path();
		if source_path.file_name().is_some_and(|name| name == ".git") { continue; }
		let destination_path = destination.join(entry.file_name());
		let file_type = entry_file_type(&entry, &source_path)?;
		if file_type.is_dir() { copy_workspace_tree(&source_path, &destination_path)?; continue; }
		if file_type.is_file() { ensure_parent_directory(&destination_path)?; copy_workspace_file(&source_path, &destination_path)?; }
	}
	Ok(())
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
	let deno_updates = build_deno_manifest_updates(&discovery.packages, &plan)?;
	let dart_updates = build_dart_manifest_updates(&discovery.packages, &plan)?;
	let manifest_updates = [cargo_updates, npm_updates, deno_updates, dart_updates].concat();
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
	let lockfile_commands =
		build_lockfile_command_executions(root, &configuration, &discovery.packages, &plan)?;
	let file_updates = if lockfile_commands.is_empty() || dry_run {
		// During dry-run, skip the expensive workspace copy and lockfile
		// command execution. The base updates already contain all version
		// file and changelog changes; lockfile diffs are omitted from the
		// preview but this avoids copying the entire workspace to a temp
		// directory (which can take minutes for large repos).
		base_updates.clone()
	} else {
		materialize_lockfile_command_updates(root, &base_updates, &lockfile_commands)?
	};
	let mut changed_files = file_updates
		.iter()
		.map(|update| root_relative(root, &update.path))
		.collect::<Vec<_>>();
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
	let file_diffs = build_file_diff_previews(root, &file_updates)?;

	let version = shared_release_version(&plan);
	let group_version = shared_group_version(&plan);
	let mut deleted_changesets = Vec::new();
	if !dry_run {
		// When lockfile commands ran, materialize_lockfile_command_updates
		// already applied base_updates in-place. Only apply when we
		// skipped that path (no lockfile commands).
		if lockfile_commands.is_empty() {
			apply_file_updates(&file_updates)?;
		}
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

#[cfg(test)]
mod workspace_ops_tests {
	use super::*;
	#[cfg(unix)]
	use std::os::unix::fs::PermissionsExt;

	fn setup_workspace_ops_fixture() -> tempfile::TempDir {
		monochange_test_helpers::fs::setup_fixture_from(
			env!("CARGO_MANIFEST_DIR"),
			"workspace-ops/lockfile-command-helpers",
		)
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
				shell: monochange_core::ShellConfig::None,
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
				shell: monochange_core::ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected empty command error"));
		assert!(empty_error
			.to_string()
			.contains("lockfile command must not be empty"));

		let spawn_error = run_lockfile_command(
			fixture.path(),
			temp_root.path(),
			&LockfileCommandExecution {
				command: "definitely-not-a-real-command".to_string(),
				cwd: fixture.path().to_path_buf(),
				shell: monochange_core::ShellConfig::None,
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected spawn error"));
		assert!(spawn_error
			.to_string()
			.contains("failed to run lockfile command"));

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
				shell: monochange_core::ShellConfig::None,
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
				shell: monochange_core::ShellConfig::None,
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
				shell: monochange_core::ShellConfig::None,
			},
		)
		.unwrap_or_else(|error| panic!("write generated file: {error}"));

		let lockfile_cmd = LockfileCommandExecution {
			command: String::new(),
			cwd: fixture.path().to_path_buf(),
			shell: monochange_core::ShellConfig::None,
		};
		let updates =
			collect_workspace_file_updates(fixture.path(), temp_root.path(), &[], &[lockfile_cmd])
				.unwrap_or_else(|error| panic!("collect updates: {error}"));
		assert!(updates
			.iter()
			.any(|update| update.path.ends_with("generated.txt")));

		let mut paths = BTreeSet::new();
		collect_workspace_files(fixture.path(), fixture.path(), &mut paths)
			.unwrap_or_else(|error| panic!("collect files: {error}"));
		assert!(paths.contains(Path::new("root.txt")));
		assert!(!paths.iter().any(|path| path.starts_with(".git")));
	}

	#[test]
	fn file_helpers_report_missing_and_invalid_paths() {
		let fixture = setup_workspace_ops_fixture();
		assert!(read_optional_file(&fixture.path().join("missing.txt"))
			.unwrap_or_else(|error| panic!("missing file lookup: {error}"))
			.is_none());
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
		assert!(strip_prefix_error
			.to_string()
			.contains("was outside workspace root"));

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
}
