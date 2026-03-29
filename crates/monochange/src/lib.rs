#![deny(clippy::all)]

//! # `monochange`
//!
//! <!-- {=monochangeCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange` is the top-level entry point for the workspace.
//!
//! Reach for this crate when you want one API and CLI surface that can discover packages across Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter workspaces, turn explicit change files into a release plan, and run configured release workflows from that plan.
//!
//! ## Why use it?
//!
//! - coordinate one release workflow across several package ecosystems
//! - expose discovery and release planning as either CLI commands or library calls
//! - connect configuration loading, package discovery, graph propagation, and semver evidence in one place
//!
//! ## Best for
//!
//! - shipping the `mc` CLI in CI or local release tooling
//! - embedding the full end-to-end planner instead of wiring the lower-level crates together yourself
//! - rendering discovery or release-plan output in text or JSON
//!
//! ## Key commands
//!
//! ```bash
//! mc workspace discover --root . --format json
//! mc changes add --root . --package crates/monochange --bump patch --reason "describe the change"
//! mc plan release --root . --changes .changeset/1234567890-crates-monochange.md --format json
//! mc release --dry-run
//! ```
//!
//! ## Responsibilities
//!
//! - aggregate all supported ecosystem adapters
//! - load `monochange.toml`
//! - resolve change input files
//! - render discovery and release-plan output in text or JSON
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
use monochange_core::materialize_dependency_edges;
use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::ReleasePlan;
use monochange_core::WorkflowDefinition;
use monochange_core::WorkflowStepDefinition;
use monochange_dart::discover_dart_packages;
use monochange_deno::discover_deno_packages;
use monochange_graph::build_release_plan;
use monochange_npm::discover_npm_packages;
use monochange_semver::collect_assessments;
use monochange_semver::CompatibilityProvider;
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
struct WorkflowInvocation {
	name: String,
	root: PathBuf,
	dry_run: bool,
	help: bool,
	extra_args: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PreparedRelease {
	pub plan: ReleasePlan,
	pub changeset_paths: Vec<PathBuf>,
	pub released_packages: Vec<String>,
	pub version: Option<String>,
	pub group_version: Option<String>,
	pub changed_files: Vec<PathBuf>,
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
struct WorkflowContext {
	root: PathBuf,
	dry_run: bool,
	prepared_release: Option<PreparedRelease>,
	command_logs: Vec<String>,
}

const CHANGESET_DIR: &str = ".changeset";
const RESERVED_COMMAND_NAMES: &[&str] = &["workspace", "plan", "changes", "help", "version"];

pub fn build_command(bin_name: &'static str) -> Command {
	Command::new(bin_name)
		.about("Manage versions and releases for your multiplatform, multilanguage monorepo")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(
			Command::new("workspace")
				.subcommand_required(true)
				.subcommand(
					Command::new("discover")
						.about("Discover packages across supported ecosystems")
						.arg(root_arg())
						.arg(format_arg()),
				),
		)
		.subcommand(
			Command::new("plan").subcommand_required(true).subcommand(
				Command::new("release")
					.about("Plan a release from explicit change input")
					.arg(root_arg())
					.arg(
						Arg::new("changes")
							.long("changes")
							.value_name("PATH")
							.required(true),
					)
					.arg(format_arg()),
			),
		)
		.subcommand(
			Command::new("changes")
				.subcommand_required(true)
				.subcommand(
					Command::new("add")
						.about("Create a change file for one or more packages")
						.arg(root_arg())
						.arg(
							Arg::new("package")
								.long("package")
								.value_name("PACKAGE")
								.action(ArgAction::Append)
								.required(true),
						)
						.arg(
							Arg::new("bump")
								.long("bump")
								.value_name("BUMP")
								.default_value("patch")
								.value_parser(clap::builder::EnumValueParser::<ChangeBump>::new()),
						)
						.arg(
							Arg::new("reason")
								.long("reason")
								.value_name("TEXT")
								.required(true),
						)
						.arg(
							Arg::new("evidence")
								.long("evidence")
								.value_name("TEXT")
								.action(ArgAction::Append),
						)
						.arg(Arg::new("output").long("output").value_name("PATH")),
				),
		)
}

fn root_arg() -> Arg {
	Arg::new("root")
		.long("root")
		.value_name("PATH")
		.default_value(".")
}

fn format_arg() -> Arg {
	Arg::new("format")
		.long("format")
		.value_name("FORMAT")
		.default_value("text")
		.value_parser(clap::builder::EnumValueParser::<OutputFormat>::new())
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
	let args = args.into_iter().collect::<Vec<_>>();
	let Some(invocation) = parse_workflow_invocation(&args)? else {
		return run_builtin_command(bin_name, args);
	};
	if RESERVED_COMMAND_NAMES.contains(&invocation.name.as_str()) {
		return run_builtin_command(bin_name, args);
	}
	if !invocation.extra_args.is_empty() {
		return Err(MonochangeError::Config(format!(
			"workflow `{}` does not accept positional arguments: {}",
			invocation.name,
			invocation.extra_args.join(" ")
		)));
	}

	let configuration = load_workspace_configuration(&invocation.root)?;
	let Some(workflow) = configuration
		.workflows
		.iter()
		.find(|workflow| workflow.name == invocation.name)
	else {
		let available_workflows = configuration
			.workflows
			.iter()
			.map(|workflow| workflow.name.as_str())
			.collect::<Vec<_>>();
		return Err(MonochangeError::Config(if available_workflows.is_empty() {
			format!("unknown command `{}`", invocation.name)
		} else {
			format!(
				"unknown command `{}`. available workflows: {}",
				invocation.name,
				available_workflows.join(", ")
			)
		}));
	};
	if invocation.help {
		return Ok(workflow_help_output(bin_name, workflow));
	}

	execute_workflow(&invocation.root, workflow, invocation.dry_run)
}

fn run_builtin_command(bin_name: &'static str, args: Vec<OsString>) -> MonochangeResult<String> {
	let matches = match build_command(bin_name).try_get_matches_from(args) {
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
	execute_matches(&matches)
}

fn parse_workflow_invocation(args: &[OsString]) -> MonochangeResult<Option<WorkflowInvocation>> {
	let mut iterator = args.iter().skip(1);
	let mut root = PathBuf::from(".");
	let mut dry_run = false;
	let mut help = false;
	let mut name = None;
	let mut extra_args = Vec::new();

	while let Some(argument) = iterator.next() {
		let value = argument.to_string_lossy();
		match value.as_ref() {
			"--help" | "-h" => help = true,
			"--version" | "-V" => return Ok(None),
			"--dry-run" => dry_run = true,
			"--root" => {
				let Some(path) = iterator.next() else {
					return Err(MonochangeError::Config(
						"workflow flag `--root` requires a path".to_string(),
					));
				};
				root = PathBuf::from(path);
			}
			flag if flag.starts_with('-') => return Ok(None),
			command => {
				if name.is_none() {
					name = Some(command.to_string());
				} else {
					extra_args.push(command.to_string());
				}
			}
		}
	}

	Ok(name.map(|name| WorkflowInvocation {
		name,
		root,
		dry_run,
		help,
		extra_args,
	}))
}

fn workflow_help_output(bin_name: &str, workflow: &WorkflowDefinition) -> String {
	let mut lines = vec![format!(
		"Usage: {bin_name} {} [--root PATH] [--dry-run]",
		workflow.name,
	)];
	lines.push(String::new());
	lines.push(format!("Workflow: {}", workflow.name));
	lines.push("Steps:".to_string());
	for step in &workflow.steps {
		match step {
			WorkflowStepDefinition::PrepareRelease => lines.push("- PrepareRelease".to_string()),
			WorkflowStepDefinition::Command { command } => {
				lines.push(format!("- Command: {command}"));
			}
		}
	}
	lines.join("\n")
}

fn execute_workflow(
	root: &Path,
	workflow: &WorkflowDefinition,
	dry_run: bool,
) -> MonochangeResult<String> {
	let mut context = WorkflowContext {
		root: root.to_path_buf(),
		dry_run,
		prepared_release: None,
		command_logs: Vec::new(),
	};

	for step in &workflow.steps {
		match step {
			WorkflowStepDefinition::PrepareRelease => {
				context.prepared_release = Some(prepare_release(root, dry_run)?);
			}
			WorkflowStepDefinition::Command { command } => {
				run_workflow_command(&mut context, command)?;
			}
		}
	}

	Ok(render_workflow_result(workflow, &context))
}

fn run_workflow_command(context: &mut WorkflowContext, command: &str) -> MonochangeResult<()> {
	let interpolated = interpolate_workflow_command(context, command);
	if context.dry_run {
		context
			.command_logs
			.push(format!("skipped command `{interpolated}` (dry-run)"));
		return Ok(());
	}

	let output = ProcessCommand::new("sh")
		.arg("-c")
		.arg(&interpolated)
		.current_dir(&context.root)
		.output()
		.map_err(|error| {
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

fn interpolate_workflow_command(context: &WorkflowContext, command: &str) -> String {
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
	let released_packages = context
		.prepared_release
		.as_ref()
		.map(|prepared| prepared.released_packages.join(","))
		.unwrap_or_default();
	let changed_files = context
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
		.unwrap_or_default();
	let changesets = context
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
		.unwrap_or_default();

	command
		.replace("$group_version", group_version)
		.replace("$released_packages", &released_packages)
		.replace("$changed_files", &changed_files)
		.replace("$changesets", &changesets)
		.replace("$version", version)
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
	lines.join("\n")
}

pub fn execute_matches(matches: &ArgMatches) -> MonochangeResult<String> {
	match matches.subcommand() {
		Some(("workspace", workspace_matches)) => match workspace_matches.subcommand() {
			Some(("discover", discover_matches)) => {
				let root = required_path(discover_matches, "root")?;
				let format = required_format(discover_matches, "format")?;
				render_discovery_report(&discover_workspace(&root)?, format)
			}
			_ => Err(MonochangeError::Config(
				"unknown workspace command".to_string(),
			)),
		},
		Some(("plan", plan_matches)) => match plan_matches.subcommand() {
			Some(("release", release_matches)) => {
				let root = required_path(release_matches, "root")?;
				let changes = required_path(release_matches, "changes")?;
				let format = required_format(release_matches, "format")?;
				render_release_plan(&plan_release(&root, &changes)?, format)
			}
			_ => Err(MonochangeError::Config("unknown plan command".to_string())),
		},
		Some(("changes", changes_matches)) => match changes_matches.subcommand() {
			Some(("add", add_matches)) => {
				let root = required_path(add_matches, "root")?;
				let package_refs = required_strings(add_matches, "package")?;
				let bump = required_bump(add_matches, "bump")?;
				let reason = required_string(add_matches, "reason")?;
				let evidence = optional_strings(add_matches, "evidence");
				let output = optional_path(add_matches, "output");
				let path = add_change_file(
					&root,
					&package_refs,
					bump.into(),
					&reason,
					&evidence,
					output.as_deref(),
				)?;
				Ok(format!("wrote change file {}", path.display()))
			}
			_ => Err(MonochangeError::Config(
				"unknown changes command".to_string(),
			)),
		},
		_ => Err(MonochangeError::Config("unknown command".to_string())),
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
	let discovery = discover_workspace(root)?;
	let packages = canonical_change_packages(root, package_refs, &discovery.packages)?;
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
	let change_signals = load_change_signals(changes_path, root, &discovery.packages)?;
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
			signals.extend(load_change_signals(path, root, &discovery.packages)?);
			Ok::<_, MonochangeError>(signals)
		})?;
	let plan = build_release_plan_from_signals(&configuration, &discovery, &change_signals);
	let released_packages = released_package_names(&discovery.packages, &plan);
	if released_packages.is_empty() {
		return Err(MonochangeError::Config(
			"no releaseable packages were found in discovered changesets".to_string(),
		));
	}

	let changelog_overrides = resolve_changelog_overrides(&configuration, &discovery.packages)?;
	let manifest_updates = build_cargo_manifest_updates(&discovery.packages, &plan)?;
	let changelog_updates = build_changelog_updates(
		root,
		&discovery.packages,
		&plan,
		&change_signals,
		&changelog_overrides,
	)?;
	let mut changed_files = manifest_updates
		.iter()
		.map(|update| root_relative(root, &update.path))
		.collect::<Vec<_>>();
	changed_files.extend(
		changelog_updates
			.iter()
			.map(|update| root_relative(root, &update.path)),
	);
	changed_files.sort();
	changed_files.dedup();
	let updated_changelogs = changelog_updates
		.iter()
		.map(|update| root_relative(root, &update.path))
		.collect::<Vec<_>>();

	let version = shared_release_version(&plan);
	let group_version = shared_group_version(&plan);
	let mut deleted_changesets = Vec::new();
	if !dry_run {
		apply_file_updates(&manifest_updates)?;
		apply_file_updates(&changelog_updates)?;
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
		changed_files,
		updated_changelogs,
		deleted_changesets,
		dry_run,
	})
}

fn discover_changeset_paths(root: &Path) -> MonochangeResult<Vec<PathBuf>> {
	let changeset_dir = root.join(CHANGESET_DIR);
	if !changeset_dir.exists() {
		return Err(MonochangeError::Config(format!(
			"no markdown changesets found under {}",
			changeset_dir.display()
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
			"no markdown changesets found under {}",
			changeset_dir.display()
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
	packages: &[PackageRecord],
) -> MonochangeResult<Vec<String>> {
	let mut canonical_packages = Vec::new();
	for package_ref in package_refs {
		let package_id = resolve_package_reference(package_ref, root, packages)?;
		let package = packages
			.iter()
			.find(|package| package.id == package_id)
			.ok_or_else(|| {
				MonochangeError::Config(format!("failed to resolve package `{package_ref}`"))
			})?;
		if !canonical_packages.contains(&package.name) {
			canonical_packages.push(package.name.clone());
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

fn resolve_changelog_overrides(
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> MonochangeResult<BTreeMap<String, PathBuf>> {
	let mut changelog_overrides = BTreeMap::new();
	for package_override in &configuration.package_overrides {
		let Some(changelog_path) = &package_override.changelog else {
			continue;
		};
		let package_id = resolve_package_reference(
			&package_override.package,
			&configuration.root_path,
			packages,
		)?;
		let resolved_path = if changelog_path.is_absolute() {
			changelog_path.clone()
		} else {
			configuration.root_path.join(changelog_path)
		};
		changelog_overrides.insert(package_id, resolved_path);
	}
	Ok(changelog_overrides)
}

fn build_changelog_updates(
	_root: &Path,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
	change_signals: &[ChangeSignal],
	changelog_overrides: &BTreeMap<String, PathBuf>,
) -> MonochangeResult<Vec<FileUpdate>> {
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
	for decision in plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
	{
		let Some(changelog_path) = changelog_overrides.get(&decision.package_id) else {
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
		let current = if changelog_path.exists() {
			fs::read_to_string(changelog_path).map_err(|error| {
				MonochangeError::Io(format!(
					"failed to read {}: {error}",
					changelog_path.display()
				))
			})?
		} else {
			String::new()
		};
		let mut content = current.trim_end().to_string();
		if !content.is_empty() {
			content.push_str("\n\n");
		}
		content.push_str(&render_changelog_section(
			&package.name,
			&planned_version.to_string(),
			&notes,
		));
		content.push('\n');
		updates.push(FileUpdate {
			path: changelog_path.clone(),
			content,
		});
	}
	Ok(updates)
}

fn render_changelog_section(package_name: &str, version: &str, notes: &[String]) -> String {
	let mut lines = vec![format!("## {version}"), String::new()];
	if notes.is_empty() {
		lines.push(format!("- prepare release for `{package_name}`"));
	} else {
		for note in notes {
			lines.push(format!("- {note}"));
		}
	}
	lines.join("\n")
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
	path.strip_prefix(root)
		.map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
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

fn render_release_plan(plan: &ReleasePlan, format: OutputFormat) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json => serde_json::to_string_pretty(&json_release_plan(plan))
			.map_err(|error| MonochangeError::Discovery(error.to_string())),
		OutputFormat::Text => Ok(text_release_plan(plan)),
	}
}

fn json_discovery_report(report: &DiscoveryReport) -> serde_json::Value {
	json!({
		"workspaceRoot": report.workspace_root,
		"packages": report.packages.iter().map(|package| {
			json!({
				"id": package.id,
				"name": package.name,
				"ecosystem": package.ecosystem.as_str(),
				"manifestPath": package.manifest_path,
				"workspaceRoot": package.workspace_root,
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

fn json_release_plan(plan: &ReleasePlan) -> serde_json::Value {
	json!({
		"workspaceRoot": plan.workspace_root,
		"decisions": plan.decisions.iter().map(|decision| {
			json!({
				"package": decision.package_id,
				"bump": decision.recommended_bump.to_string(),
				"trigger": decision.trigger_type,
				"plannedVersion": decision.planned_version.as_ref().map(ToString::to_string),
				"reasons": decision.reasons,
				"upstreamSources": decision.upstream_sources,
			})
		}).collect::<Vec<_>>(),
		"groups": plan.groups.iter().map(|group| {
			json!({
				"id": group.group_id,
				"plannedVersion": group.planned_version.as_ref().map(ToString::to_string),
				"members": group.members,
				"bump": group.recommended_bump.to_string(),
			})
		}).collect::<Vec<_>>(),
		"warnings": plan.warnings,
		"unresolvedItems": plan.unresolved_items,
		"compatibilityEvidence": plan.compatibility_evidence.iter().map(|assessment| {
			json!({
				"package": assessment.package_id,
				"provider": assessment.provider_id,
				"severity": assessment.severity.to_string(),
				"summary": assessment.summary,
				"confidence": assessment.confidence,
				"evidenceLocation": assessment.evidence_location,
			})
		}).collect::<Vec<_>>(),
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

fn text_release_plan(plan: &ReleasePlan) -> String {
	let mut lines = vec![format!(
		"Release plan for {}",
		plan.workspace_root.display()
	)];
	for decision in plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
	{
		let planned_version = decision
			.planned_version
			.as_ref()
			.map_or_else(|| "unversioned".to_string(), ToString::to_string);
		lines.push(format!(
			"- {}: {} ({}) -> {}",
			decision.package_id, decision.recommended_bump, decision.trigger_type, planned_version,
		));
		for reason in &decision.reasons {
			lines.push(format!("  - {reason}"));
		}
	}
	if !plan.groups.is_empty() {
		lines.push("Version groups:".to_string());
		for group in &plan.groups {
			lines.push(format!(
				"- {}: {} -> {}",
				group.group_id,
				group.recommended_bump,
				group
					.planned_version
					.as_ref()
					.map_or_else(|| "unversioned".to_string(), ToString::to_string),
			));
		}
	}
	if !plan.compatibility_evidence.is_empty() {
		lines.push("Compatibility evidence:".to_string());
		for assessment in &plan.compatibility_evidence {
			lines.push(format!(
				"- {}: {} ({})",
				assessment.package_id, assessment.severity, assessment.summary
			));
		}
	}
	if !plan.warnings.is_empty() {
		lines.push("Warnings:".to_string());
		for warning in &plan.warnings {
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

fn required_string(matches: &ArgMatches, key: &str) -> MonochangeResult<String> {
	matches
		.get_one::<String>(key)
		.cloned()
		.ok_or_else(|| MonochangeError::Config(format!("missing `{key}`")))
}

fn required_strings(matches: &ArgMatches, key: &str) -> MonochangeResult<Vec<String>> {
	let values = optional_strings(matches, key);
	if values.is_empty() {
		Err(MonochangeError::Config(format!("missing `{key}`")))
	} else {
		Ok(values)
	}
}

fn optional_strings(matches: &ArgMatches, key: &str) -> Vec<String> {
	matches
		.get_many::<String>(key)
		.map(|values| values.cloned().collect())
		.unwrap_or_default()
}

fn optional_path(matches: &ArgMatches, key: &str) -> Option<PathBuf> {
	matches.get_one::<String>(key).map(PathBuf::from)
}

fn required_bump(matches: &ArgMatches, key: &str) -> MonochangeResult<ChangeBump> {
	matches
		.get_one::<ChangeBump>(key)
		.copied()
		.ok_or_else(|| MonochangeError::Config(format!("missing `{key}`")))
}

fn required_path(matches: &ArgMatches, key: &str) -> MonochangeResult<PathBuf> {
	matches
		.get_one::<String>(key)
		.map(PathBuf::from)
		.ok_or_else(|| MonochangeError::Config(format!("missing `{key}`")))
}

fn required_format(matches: &ArgMatches, key: &str) -> MonochangeResult<OutputFormat> {
	matches
		.get_one::<OutputFormat>(key)
		.copied()
		.ok_or_else(|| MonochangeError::Config(format!("missing `{key}`")))
}

#[cfg(test)]
mod __tests;
