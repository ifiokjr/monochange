#![deny(clippy::all)]

//! # `monochange`
//!
//! <!-- {=monochangeCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange` is the top-level entry point for the workspace.
//!
//! Reach for this crate when you want one API and CLI surface that discovers packages across Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter workspaces, exposes top-level commands from `monochange.toml`, and runs configured release workflows from those definitions.
//!
//! ## Why use it?
//!
//! - coordinate one workflow-defined CLI across several package ecosystems
//! - expose discovery, change creation, and release preparation as both commands and library calls
//! - connect configuration loading, package discovery, graph propagation, and semver evidence in one place
//!
//! ## Best for
//!
//! - shipping the `mc` CLI in CI or local release tooling
//! - embedding the full end-to-end planner instead of wiring the lower-level crates together yourself
//! - generating starter config with `mc init` and then evolving the workflow surface over time
//!
//! ## Key commands
//!
//! ```bash
//! mc init
//! mc discover --format json
//! mc change --package crates/monochange --bump patch --reason "describe the change"
//! mc release --dry-run --format json
//! ```
//!
//! ## Responsibilities
//!
//! - aggregate all supported ecosystem adapters
//! - load `monochange.toml`
//! - synthesize default workflows when config does not declare any
//! - resolve change input files
//! - render discovery and release workflow output in text or JSON
//! - execute configured release workflows
//! <!-- {/monochangeCrateDocs} -->

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use clap::error::ErrorKind;
use clap::Arg;
use clap::ArgAction;
use clap::ArgMatches;
use clap::Command;
use clap::ValueEnum;
use monochange_cargo::discover_cargo_packages;
use monochange_cargo::RustSemverProvider;
use monochange_config::apply_version_groups;
use monochange_config::load_change_signals;
use monochange_config::load_workspace_configuration;
use monochange_config::resolve_package_reference;
use monochange_config::validate_workspace;
use monochange_core::default_workflows;
use monochange_core::materialize_dependency_edges;
use monochange_core::relative_to_root;
use monochange_core::render_release_notes;
use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogTarget;
use monochange_core::CommandVariable;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::PackageType;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseManifestCompatibilityEvidence;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestPlanDecision;
use monochange_core::ReleaseManifestPlanGroup;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseNotesSection;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleasePlan;
use monochange_core::VersionFormat;
use monochange_core::VersionedFileDefinition;
use monochange_core::WorkflowDefinition;
use monochange_core::WorkflowInputDefinition;
use monochange_core::WorkflowInputKind;
use monochange_core::WorkflowStepDefinition;
use monochange_core::WorkspaceDefaults;
use monochange_dart::discover_dart_packages;
use monochange_deno::discover_deno_packages;
use monochange_graph::build_release_plan;
use monochange_npm::discover_npm_packages;
use monochange_semver::collect_assessments;
use monochange_semver::CompatibilityProvider;
use serde::Serialize;
use serde_json::json;
use toml::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
	Text,
	Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ChangeBump {
	Patch,
	Minor,
	Major,
}

impl From<ChangeBump> for BumpSeverity {
	fn from(value: ChangeBump) -> Self {
		match value {
			ChangeBump::Patch => Self::Patch,
			ChangeBump::Minor => Self::Minor,
			ChangeBump::Major => Self::Major,
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReleaseTarget {
	pub id: String,
	pub kind: ReleaseOwnerKind,
	pub version: String,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
	pub tag_name: String,
	pub members: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PreparedChangelog {
	pub owner_id: String,
	pub owner_kind: ReleaseOwnerKind,
	pub path: PathBuf,
	pub format: ChangelogFormat,
	pub notes: ReleaseNotesDocument,
	pub rendered: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PreparedRelease {
	pub plan: ReleasePlan,
	pub changeset_paths: Vec<PathBuf>,
	pub released_packages: Vec<String>,
	pub version: Option<String>,
	pub group_version: Option<String>,
	pub release_targets: Vec<ReleaseTarget>,
	pub changed_files: Vec<PathBuf>,
	pub changelogs: Vec<PreparedChangelog>,
	pub updated_changelogs: Vec<PathBuf>,
	pub deleted_changesets: Vec<PathBuf>,
	pub dry_run: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct FileUpdate {
	path: PathBuf,
	content: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ChangelogUpdate {
	file: FileUpdate,
	owner_id: String,
	owner_kind: ReleaseOwnerKind,
	format: ChangelogFormat,
	notes: ReleaseNotesDocument,
	rendered: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct WorkflowContext {
	root: PathBuf,
	dry_run: bool,
	inputs: BTreeMap<String, Vec<String>>,
	prepared_release: Option<PreparedRelease>,
	release_manifest_path: Option<PathBuf>,
	command_logs: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InitWorkspaceConfiguration {
	defaults: WorkspaceDefaults,
	#[serde(skip_serializing_if = "BTreeMap::is_empty")]
	package: BTreeMap<String, InitPackageDefinition>,
	#[serde(skip_serializing_if = "BTreeMap::is_empty")]
	group: BTreeMap<String, InitGroupDefinition>,
	workflows: Vec<WorkflowDefinition>,
}

#[derive(Debug, Serialize)]
struct InitPackageDefinition {
	path: PathBuf,
	#[serde(rename = "type")]
	package_type: PackageType,
	#[serde(skip_serializing_if = "Option::is_none")]
	changelog: Option<PathBuf>,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	versioned_files: Vec<VersionedFileDefinition>,
}

#[derive(Debug, Serialize)]
struct InitGroupDefinition {
	packages: Vec<String>,
	tag: bool,
	release: bool,
	version_format: VersionFormat,
}

const CHANGESET_DIR: &str = ".changeset";

pub fn build_command(bin_name: &'static str) -> Command {
	let root = current_dir_or_dot();
	build_command_for_root(bin_name, &root)
}

fn build_command_for_root(bin_name: &'static str, root: &Path) -> Command {
	let workflows = load_workspace_configuration(root).map_or_else(
		|_| default_workflows(),
		|configuration| configuration.workflows,
	);
	build_command_with_workflows(bin_name, &workflows)
}

fn build_command_with_workflows(
	bin_name: &'static str,
	workflows: &[WorkflowDefinition],
) -> Command {
	let mut command = Command::new(bin_name)
		.about("Manage versions and releases for your multiplatform, multilanguage monorepo")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(
			Command::new("init")
				.about("Generate monochange.toml with detected packages, groups, and default workflows")
				.arg(
					Arg::new("force")
						.long("force")
						.help("Overwrite an existing monochange.toml file")
						.action(ArgAction::SetTrue),
				),
		);

	for workflow in workflows {
		command = command.subcommand(build_workflow_subcommand(workflow));
	}

	command
}

fn build_workflow_subcommand(workflow: &WorkflowDefinition) -> Command {
	let mut command = Command::new(leak_string(workflow.name.clone()))
		.about(
			workflow
				.help_text
				.clone()
				.unwrap_or_else(|| format!("Run the `{}` workflow", workflow.name)),
		)
		.arg(
			Arg::new("dry-run")
				.long("dry-run")
				.help("Run the workflow in dry-run mode when supported")
				.action(ArgAction::SetTrue),
		);

	for input in &workflow.inputs {
		command = command.arg(build_workflow_input_arg(input));
	}

	command
}

fn build_workflow_input_arg(input: &WorkflowInputDefinition) -> Arg {
	let long_name = leak_string(input.name.replace('_', "-"));
	let value_name = leak_string(input.name.to_uppercase());
	let mut arg = Arg::new(leak_string(input.name.clone()))
		.long(long_name)
		.required(input.required)
		.help(input.help_text.clone().unwrap_or_default());

	arg = match input.kind {
		WorkflowInputKind::String => arg.value_name(value_name),
		WorkflowInputKind::StringList => arg.value_name(value_name).action(ArgAction::Append),
		WorkflowInputKind::Path => arg.value_name("PATH"),
		WorkflowInputKind::Choice => {
			arg.value_name(value_name)
				.value_parser(clap::builder::PossibleValuesParser::new(
					input
						.choices
						.iter()
						.cloned()
						.map(leak_string)
						.collect::<Vec<_>>(),
				))
		}
	};

	if let Some(default) = &input.default {
		arg = arg.default_value(leak_string(default.clone()));
	}

	arg
}

fn leak_string(value: impl Into<String>) -> &'static str {
	Box::leak(value.into().into_boxed_str())
}

fn current_dir_or_dot() -> PathBuf {
	std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn run_from_env(bin_name: &'static str) -> MonochangeResult<()> {
	let args = std::env::args_os();
	let output = run_with_args(bin_name, args)?;
	println!("{output}");
	Ok(())
}

pub fn run_with_args<I>(bin_name: &'static str, args: I) -> MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	let root = current_dir_or_dot();
	run_with_args_in_dir(bin_name, args, &root)
}

fn run_with_args_in_dir<I>(bin_name: &'static str, args: I, root: &Path) -> MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	let args = args.into_iter().collect::<Vec<_>>();
	let configuration = load_workspace_configuration(root);
	let workflows = configuration
		.as_ref()
		.map_or_else(|_| default_workflows(), |loaded| loaded.workflows.clone());
	let matches =
		match build_command_with_workflows(bin_name, &workflows).try_get_matches_from(args) {
			Ok(matches) => matches,
			Err(error)
				if matches!(
					error.kind(),
					ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
				) =>
			{
				return Ok(error.to_string());
			}
			Err(error) => return Err(MonochangeError::Config(error.to_string())),
		};

	match matches.subcommand() {
		Some(("init", init_matches)) => {
			let path = init_workspace(root, init_matches.get_flag("force"))?;
			Ok(format!("wrote {}", path.display()))
		}
		Some((workflow_name, workflow_matches)) => {
			let configuration = configuration?;
			execute_matches(root, &configuration, workflow_name, workflow_matches)
		}
		None => Err(MonochangeError::Config("unknown command".to_string())),
	}
}

fn execute_matches(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	workflow_name: &str,
	workflow_matches: &ArgMatches,
) -> MonochangeResult<String> {
	let Some(workflow) = configuration
		.workflows
		.iter()
		.find(|workflow| workflow.name == workflow_name)
	else {
		return Err(MonochangeError::Config(format!(
			"unknown command `{workflow_name}`"
		)));
	};

	let dry_run = workflow_matches.get_flag("dry-run");
	let inputs = collect_workflow_inputs(workflow, workflow_matches);
	execute_workflow(root, workflow, dry_run, inputs)
}

fn collect_workflow_inputs(
	workflow: &WorkflowDefinition,
	matches: &ArgMatches,
) -> BTreeMap<String, Vec<String>> {
	let mut inputs = BTreeMap::new();
	for input in &workflow.inputs {
		let values = match input.kind {
			WorkflowInputKind::StringList => matches
				.get_many::<String>(input.name.as_str())
				.map(|values| values.cloned().collect::<Vec<_>>())
				.unwrap_or_default(),
			WorkflowInputKind::String | WorkflowInputKind::Path | WorkflowInputKind::Choice => {
				matches
					.get_one::<String>(input.name.as_str())
					.map(|value| vec![value.clone()])
					.unwrap_or_default()
			}
		};
		inputs.insert(input.name.clone(), values);
	}
	inputs
}

fn execute_workflow(
	root: &Path,
	workflow: &WorkflowDefinition,
	dry_run: bool,
	inputs: BTreeMap<String, Vec<String>>,
) -> MonochangeResult<String> {
	let mut context = WorkflowContext {
		root: root.to_path_buf(),
		dry_run,
		inputs,
		prepared_release: None,
		release_manifest_path: None,
		command_logs: Vec::new(),
	};
	let mut output = None;

	for step in &workflow.steps {
		match step {
			WorkflowStepDefinition::Validate => {
				validate_workspace(root)?;
				output = Some(format!(
					"workspace validation passed for {}",
					root_relative(root, root).display()
				));
			}
			WorkflowStepDefinition::Discover => {
				let format = context
					.inputs
					.get("format")
					.and_then(|values| values.first())
					.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;
				output = Some(render_discovery_report(&discover_workspace(root)?, format)?);
			}
			WorkflowStepDefinition::CreateChangeFile => {
				let package_refs = context.inputs.get("package").cloned().unwrap_or_default();
				if package_refs.is_empty() {
					return Err(MonochangeError::Config(
						"workflow `change` requires at least one `--package` value".to_string(),
					));
				}
				let bump = context
					.inputs
					.get("bump")
					.and_then(|values| values.first())
					.map_or(Ok(ChangeBump::Patch), |value| parse_change_bump(value))?;
				let reason = context
					.inputs
					.get("reason")
					.and_then(|values| values.first())
					.cloned()
					.ok_or_else(|| {
						MonochangeError::Config(
							"workflow `change` requires a `--reason` value".to_string(),
						)
					})?;
				let evidence = context.inputs.get("evidence").cloned().unwrap_or_default();
				let output_path = context
					.inputs
					.get("output")
					.and_then(|values| values.first())
					.map(PathBuf::from);
				let path = add_change_file(
					root,
					&package_refs,
					bump.into(),
					&reason,
					&evidence,
					output_path.as_deref(),
				)?;
				output = Some(format!(
					"wrote change file {}",
					root_relative(root, &path).display()
				));
			}
			WorkflowStepDefinition::PrepareRelease => {
				context.prepared_release = Some(prepare_release(root, dry_run)?);
				output = None;
			}
			WorkflowStepDefinition::RenderReleaseManifest { path } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`RenderReleaseManifest` requires a previous `PrepareRelease` step"
							.to_string(),
					)
				})?;
				let manifest =
					build_release_manifest(workflow, prepared_release, &context.command_logs);
				if let Some(path) = path {
					let resolved_path = resolve_config_path(root, path);
					let rendered = render_release_manifest_json(&manifest)?;
					apply_file_updates(&[FileUpdate {
						path: resolved_path.clone(),
						content: rendered,
					}])?;
					context.release_manifest_path = Some(root_relative(root, &resolved_path));
				}
				output = None;
			}
			WorkflowStepDefinition::Command {
				command,
				dry_run,
				shell,
				variables,
			} => run_workflow_command(
				&mut context,
				command,
				dry_run.as_deref(),
				*shell,
				variables.as_ref(),
			)?,
		}
	}

	if let Some(prepared_release) = &context.prepared_release {
		let format = context
			.inputs
			.get("format")
			.and_then(|values| values.first())
			.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;
		return match format {
			OutputFormat::Json => render_release_manifest_json(&build_release_manifest(
				workflow,
				prepared_release,
				&context.command_logs,
			)),
			OutputFormat::Text => Ok(render_workflow_result(workflow, &context)),
		};
	}
	if !context.command_logs.is_empty() {
		return Ok(render_workflow_result(workflow, &context));
	}

	Ok(output.unwrap_or_else(|| {
		format!(
			"workflow `{}` completed{}",
			workflow.name,
			if dry_run { " (dry-run)" } else { "" }
		)
	}))
}

fn run_workflow_command(
	context: &mut WorkflowContext,
	command: &str,
	dry_run_command: Option<&str>,
	shell: bool,
	variables: Option<&BTreeMap<String, CommandVariable>>,
) -> MonochangeResult<()> {
	let command_to_run = if context.dry_run {
		if let Some(command) = dry_run_command {
			command
		} else {
			let skipped = interpolate_workflow_command(context, command, variables);
			context
				.command_logs
				.push(format!("skipped command `{skipped}` (dry-run)"));
			return Ok(());
		}
	} else {
		command
	};
	let interpolated = interpolate_workflow_command(context, command_to_run, variables);

	let output = if shell {
		ProcessCommand::new("sh")
			.arg("-c")
			.arg(&interpolated)
			.current_dir(&context.root)
			.output()
	} else {
		let parts = shlex::split(&interpolated).ok_or_else(|| {
			MonochangeError::Config(format!("failed to parse workflow command `{interpolated}`"))
		})?;
		let Some((program, args)) = parts.split_first() else {
			return Err(MonochangeError::Config(
				"workflow command must not be empty".to_string(),
			));
		};
		ProcessCommand::new(program)
			.args(args)
			.current_dir(&context.root)
			.output()
	};
	let output = output.map_err(|error| {
		MonochangeError::Io(format!(
			"failed to run workflow command `{interpolated}`: {error}"
		))
	})?;
	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
		let details = if stderr.is_empty() {
			format!("exit status {}", output.status)
		} else {
			stderr
		};
		return Err(MonochangeError::Discovery(format!(
			"workflow command `{interpolated}` failed: {details}"
		)));
	}

	let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
	if stdout.is_empty() {
		context.command_logs.push(format!("ran `{interpolated}`"));
	} else {
		context.command_logs.push(stdout);
	}
	Ok(())
}

fn interpolate_workflow_command(
	context: &WorkflowContext,
	command: &str,
	variables: Option<&BTreeMap<String, CommandVariable>>,
) -> String {
	if let Some(variables) = variables {
		let mut interpolated = command.to_string();
		for (needle, variable) in variables {
			interpolated =
				interpolated.replace(needle, &workflow_variable_value(context, *variable));
		}
		return interpolated;
	}

	command
		.replace(
			"$group_version",
			&workflow_variable_value(context, CommandVariable::GroupVersion),
		)
		.replace(
			"$released_packages",
			&workflow_variable_value(context, CommandVariable::ReleasedPackages),
		)
		.replace(
			"$changed_files",
			&workflow_variable_value(context, CommandVariable::ChangedFiles),
		)
		.replace(
			"$changesets",
			&workflow_variable_value(context, CommandVariable::Changesets),
		)
		.replace(
			"$version",
			&workflow_variable_value(context, CommandVariable::Version),
		)
}

fn workflow_variable_value(context: &WorkflowContext, variable: CommandVariable) -> String {
	let version = context
		.prepared_release
		.as_ref()
		.and_then(|prepared| prepared.version.as_deref())
		.unwrap_or("");
	let group_version = context
		.prepared_release
		.as_ref()
		.and_then(|prepared| prepared.group_version.as_deref())
		.unwrap_or(version);
	match variable {
		CommandVariable::Version => version.to_string(),
		CommandVariable::GroupVersion => group_version.to_string(),
		CommandVariable::ReleasedPackages => context
			.prepared_release
			.as_ref()
			.map(|prepared| prepared.released_packages.join(","))
			.unwrap_or_default(),
		CommandVariable::ChangedFiles => context
			.prepared_release
			.as_ref()
			.map(|prepared| {
				prepared
					.changed_files
					.iter()
					.map(|path| path.display().to_string())
					.collect::<Vec<_>>()
					.join(" ")
			})
			.unwrap_or_default(),
		CommandVariable::Changesets => context
			.prepared_release
			.as_ref()
			.map(|prepared| {
				prepared
					.changeset_paths
					.iter()
					.map(|path| path.display().to_string())
					.collect::<Vec<_>>()
					.join(" ")
			})
			.unwrap_or_default(),
	}
}

fn render_workflow_result(workflow: &WorkflowDefinition, context: &WorkflowContext) -> String {
	let mut lines = vec![format!(
		"workflow `{}` completed{}",
		workflow.name,
		if context.dry_run { " (dry-run)" } else { "" }
	)];
	if let Some(prepared_release) = &context.prepared_release {
		if let Some(version) = &prepared_release.version {
			lines.push(format!("version: {version}"));
		}
		if !prepared_release.released_packages.is_empty() {
			lines.push(format!(
				"released packages: {}",
				prepared_release.released_packages.join(", ")
			));
		}
		if !prepared_release.release_targets.is_empty() {
			lines.push("release targets:".to_string());
			for target in &prepared_release.release_targets {
				lines.push(format!(
					"- {} {} -> {} (tag: {}, release: {})",
					target.kind, target.id, target.tag_name, target.tag, target.release,
				));
			}
		}
		if let Some(path) = &context.release_manifest_path {
			lines.push(format!("release manifest: {}", path.display()));
		}
		if !prepared_release.changed_files.is_empty() {
			lines.push("changed files:".to_string());
			for path in &prepared_release.changed_files {
				lines.push(format!("- {}", path.display()));
			}
		}
		if !prepared_release.deleted_changesets.is_empty() {
			lines.push("deleted changesets:".to_string());
			for path in &prepared_release.deleted_changesets {
				lines.push(format!("- {}", path.display()));
			}
		}
	}
	if !context.command_logs.is_empty() {
		lines.push("workflow commands:".to_string());
		for log in &context.command_logs {
			lines.push(format!("- {log}"));
		}
	}
	lines.join(
		"
",
	)
}

fn parse_output_format(value: &str) -> MonochangeResult<OutputFormat> {
	match value {
		"text" => Ok(OutputFormat::Text),
		"json" => Ok(OutputFormat::Json),
		other => Err(MonochangeError::Config(format!(
			"unsupported output format `{other}`"
		))),
	}
}

fn parse_change_bump(value: &str) -> MonochangeResult<ChangeBump> {
	match value {
		"patch" => Ok(ChangeBump::Patch),
		"minor" => Ok(ChangeBump::Minor),
		"major" => Ok(ChangeBump::Major),
		other => Err(MonochangeError::Config(format!(
			"unsupported bump `{other}`"
		))),
	}
}

fn init_workspace(root: &Path, force: bool) -> MonochangeResult<PathBuf> {
	let path = monochange_config::config_path(root);
	if path.exists() && !force {
		return Err(MonochangeError::Config(format!(
			"{} already exists; rerun with --force to overwrite it",
			path.display()
		)));
	}

	let config = synthesize_init_configuration(root)?;
	let content = toml::to_string_pretty(&config)
		.map_err(|error| MonochangeError::Config(error.to_string()))?;
	fs::write(&path, content).map_err(|error| {
		MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
	})?;
	Ok(path)
}

fn synthesize_init_configuration(root: &Path) -> MonochangeResult<InitWorkspaceConfiguration> {
	let packages = discover_packages(root)?;
	let mut package_configs = BTreeMap::new();
	let mut package_ids = Vec::new();
	let mut name_counts = BTreeMap::<String, usize>::new();

	for package in packages {
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
		let changelog = detect_default_changelog(root, &manifest_dir);
		package_configs.insert(
			id,
			InitPackageDefinition {
				path: relative_dir,
				package_type: package_type_for_ecosystem(package.ecosystem),
				changelog,
				versioned_files: Vec::new(),
			},
		);
	}

	let mut group_configs = BTreeMap::new();
	if package_ids.len() > 1 {
		group_configs.insert(
			"main".to_string(),
			InitGroupDefinition {
				packages: package_ids,
				tag: true,
				release: true,
				version_format: VersionFormat::Primary,
			},
		);
	}

	Ok(InitWorkspaceConfiguration {
		defaults: WorkspaceDefaults::default(),
		package: package_configs,
		group: group_configs,
		workflows: default_workflows(),
	})
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

pub fn add_change_file(
	root: &Path,
	package_refs: &[String],
	bump: BumpSeverity,
	reason: &str,
	evidence: &[String],
	output: Option<&Path>,
) -> MonochangeResult<PathBuf> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;
	let packages =
		canonical_change_packages(root, package_refs, &configuration, &discovery.packages)?;
	let output_path =
		output.map_or_else(|| default_change_path(root, &packages), Path::to_path_buf);
	if let Some(parent) = output_path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
	}

	let content = render_changeset_markdown(&packages, bump, reason, evidence);
	fs::write(&output_path, content).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write {}: {error}",
			output_path.display()
		))
	})?;
	Ok(output_path)
}

pub fn plan_release(root: &Path, changes_path: &Path) -> MonochangeResult<ReleasePlan> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;
	let change_signals = load_change_signals(changes_path, &configuration, &discovery.packages)?;
	Ok(build_release_plan_from_signals(
		&configuration,
		&discovery,
		&change_signals,
	))
}

pub fn prepare_release(root: &Path, dry_run: bool) -> MonochangeResult<PreparedRelease> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;
	let changeset_paths = discover_changeset_paths(root)?;
	let change_signals = changeset_paths
		.iter()
		.try_fold(Vec::new(), |mut signals, path| {
			signals.extend(load_change_signals(
				path,
				&configuration,
				&discovery.packages,
			)?);
			Ok::<_, MonochangeError>(signals)
		})?;
	let plan = build_release_plan_from_signals(&configuration, &discovery, &change_signals);
	let released_packages = released_package_names(&discovery.packages, &plan);
	if released_packages.is_empty() {
		return Err(MonochangeError::Config(
			"no releaseable packages were found in discovered changesets".to_string(),
		));
	}

	let changelog_targets = resolve_changelog_targets(&configuration, &discovery.packages)?;
	let manifest_updates = build_cargo_manifest_updates(&discovery.packages, &plan)?;
	let versioned_file_updates =
		build_versioned_file_updates(root, &configuration, &discovery.packages, &plan)?;
	let changelog_updates = build_changelog_updates(
		root,
		&configuration,
		&discovery.packages,
		&plan,
		&change_signals,
		&changelog_targets,
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

	let version = shared_release_version(&plan);
	let group_version = shared_group_version(&plan);
	let release_targets = build_release_targets(&configuration, &discovery.packages, &plan);
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

	Ok(PreparedRelease {
		plan,
		changeset_paths,
		released_packages,
		version,
		group_version,
		release_targets,
		changed_files,
		changelogs,
		updated_changelogs,
		deleted_changesets,
		dry_run,
	})
}

fn discover_changeset_paths(root: &Path) -> MonochangeResult<Vec<PathBuf>> {
	let changeset_dir = root.join(CHANGESET_DIR);
	if !changeset_dir.exists() {
		return Err(MonochangeError::Config(format!(
			"no markdown changesets found under {CHANGESET_DIR}"
		)));
	}

	let mut changeset_paths = fs::read_dir(&changeset_dir)
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read {}: {error}",
				changeset_dir.display()
			))
		})?
		.filter_map(Result::ok)
		.map(|entry| entry.path())
		.filter(|path| path.extension().and_then(|value| value.to_str()) == Some("md"))
		.collect::<Vec<_>>();
	changeset_paths.sort();
	if changeset_paths.is_empty() {
		return Err(MonochangeError::Config(format!(
			"no markdown changesets found under {CHANGESET_DIR}"
		)));
	}
	Ok(changeset_paths)
}

fn build_release_plan_from_signals(
	configuration: &monochange_core::WorkspaceConfiguration,
	discovery: &DiscoveryReport,
	change_signals: &[ChangeSignal],
) -> ReleasePlan {
	let rust_provider = RustSemverProvider;
	let providers: [&dyn CompatibilityProvider; 1] = [&rust_provider];
	let compatibility_evidence =
		collect_assessments(&providers, &discovery.packages, change_signals);

	build_release_plan(
		&discovery.workspace_root,
		&discovery.packages,
		&discovery.dependencies,
		&discovery.version_groups,
		change_signals,
		&compatibility_evidence,
		configuration.defaults.parent_bump,
	)
}

fn canonical_change_packages(
	root: &Path,
	package_refs: &[String],
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> MonochangeResult<Vec<String>> {
	let mut canonical_packages = Vec::new();
	for package_ref in package_refs {
		let canonical_key = if configuration
			.groups
			.iter()
			.any(|group| group.id == *package_ref)
			|| configuration
				.packages
				.iter()
				.any(|package| package.id == *package_ref)
		{
			package_ref.clone()
		} else {
			let package_id = resolve_package_reference(package_ref, root, packages)?;
			let package = packages
				.iter()
				.find(|package| package.id == package_id)
				.ok_or_else(|| {
					MonochangeError::Config(format!("failed to resolve package `{package_ref}`"))
				})?;
			package
				.metadata
				.get("config_id")
				.cloned()
				.unwrap_or_else(|| package.name.clone())
		};
		if !canonical_packages.contains(&canonical_key) {
			canonical_packages.push(canonical_key);
		}
	}
	Ok(canonical_packages)
}

fn released_package_names(packages: &[PackageRecord], plan: &ReleasePlan) -> Vec<String> {
	let mut released_packages = plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
		.filter_map(|decision| {
			packages
				.iter()
				.find(|package| package.id == decision.package_id)
				.map(|package| package.name.clone())
		})
		.collect::<Vec<_>>();
	released_packages.sort();
	released_packages.dedup();
	released_packages
}

type PackageChangelogTargets = BTreeMap<String, ChangelogTarget>;
type GroupChangelogTargets = BTreeMap<String, ChangelogTarget>;

fn resolve_changelog_targets(
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> MonochangeResult<(PackageChangelogTargets, GroupChangelogTargets)> {
	let mut package_targets = BTreeMap::new();
	let mut group_targets = BTreeMap::new();

	for package_definition in &configuration.packages {
		let Some(changelog_path) = &package_definition.changelog else {
			continue;
		};
		let package_id =
			resolve_package_reference(&package_definition.id, &configuration.root_path, packages)?;
		package_targets.insert(
			package_id,
			ChangelogTarget {
				path: resolve_config_path(&configuration.root_path, &changelog_path.path),
				format: changelog_path.format,
			},
		);
	}
	for group_definition in &configuration.groups {
		let Some(changelog_path) = &group_definition.changelog else {
			continue;
		};
		group_targets.insert(
			group_definition.id.clone(),
			ChangelogTarget {
				path: resolve_config_path(&configuration.root_path, &changelog_path.path),
				format: changelog_path.format,
			},
		);
	}

	Ok((package_targets, group_targets))
}

fn resolve_config_path(root: &Path, path: &Path) -> PathBuf {
	if path.is_absolute() {
		path.to_path_buf()
	} else {
		root.join(path)
	}
}

fn build_changelog_updates(
	_root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
	change_signals: &[ChangeSignal],
	changelog_targets: &(PackageChangelogTargets, GroupChangelogTargets),
) -> MonochangeResult<Vec<ChangelogUpdate>> {
	let mut notes_by_package = BTreeMap::<String, BTreeSet<String>>::new();
	for signal in change_signals {
		if let Some(note) = &signal.notes {
			notes_by_package
				.entry(signal.package_id.clone())
				.or_default()
				.insert(note.clone());
		}
	}

	let mut updates = Vec::new();
	let package_changelog_targets = &changelog_targets.0;
	let group_changelog_targets = &changelog_targets.1;
	for decision in plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
	{
		let Some(changelog_target) = package_changelog_targets.get(&decision.package_id) else {
			continue;
		};
		let Some(package) = packages
			.iter()
			.find(|package| package.id == decision.package_id)
		else {
			continue;
		};
		let Some(planned_version) = decision.planned_version.as_ref() else {
			continue;
		};
		let notes = notes_by_package
			.get(&decision.package_id)
			.map(|notes| notes.iter().cloned().collect::<Vec<_>>())
			.filter(|notes| !notes.is_empty())
			.unwrap_or_else(|| decision.reasons.clone());
		let document = package_release_notes(&package.name, &planned_version.to_string(), &notes);
		let rendered = render_release_notes(changelog_target.format, &document);
		updates.push(ChangelogUpdate {
			file: FileUpdate {
				path: changelog_target.path.clone(),
				content: append_changelog_section(&changelog_target.path, &rendered)?,
			},
			owner_id: decision.package_id.clone(),
			owner_kind: ReleaseOwnerKind::Package,
			format: changelog_target.format,
			notes: document,
			rendered,
		});
	}

	for planned_group in plan
		.groups
		.iter()
		.filter(|group| group.recommended_bump.is_release())
	{
		let Some(changelog_target) = group_changelog_targets.get(&planned_group.group_id) else {
			continue;
		};
		let Some(planned_version) = planned_group.planned_version.as_ref() else {
			continue;
		};
		let member_notes = planned_group
			.members
			.iter()
			.filter_map(|member_id| notes_by_package.get(member_id))
			.flat_map(BTreeSet::iter)
			.cloned()
			.collect::<BTreeSet<_>>()
			.into_iter()
			.collect::<Vec<_>>();
		let member_ids = configuration
			.groups
			.iter()
			.find(|group| group.id == planned_group.group_id)
			.map(|group| group.packages.clone())
			.unwrap_or_default();
		let notes = if member_notes.is_empty() {
			vec![format!(
				"prepare grouped release for `{}`",
				planned_group.group_id
			)]
		} else {
			member_notes
		};
		let document = group_release_notes(
			&planned_group.group_id,
			&planned_version.to_string(),
			&member_ids,
			&notes,
		);
		let rendered = render_release_notes(changelog_target.format, &document);
		updates.push(ChangelogUpdate {
			file: FileUpdate {
				path: changelog_target.path.clone(),
				content: append_changelog_section(&changelog_target.path, &rendered)?,
			},
			owner_id: planned_group.group_id.clone(),
			owner_kind: ReleaseOwnerKind::Group,
			format: changelog_target.format,
			notes: document,
			rendered,
		});
	}

	Ok(dedup_changelog_updates(updates))
}

fn append_changelog_section(path: &Path, section: &str) -> MonochangeResult<String> {
	let current = if path.exists() {
		fs::read_to_string(path).map_err(|error| {
			MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
		})?
	} else {
		String::new()
	};
	let mut content = current.trim_end().to_string();
	if !content.is_empty() {
		content.push_str("\n\n");
	}
	content.push_str(section);
	content.push('\n');
	Ok(content)
}

fn dedup_changelog_updates(updates: Vec<ChangelogUpdate>) -> Vec<ChangelogUpdate> {
	updates
		.into_iter()
		.fold(
			BTreeMap::<PathBuf, ChangelogUpdate>::new(),
			|mut acc, update| {
				acc.insert(update.file.path.clone(), update);
				acc
			},
		)
		.into_values()
		.collect()
}

fn package_release_notes(
	package_name: &str,
	version: &str,
	notes: &[String],
) -> ReleaseNotesDocument {
	ReleaseNotesDocument {
		title: version.to_string(),
		summary: Vec::new(),
		sections: vec![ReleaseNotesSection {
			title: "Changed".to_string(),
			entries: if notes.is_empty() {
				vec![format!("prepare release for `{package_name}`")]
			} else {
				notes.to_vec()
			},
		}],
	}
}

fn group_release_notes(
	group_name: &str,
	version: &str,
	members: &[String],
	notes: &[String],
) -> ReleaseNotesDocument {
	let mut summary = vec![format!("Grouped release for `{group_name}`.")];
	if !members.is_empty() {
		summary.push(format!("Members: {}", members.join(", ")));
	}
	ReleaseNotesDocument {
		title: version.to_string(),
		summary,
		sections: vec![ReleaseNotesSection {
			title: "Changed".to_string(),
			entries: notes.to_vec(),
		}],
	}
}

struct VersionedFileUpdateContext<'a> {
	package_definitions_by_id: BTreeMap<&'a str, &'a monochange_core::PackageDefinition>,
	package_by_record_id: BTreeMap<&'a str, &'a PackageRecord>,
	released_versions_by_native_name: BTreeMap<String, String>,
}

fn build_versioned_file_updates(
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
	let package_definitions_by_id = configuration
		.packages
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
		package_definitions_by_id,
		package_by_record_id,
		released_versions_by_native_name,
	};
	let mut updates = BTreeMap::<PathBuf, Value>::new();

	for package_definition in &configuration.packages {
		let Some(version) = released_versions_by_config_id.get(&package_definition.id) else {
			continue;
		};
		for versioned_file in &package_definition.versioned_files {
			apply_versioned_file_definition(
				root,
				&mut updates,
				versioned_file,
				package_definition.id.as_str(),
				version,
				shared_release_version.as_ref(),
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
		for versioned_file in &group_definition.versioned_files {
			apply_versioned_file_definition(
				root,
				&mut updates,
				versioned_file,
				group_definition.id.as_str(),
				&group_version,
				Some(&group_version),
				&context,
			)?;
		}
	}

	updates
		.into_iter()
		.map(|(path, document)| {
			toml::to_string_pretty(&document)
				.map(|content| FileUpdate { path, content })
				.map_err(|error| MonochangeError::Config(error.to_string()))
		})
		.collect()
}

fn apply_versioned_file_definition(
	root: &Path,
	updates: &mut BTreeMap<PathBuf, Value>,
	definition: &VersionedFileDefinition,
	owner_id: &str,
	owner_version: &str,
	shared_release_version: Option<&String>,
	context: &VersionedFileUpdateContext<'_>,
) -> MonochangeResult<()> {
	match definition {
		VersionedFileDefinition::Path(path) => {
			let resolved_path = resolve_config_path(root, path);
			let mut document = if let Some(document) = updates.remove(&resolved_path) {
				document
			} else {
				read_toml_document(&resolved_path)?
			};
			update_document_for_release_file(
				&mut document,
				owner_id,
				owner_version,
				&context.released_versions_by_native_name,
				shared_release_version.map(String::as_str),
			);
			updates.insert(resolved_path, document);
		}
		VersionedFileDefinition::Dependency { path, dependency } => {
			let Some(package_definition) =
				context.package_definitions_by_id.get(dependency.as_str())
			else {
				return Err(MonochangeError::Config(format!(
					"versioned file dependency `{dependency}` is not a declared package"
				)));
			};
			let dependency_native_name = context
				.package_by_record_id
				.values()
				.find(|package| package.metadata.get("config_id") == Some(&package_definition.id))
				.map_or_else(|| dependency.clone(), |package| package.name.clone());
			let Some(version) = context
				.released_versions_by_native_name
				.get(&dependency_native_name)
			else {
				return Ok(());
			};
			let resolved_path = resolve_config_path(root, path);
			let mut document = if let Some(document) = updates.remove(&resolved_path) {
				document
			} else {
				read_toml_document(&resolved_path)?
			};
			let single_dependency = BTreeMap::from([(dependency_native_name, version.clone())]);
			update_document_dependencies(&mut document, &single_dependency);
			updates.insert(resolved_path, document);
		}
	}
	Ok(())
}

fn update_document_for_release_file(
	document: &mut Value,
	owner_id: &str,
	owner_version: &str,
	released_versions_by_native_name: &BTreeMap<String, String>,
	shared_release_version: Option<&str>,
) {
	if let Some(package_table) = document.get_mut("package").and_then(Value::as_table_mut) {
		package_table.insert(
			"version".to_string(),
			Value::String(owner_version.to_string()),
		);
		let _ = owner_id;
	}
	if let Some(workspace_table) = document.get_mut("workspace").and_then(Value::as_table_mut) {
		if let Some(workspace_package_table) = workspace_table
			.get_mut("package")
			.and_then(Value::as_table_mut)
		{
			if let Some(shared_release_version) = shared_release_version {
				workspace_package_table.insert(
					"version".to_string(),
					Value::String(shared_release_version.to_string()),
				);
			}
		}
	}
	update_document_dependencies(document, released_versions_by_native_name);
}

fn update_document_dependencies(
	document: &mut Value,
	released_versions_by_native_name: &BTreeMap<String, String>,
) {
	for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
		update_dependency_table(document, section, released_versions_by_native_name);
	}
	if let Some(workspace_table) = document.get_mut("workspace").and_then(Value::as_table_mut) {
		if let Some(workspace_dependency_table) = workspace_table
			.get_mut("dependencies")
			.and_then(Value::as_table_mut)
		{
			for (package_name, version) in released_versions_by_native_name {
				let Some(entry) = workspace_dependency_table.get_mut(package_name) else {
					continue;
				};
				if let Some(entry_table) = entry.as_table_mut() {
					entry_table.insert("version".to_string(), Value::String(version.clone()));
				} else {
					*entry = Value::String(version.clone());
				}
			}
		}
	}
}

fn released_versions_by_record_id(plan: &ReleasePlan) -> BTreeMap<String, String> {
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

fn build_release_targets(
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> Vec<ReleaseTarget> {
	let mut release_targets = configuration
		.groups
		.iter()
		.filter_map(|group| {
			plan.groups
				.iter()
				.find(|planned_group| {
					planned_group.group_id == group.id
						&& planned_group.recommended_bump.is_release()
				})
				.and_then(|planned_group| {
					planned_group
						.planned_version
						.as_ref()
						.map(|version| ReleaseTarget {
							id: group.id.clone(),
							kind: ReleaseOwnerKind::Group,
							version: version.to_string(),
							tag: group.tag,
							release: group.release,
							version_format: group.version_format,
							tag_name: render_tag_name(
								&group.id,
								&version.to_string(),
								group.version_format,
							),
							members: group.packages.clone(),
						})
				})
		})
		.collect::<Vec<_>>();
	for decision in plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release() && decision.group_id.is_none())
	{
		let Some(package) = packages
			.iter()
			.find(|package| package.id == decision.package_id)
		else {
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
		release_targets.push(ReleaseTarget {
			id: identity.owner_id.clone(),
			kind: identity.owner_kind,
			version: version.to_string(),
			tag: identity.tag,
			release: identity.release,
			version_format: identity.version_format,
			tag_name: render_tag_name(
				&identity.owner_id,
				&version.to_string(),
				identity.version_format,
			),
			members: identity.members,
		});
	}
	release_targets.sort_by(|left, right| left.id.cmp(&right.id));
	release_targets
}

fn render_tag_name(id: &str, version: &str, version_format: VersionFormat) -> String {
	match version_format {
		VersionFormat::Namespaced => format!("{id}/v{version}"),
		VersionFormat::Primary => format!("v{version}"),
	}
}

fn build_cargo_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
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

	let mut updated_documents = BTreeMap::<PathBuf, Value>::new();
	for package in packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Cargo)
	{
		let should_update_manifest = released_versions.contains_key(&package.id)
			|| package
				.declared_dependencies
				.iter()
				.any(|dependency| released_versions_by_name.contains_key(&dependency.name));
		if !should_update_manifest {
			continue;
		}

		let mut document = read_toml_document(&package.manifest_path)?;
		update_cargo_manifest(
			&mut document,
			package,
			&released_versions,
			&released_versions_by_name,
		);
		updated_documents.insert(package.manifest_path.clone(), document);
	}

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
		let mut document = if let Some(document) = updated_documents.remove(&workspace_manifest) {
			document
		} else {
			read_toml_document(&workspace_manifest)?
		};
		update_workspace_manifest(
			&mut document,
			&shared_workspace_version,
			&released_versions_by_name,
		);
		updated_documents.insert(workspace_manifest, document);
	}

	updated_documents
		.into_iter()
		.map(|(path, document)| {
			toml::to_string_pretty(&document)
				.map(|content| FileUpdate { path, content })
				.map_err(|error| MonochangeError::Config(error.to_string()))
		})
		.collect()
}

fn read_toml_document(path: &Path) -> MonochangeResult<Value> {
	let contents = fs::read_to_string(path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;
	toml::from_str::<Value>(&contents).map_err(|error| {
		MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
	})
}

fn update_cargo_manifest(
	document: &mut Value,
	package: &PackageRecord,
	released_versions: &BTreeMap<String, String>,
	released_versions_by_name: &BTreeMap<String, String>,
) {
	if let Some(version) = released_versions.get(&package.id) {
		if let Some(package_table) = document.get_mut("package").and_then(Value::as_table_mut) {
			let uses_workspace_version = package_table
				.get("version")
				.and_then(Value::as_table)
				.and_then(|version_table| version_table.get("workspace"))
				.and_then(Value::as_bool)
				== Some(true);
			if !uses_workspace_version {
				package_table.insert("version".to_string(), Value::String(version.clone()));
			}
		}
	}

	for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
		update_dependency_table(document, section, released_versions_by_name);
	}
}

fn update_dependency_table(
	document: &mut Value,
	section: &str,
	released_versions_by_name: &BTreeMap<String, String>,
) {
	let Some(table) = document.get_mut(section).and_then(Value::as_table_mut) else {
		return;
	};
	for (dependency_name, version) in released_versions_by_name {
		let Some(entry) = table.get_mut(dependency_name) else {
			continue;
		};
		if let Some(version_value) = entry.as_str() {
			let _ = version_value;
			*entry = Value::String(version.clone());
			continue;
		}
		let Some(entry_table) = entry.as_table_mut() else {
			continue;
		};
		let uses_workspace_dependency =
			entry_table.get("workspace").and_then(Value::as_bool) == Some(true);
		if !uses_workspace_dependency {
			entry_table.insert("version".to_string(), Value::String(version.clone()));
		}
	}
}

fn update_workspace_manifest(
	document: &mut Value,
	shared_workspace_version: &str,
	released_versions_by_name: &BTreeMap<String, String>,
) {
	if let Some(workspace_table) = document.get_mut("workspace").and_then(Value::as_table_mut) {
		if let Some(workspace_package_table) = workspace_table
			.get_mut("package")
			.and_then(Value::as_table_mut)
		{
			workspace_package_table.insert(
				"version".to_string(),
				Value::String(shared_workspace_version.to_string()),
			);
		}
		if let Some(workspace_dependency_table) = workspace_table
			.get_mut("dependencies")
			.and_then(Value::as_table_mut)
		{
			for (package_name, version) in released_versions_by_name {
				let Some(entry) = workspace_dependency_table.get_mut(package_name) else {
					continue;
				};
				if let Some(entry_table) = entry.as_table_mut() {
					entry_table.insert("version".to_string(), Value::String(version.clone()));
				}
			}
		}
	}
}

fn apply_file_updates(updates: &[FileUpdate]) -> MonochangeResult<()> {
	for update in updates {
		if let Some(parent) = update.path.parent() {
			fs::create_dir_all(parent).map_err(|error| {
				MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
			})?;
		}
		fs::write(&update.path, &update.content).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to write {}: {error}",
				update.path.display()
			))
		})?;
	}
	Ok(())
}

fn shared_release_version(plan: &ReleasePlan) -> Option<String> {
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

fn shared_group_version(plan: &ReleasePlan) -> Option<String> {
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

fn root_relative(root: &Path, path: &Path) -> PathBuf {
	let relative = relative_to_root(root, path).unwrap_or_else(|| path.to_path_buf());
	if relative.as_os_str().is_empty() {
		PathBuf::from(".")
	} else {
		relative
	}
}

fn render_discovery_report(
	report: &DiscoveryReport,
	format: OutputFormat,
) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json => serde_json::to_string_pretty(&json_discovery_report(report))
			.map_err(|error| MonochangeError::Discovery(error.to_string())),
		OutputFormat::Text => Ok(text_discovery_report(report)),
	}
}

fn build_release_manifest(
	workflow: &WorkflowDefinition,
	prepared_release: &PreparedRelease,
	_command_logs: &[String],
) -> ReleaseManifest {
	ReleaseManifest {
		workflow: workflow.name.clone(),
		dry_run: prepared_release.dry_run,
		version: prepared_release.version.clone(),
		group_version: prepared_release.group_version.clone(),
		release_targets: prepared_release
			.release_targets
			.iter()
			.map(|target| ReleaseManifestTarget {
				id: target.id.clone(),
				kind: target.kind,
				version: target.version.clone(),
				tag: target.tag,
				release: target.release,
				version_format: target.version_format,
				tag_name: target.tag_name.clone(),
				members: target.members.clone(),
			})
			.collect(),
		released_packages: prepared_release.released_packages.clone(),
		changed_files: prepared_release.changed_files.clone(),
		changelogs: prepared_release
			.changelogs
			.iter()
			.map(|changelog| ReleaseManifestChangelog {
				owner_id: changelog.owner_id.clone(),
				owner_kind: changelog.owner_kind,
				path: changelog.path.clone(),
				format: changelog.format,
				notes: changelog.notes.clone(),
				rendered: changelog.rendered.clone(),
			})
			.collect(),
		deleted_changesets: prepared_release.deleted_changesets.clone(),
		deployments: Vec::new(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: prepared_release
				.plan
				.decisions
				.iter()
				.map(|decision| ReleaseManifestPlanDecision {
					package: decision.package_id.clone(),
					bump: decision.recommended_bump,
					trigger: decision.trigger_type.clone(),
					planned_version: decision.planned_version.as_ref().map(ToString::to_string),
					reasons: decision.reasons.clone(),
					upstream_sources: decision.upstream_sources.clone(),
				})
				.collect(),
			groups: prepared_release
				.plan
				.groups
				.iter()
				.map(|group| ReleaseManifestPlanGroup {
					id: group.group_id.clone(),
					planned_version: group.planned_version.as_ref().map(ToString::to_string),
					members: group.members.clone(),
					bump: group.recommended_bump,
				})
				.collect(),
			warnings: prepared_release.plan.warnings.clone(),
			unresolved_items: prepared_release.plan.unresolved_items.clone(),
			compatibility_evidence: prepared_release
				.plan
				.compatibility_evidence
				.iter()
				.map(|assessment| ReleaseManifestCompatibilityEvidence {
					package: assessment.package_id.clone(),
					provider: assessment.provider_id.clone(),
					severity: assessment.severity,
					summary: assessment.summary.clone(),
					confidence: assessment.confidence.clone(),
					evidence_location: assessment.evidence_location.clone(),
				})
				.collect(),
		},
	}
}

fn render_release_manifest_json(manifest: &ReleaseManifest) -> MonochangeResult<String> {
	serde_json::to_string_pretty(manifest)
		.map_err(|error| MonochangeError::Discovery(error.to_string()))
}

fn json_discovery_report(report: &DiscoveryReport) -> serde_json::Value {
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

fn text_discovery_report(report: &DiscoveryReport) -> String {
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

fn default_change_path(root: &Path, package_refs: &[String]) -> PathBuf {
	let timestamp = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |duration| duration.as_secs());
	let slug_source = package_refs.first().map_or("change", String::as_str);
	let slug = slug_source
		.chars()
		.map(|character| {
			if character.is_ascii_alphanumeric() {
				character.to_ascii_lowercase()
			} else {
				'-'
			}
		})
		.collect::<String>()
		.trim_matches('-')
		.to_string();
	let slug = if slug.is_empty() {
		"change".to_string()
	} else {
		slug
	};
	root.join(CHANGESET_DIR)
		.join(format!("{timestamp}-{slug}.md"))
}

fn render_changeset_markdown(
	package_refs: &[String],
	bump: BumpSeverity,
	reason: &str,
	evidence: &[String],
) -> String {
	let mut lines = vec!["---".to_string()];
	for package in package_refs {
		lines.push(format!("{package}: {bump}"));
	}
	if !evidence.is_empty() {
		lines.push("evidence:".to_string());
		for package in package_refs {
			lines.push(format!("  {package}:"));
			for item in evidence {
				lines.push(format!("    - {item}"));
			}
		}
	}
	lines.push("---".to_string());
	lines.push(String::new());
	lines.push(format!("#### {reason}"));
	lines.push(String::new());
	lines.join("\n")
}

fn format_publish_state(publish_state: monochange_core::PublishState) -> &'static str {
	match publish_state {
		monochange_core::PublishState::Public => "public",
		monochange_core::PublishState::Private => "private",
		monochange_core::PublishState::Unpublished => "unpublished",
		monochange_core::PublishState::Excluded => "excluded",
	}
}

#[cfg(test)]
mod __tests;
