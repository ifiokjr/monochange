#![deny(clippy::all)]

//! # `monochange`
//!
//! <!-- {=monochangeCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange` is the top-level entry point for the workspace.
//!
//! Reach for this crate when you want one API and CLI surface that discovers packages across Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter workspaces, exposes top-level commands from `monochange.toml`, and runs configured CLI commands from those definitions.
//!
//! ## Why use it?
//!
//! - coordinate one config-defined CLI across several package ecosystems
//! - expose discovery, change creation, and release preparation as both commands and library calls
//! - connect configuration loading, package discovery, graph propagation, and semver evidence in one place
//!
//! ## Best for
//!
//! - shipping the `mc` CLI in CI or local release tooling
//! - embedding the full end-to-end planner instead of wiring the lower-level crates together yourself
//! - generating starter config with `mc init` and then evolving the CLI command surface over time
//!
//! ## Key commands
//!
//! ```bash
//! mc init
//! mc assist pi
//! mc discover --format json
//! mc change --package monochange --bump patch --reason "describe the change"
//! mc release --dry-run --format json
//! mc mcp
//! ```
//!
//! ## Responsibilities
//!
//! - aggregate all supported ecosystem adapters
//! - load `monochange.toml`
//! - synthesize default CLI commands when config does not declare any
//! - resolve change input files
//! - render discovery and release command output in text or JSON
//! - execute configured CLI commands plus built-in assistant setup and MCP commands
//! - preview or publish provider releases from prepared release data
//! - evaluate pull-request changeset policy from CI-supplied changed paths and labels
//! - expose JSON-first MCP tools for assistant workflows
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
use clap::parser::ValueSource;
use clap::ArgMatches;
use clap::ValueEnum;
use minijinja::Environment;
use minijinja::UndefinedBehavior;
use monochange_cargo::RustSemverProvider;
use monochange_config::load_changeset_file;
use monochange_config::load_workspace_configuration;
use monochange_config::resolve_package_reference;
use monochange_config::validate_workspace;
use monochange_core::materialize_dependency_edges;
use monochange_core::relative_to_root;
use monochange_core::render_release_notes;
use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogTarget;
use monochange_core::ChangesetContext;
use monochange_core::ChangesetPolicyEvaluation;
use monochange_core::ChangesetPolicyStatus;
use monochange_core::ChangesetRevision;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputKind;
use monochange_core::CliStepDefinition;
use monochange_core::CliStepInputValue;
use monochange_core::CommandVariable;
use monochange_core::ShellConfig;
use monochange_core::DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED;
use monochange_core::DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY;
use monochange_core::DEFAULT_RELEASE_TITLE_NAMESPACED;
use monochange_core::DEFAULT_RELEASE_TITLE_PRIMARY;

use monochange_core::CommitMessage;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::ExtraChangelogSection;
use monochange_core::GroupChangelogInclude;
use monochange_core::HostedActorRef;
use monochange_core::HostedActorSourceKind;
use monochange_core::HostedCommitRef;
use monochange_core::HostedIssueRef;
use monochange_core::HostedIssueRelationshipKind;
use monochange_core::HostedReviewRequestRef;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::PreparedChangeset;
use monochange_core::PreparedChangesetTarget;

use monochange_core::render_release_record_block;
use monochange_core::ChangesetTargetKind;
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
use monochange_core::ReleaseRecord;
use monochange_core::ReleaseRecordDiscovery;
use monochange_core::ReleaseRecordProvider;
use monochange_core::ReleaseRecordTarget;
use monochange_core::RetargetOperation;
use monochange_core::RetargetProviderResult;
use monochange_core::RetargetResult;
use monochange_core::RetargetTagResult;
use monochange_core::SourceChangeRequest;
use monochange_core::SourceChangeRequestOperation;
use monochange_core::SourceChangeRequestOutcome;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::SourceReleaseOperation;
use monochange_core::SourceReleaseOutcome;
use monochange_core::SourceReleaseRequest;
use monochange_core::VersionFormat;
use monochange_core::VersionedFileDefinition;
use monochange_gitea as gitea_provider;
use monochange_github as github_provider;
use monochange_gitlab as gitlab_provider;
use monochange_graph::build_release_plan;
use monochange_semver::collect_assessments;
use monochange_semver::CompatibilityProvider;
use serde::Serialize;
use serde_json::json;
use toml::Value;

use assist::run_assist;
use cli::build_command_with_cli;
use cli::cli_commands_for_root;
use cli::current_dir_or_dot;
use git_support::git_commit_paths;
use git_support::git_head_commit;
use git_support::git_stage_paths;
use release_record::render_release_record_discovery;
use workspace_ops::init_workspace;

pub use changeset_policy::affected_packages;
pub use changeset_policy::evaluate_changeset_policy;
pub use changeset_policy::verify_changesets;
pub use cli::build_command;
pub use release_record::discover_release_record;
pub use release_record::execute_release_retarget;
pub use release_record::plan_release_retarget;
pub use release_record::retarget_release;
pub use workspace_ops::add_change_file;
pub use workspace_ops::discover_workspace;
pub use workspace_ops::plan_release;
pub use workspace_ops::prepare_release;

pub(crate) use changeset_policy::compute_changed_paths_since;
pub(crate) use changeset_policy::is_changeset_markdown_path;
pub(crate) use changeset_policy::normalize_changed_path;
pub(crate) use workspace_ops::add_interactive_change_file;
pub(crate) use workspace_ops::render_change_target_markdown;
pub(crate) use workspace_ops::validate_cargo_workspace_version_groups;

#[cfg(test)]
pub(crate) use cli::build_command_for_root;
#[cfg(test)]
pub(crate) use workspace_ops::change_type_default_bump;
#[cfg(test)]
pub(crate) use workspace_ops::render_interactive_changeset_markdown;

mod assist;
mod changeset_policy;
mod cli;
mod git_support;
mod interactive;
mod mcp;
mod release_record;
mod workspace_ops;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
	Text,
	Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ChangeBump {
	None,
	Patch,
	Minor,
	Major,
}

impl From<ChangeBump> for BumpSeverity {
	fn from(value: ChangeBump) -> Self {
		match value {
			ChangeBump::None => Self::None,
			ChangeBump::Patch => Self::Patch,
			ChangeBump::Minor => Self::Minor,
			ChangeBump::Major => Self::Major,
		}
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Assistant {
	Generic,
	Claude,
	Cursor,
	Copilot,
	Pi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum AssistOutputFormat {
	Text,
	Json,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ReleaseTarget {
	pub id: String,
	pub kind: ReleaseOwnerKind,
	pub version: String,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
	pub tag_name: String,
	pub members: Vec<String>,
	pub rendered_title: String,
	pub rendered_changelog_title: String,
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
	pub changesets: Vec<PreparedChangeset>,
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
	content: Vec<u8>,
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
struct ReleaseNoteChange {
	package_id: String,
	package_name: String,
	package_labels: Vec<String>,
	source_path: Option<String>,
	summary: String,
	details: Option<String>,
	bump: BumpSeverity,
	change_type: Option<String>,
	context: Option<String>,
	changeset_path: Option<String>,
	change_owner: Option<String>,
	change_owner_link: Option<String>,
	review_request: Option<String>,
	review_request_link: Option<String>,
	introduced_commit: Option<String>,
	introduced_commit_link: Option<String>,
	last_updated_commit: Option<String>,
	last_updated_commit_link: Option<String>,
	related_issues: Option<String>,
	related_issue_links: Option<String>,
	closed_issues: Option<String>,
	closed_issue_links: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
struct RenderedChangesetContext {
	context: String,
	changeset_path: String,
	change_owner: Option<String>,
	change_owner_link: Option<String>,
	review_request: Option<String>,
	review_request_link: Option<String>,
	introduced_commit: Option<String>,
	introduced_commit_link: Option<String>,
	last_updated_commit: Option<String>,
	last_updated_commit_link: Option<String>,
	related_issues: Option<String>,
	related_issue_links: Option<String>,
	closed_issues: Option<String>,
	closed_issue_links: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct GroupReleaseNoteKey {
	source_path: Option<String>,
	summary: String,
	details: Option<String>,
	bump: BumpSeverity,
	change_type: Option<String>,
	context: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ResolvedSectionDefinition {
	title: String,
	types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum BuiltinReleaseSection {
	Major,
	Minor,
	Patch,
	Note,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangesetDiagnosticsReport {
	requested_changesets: Vec<PathBuf>,
	changesets: Vec<PreparedChangeset>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RetargetReleaseReport {
	from: String,
	target: String,
	resolved_from_commit: String,
	record_commit: String,
	target_commit: String,
	distance: usize,
	is_descendant: bool,
	force: bool,
	dry_run: bool,
	sync_provider: bool,
	tags: Vec<String>,
	git_tag_results: Vec<RetargetTagResult>,
	provider_results: Vec<RetargetProviderResult>,
	status: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommitReleaseReport {
	subject: String,
	body: String,
	commit: Option<String>,
	tracked_paths: Vec<PathBuf>,
	dry_run: bool,
	status: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CliContext {
	root: PathBuf,
	dry_run: bool,
	inputs: BTreeMap<String, Vec<String>>,
	last_step_inputs: BTreeMap<String, Vec<String>>,
	prepared_release: Option<PreparedRelease>,
	release_manifest_path: Option<PathBuf>,
	release_requests: Vec<SourceReleaseRequest>,
	release_results: Vec<String>,
	release_request: Option<SourceChangeRequest>,
	release_request_result: Option<String>,
	release_commit_report: Option<CommitReleaseReport>,
	issue_comment_plans: Vec<github_provider::GitHubIssueCommentPlan>,
	issue_comment_results: Vec<String>,
	changeset_policy_evaluation: Option<ChangesetPolicyEvaluation>,
	changeset_diagnostics: Option<ChangesetDiagnosticsReport>,
	retarget_report: Option<RetargetReleaseReport>,
	step_outputs: BTreeMap<String, CommandStepOutput>,
	command_logs: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CommandStepOutput {
	stdout: String,
	stderr: String,
}

const CHANGESET_DIR: &str = ".changeset";

pub fn run_from_env(bin_name: &'static str) -> MonochangeResult<()> {
	let args = std::env::args_os();
	let output = run_with_args(bin_name, args)?;
	if !output.is_empty() {
		println!("{output}");
	}
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
	let cli = cli_commands_for_root(root);
	let matches = match build_command_with_cli(bin_name, &cli).try_get_matches_from(args) {
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
		Some(("assist", assist_matches)) => {
			let assistant = match assist_matches
				.get_one::<String>("assistant")
				.map(String::as_str)
			{
				Some("generic") => Assistant::Generic,
				Some("claude") => Assistant::Claude,
				Some("cursor") => Assistant::Cursor,
				Some("copilot") => Assistant::Copilot,
				Some("pi") => Assistant::Pi,
				Some(value) => {
					return Err(MonochangeError::Config(format!(
						"unknown assistant `{value}`"
					)))
				}
				None => return Err(MonochangeError::Config("missing assistant".to_string())),
			};
			let format = match assist_matches
				.get_one::<String>("format")
				.map_or("text", String::as_str)
			{
				"text" => AssistOutputFormat::Text,
				"json" => AssistOutputFormat::Json,
				value => {
					return Err(MonochangeError::Config(format!(
						"unknown assist output format `{value}`"
					)))
				}
			};
			run_assist(assistant, format)
		}
		Some(("mcp", _)) => {
			let runtime = tokio::runtime::Runtime::new()
				.map_err(|error| MonochangeError::Config(error.to_string()))?;
			runtime.block_on(mcp::run_server());
			Ok(String::new())
		}
		Some(("release-record", release_record_matches)) => {
			let from = release_record_matches
				.get_one::<String>("from")
				.map(String::as_str)
				.ok_or_else(|| MonochangeError::Config("missing release-record ref".to_string()))?;
			let format = if release_record_matches
				.get_one::<String>("format")
				.is_some_and(|value| value == "json")
			{
				OutputFormat::Json
			} else {
				OutputFormat::Text
			};
			render_release_record_discovery(root, from, format)
		}
		Some((cli_command_name, cli_command_matches)) => {
			let configuration = configuration?;
			execute_matches(root, &configuration, cli_command_name, cli_command_matches)
		}
		None => Err(MonochangeError::Config("unknown command".to_string())),
	}
}

fn execute_matches(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command_name: &str,
	cli_command_matches: &ArgMatches,
) -> MonochangeResult<String> {
	let Some(cli_command) = configuration
		.cli
		.iter()
		.find(|cli_command| cli_command.name == cli_command_name)
	else {
		return Err(MonochangeError::Config(format!(
			"unknown command `{cli_command_name}`"
		)));
	};

	let dry_run = cli_command_matches.get_flag("dry-run");
	let inputs = collect_cli_command_inputs(cli_command, cli_command_matches);
	execute_cli_command(root, configuration, cli_command, dry_run, inputs)
}

fn collect_cli_command_inputs(
	cli_command: &CliCommandDefinition,
	matches: &ArgMatches,
) -> BTreeMap<String, Vec<String>> {
	let mut inputs = BTreeMap::new();
	for input in &cli_command.inputs {
		let value_source = matches.value_source(input.name.as_str());
		let values = match input.kind {
			CliInputKind::StringList => matches
				.get_many::<String>(input.name.as_str())
				.map(|values| values.cloned().collect::<Vec<_>>())
				.unwrap_or_default(),
			CliInputKind::Boolean => {
				if input.default.as_deref() == Some("true") {
					matches
						.get_one::<String>(input.name.as_str())
						.map(|value| vec![value.clone()])
						.unwrap_or_default()
				} else if matches.get_flag(input.name.as_str()) {
					vec!["true".to_string()]
				} else {
					Vec::new()
				}
			}
			CliInputKind::String | CliInputKind::Path | CliInputKind::Choice => {
				if cli_command.name == "change"
					&& input.name == "bump"
					&& value_source == Some(ValueSource::DefaultValue)
				{
					Vec::new()
				} else {
					matches
						.get_one::<String>(input.name.as_str())
						.map(|value| vec![value.clone()])
						.unwrap_or_default()
				}
			}
		};
		inputs.insert(input.name.clone(), values);
	}
	inputs
}

fn resolve_step_inputs(
	context: &CliContext,
	step: &CliStepDefinition,
) -> MonochangeResult<BTreeMap<String, Vec<String>>> {
	let mut resolved = context.inputs.clone();
	if step.inputs().is_empty() {
		return Ok(resolved);
	}

	let template_context = build_cli_template_context(context, &context.inputs, None);
	for (input_name, input_value) in step.inputs() {
		resolved.insert(
			input_name.clone(),
			resolve_step_input_override(input_value, &template_context)?,
		);
	}

	Ok(resolved)
}

fn resolve_step_input_override(
	input_value: &CliStepInputValue,
	template_context: &serde_json::Map<String, serde_json::Value>,
) -> MonochangeResult<Vec<String>> {
	match input_value {
		CliStepInputValue::Boolean(value) => Ok(vec![value.to_string()]),
		CliStepInputValue::List(values) => {
			let mut resolved = Vec::new();
			for value in values {
				resolved.extend(resolve_step_input_template(value, template_context)?);
			}
			Ok(resolved)
		}
		CliStepInputValue::String(value) => resolve_step_input_template(value, template_context),
	}
}

fn resolve_step_input_template(
	template: &str,
	template_context: &serde_json::Map<String, serde_json::Value>,
) -> MonochangeResult<Vec<String>> {
	if let Some(path) = parse_direct_template_reference(template) {
		return Ok(lookup_template_value(
			&serde_json::Value::Object(template_context.clone()),
			path,
		)
		.map_or_else(Vec::new, template_value_to_input_values));
	}

	let jinja_context =
		minijinja::Value::from_serialize(serde_json::Value::Object(template_context.clone()));
	Ok(vec![render_jinja_template(template, &jinja_context)?])
}

fn parse_direct_template_reference(template: &str) -> Option<&str> {
	let trimmed = template.trim();
	let inner = trimmed.strip_prefix("{{")?.strip_suffix("}}")?.trim();
	if inner.is_empty()
		|| !inner
			.chars()
			.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
	{
		return None;
	}
	Some(inner)
}

fn lookup_template_value<'a>(
	value: &'a serde_json::Value,
	path: &str,
) -> Option<&'a serde_json::Value> {
	let mut current = value;
	for segment in path.split('.') {
		current = match current {
			serde_json::Value::Object(map) => map.get(segment)?,
			serde_json::Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
			_ => return None,
		};
	}
	Some(current)
}

fn template_value_to_input_values(value: &serde_json::Value) -> Vec<String> {
	match value {
		serde_json::Value::Null => Vec::new(),
		serde_json::Value::Bool(value) => vec![value.to_string()],
		serde_json::Value::Number(value) => vec![value.to_string()],
		serde_json::Value::String(value) => vec![value.clone()],
		serde_json::Value::Array(values) => values
			.iter()
			.flat_map(template_value_to_input_values)
			.collect(),
		serde_json::Value::Object(_) => vec![value.to_string()],
	}
}

fn execute_cli_command(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command: &CliCommandDefinition,
	dry_run: bool,
	inputs: BTreeMap<String, Vec<String>>,
) -> MonochangeResult<String> {
	let mut context = CliContext {
		root: root.to_path_buf(),
		dry_run,
		last_step_inputs: inputs.clone(),
		inputs,
		prepared_release: None,
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let mut output = None;

	for step in &cli_command.steps {
		let step_inputs = resolve_step_inputs(&context, step)?;
		context.last_step_inputs = step_inputs.clone();
		match step {
			CliStepDefinition::Validate { .. } => {
				validate_workspace(root)?;
				validate_cargo_workspace_version_groups(root)?;
				output = Some(format!(
					"workspace validation passed for {}",
					root_relative(root, root).display()
				));
			}
			CliStepDefinition::Discover { .. } => {
				let format = step_inputs
					.get("format")
					.and_then(|values| values.first())
					.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;
				output = Some(render_discovery_report(&discover_workspace(root)?, format)?);
			}
			CliStepDefinition::CreateChangeFile { .. } => {
				let is_interactive = step_inputs
					.get("interactive")
					.and_then(|values| values.first())
					.is_some_and(|value| value == "true");

				if is_interactive {
					let configuration = load_workspace_configuration(root)?;
					let options = interactive::InteractiveOptions {
						reason: step_inputs
							.get("reason")
							.and_then(|values| values.first())
							.cloned(),
						details: step_inputs
							.get("details")
							.and_then(|values| values.first())
							.cloned(),
					};
					let result = interactive::run_interactive_change(&configuration, &options)?;
					let output_path = step_inputs
						.get("output")
						.and_then(|values| values.first())
						.map(PathBuf::from);
					let path = add_interactive_change_file(root, &result, output_path.as_deref())?;
					output = Some(format!(
						"wrote change file {}",
						root_relative(root, &path).display()
					));
				} else {
					let package_refs = step_inputs.get("package").cloned().unwrap_or_default();
					if package_refs.is_empty() {
						return Err(MonochangeError::Config(
							"command `change` requires at least one `--package` value or `--interactive` mode".to_string(),
						));
					}
					let bump = if let Some(value) =
						step_inputs.get("bump").and_then(|values| values.first())
					{
						parse_change_bump(value)?
					} else if step_inputs
						.get("type")
						.and_then(|values| values.first())
						.is_some()
					{
						ChangeBump::None
					} else {
						ChangeBump::Patch
					};
					let version = step_inputs
						.get("version")
						.and_then(|values| values.first())
						.cloned();
					let reason = step_inputs
						.get("reason")
						.and_then(|values| values.first())
						.cloned()
						.ok_or_else(|| {
							MonochangeError::Config(
								"command `change` requires a `--reason` value".to_string(),
							)
						})?;
					let change_type = step_inputs
						.get("type")
						.and_then(|values| values.first())
						.cloned();
					let details = step_inputs
						.get("details")
						.and_then(|values| values.first())
						.cloned();
					let output_path = step_inputs
						.get("output")
						.and_then(|values| values.first())
						.map(PathBuf::from);
					let path = add_change_file(
						root,
						&package_refs,
						bump.into(),
						version.as_deref(),
						&reason,
						change_type.as_deref(),
						details.as_deref(),
						output_path.as_deref(),
					)?;
					output = Some(format!(
						"wrote change file {}",
						root_relative(root, &path).display()
					));
				}
			}
			CliStepDefinition::PrepareRelease { .. } => {
				context.prepared_release = Some(prepare_release(root, dry_run)?);
				output = None;
			}
			CliStepDefinition::RenderReleaseManifest { path, .. } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`RenderReleaseManifest` requires a previous `PrepareRelease` step"
							.to_string(),
					)
				})?;
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				if let Some(path) = path {
					let resolved_path = resolve_config_path(root, path);
					let rendered = render_release_manifest_json(&manifest)?;
					apply_file_updates(&[FileUpdate {
						path: resolved_path.clone(),
						content: rendered.into_bytes(),
					}])?;
					context.release_manifest_path = Some(root_relative(root, &resolved_path));
				}
				output = None;
			}
			CliStepDefinition::PublishRelease { .. } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`PublishRelease` requires a previous `PrepareRelease` step".to_string(),
					)
				})?;
				let source = load_workspace_configuration(root)?.source.ok_or_else(|| {
					MonochangeError::Config(
						"`PublishRelease` requires `[source]` configuration".to_string(),
					)
				})?;
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				context.release_requests = build_source_release_requests(&source, &manifest);
				if context.dry_run {
					context.release_results = context
						.release_requests
						.iter()
						.map(|request| {
							format!(
								"dry-run {} {} ({}) via {}",
								request.repository,
								request.tag_name,
								request.name,
								request.provider
							)
						})
						.collect();
				} else {
					context.release_results =
						publish_source_release_requests(&source, &context.release_requests)?
							.into_iter()
							.map(|result| {
								format!(
									"{} {} ({}) via {}",
									result.repository,
									result.tag_name,
									format_source_operation(&result.operation),
									result.provider
								)
							})
							.collect();
				}
				output = None;
			}
			CliStepDefinition::CommitRelease { .. } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`CommitRelease` requires a previous `PrepareRelease` step".to_string(),
					)
				})?;
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				#[rustfmt::skip]
				let release_commit_report = commit_release(root, &context, configuration.source.as_ref(), &manifest)?;
				context.release_commit_report = Some(release_commit_report);
				output = None;
			}
			CliStepDefinition::OpenReleaseRequest { .. } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`OpenReleaseRequest` requires a previous `PrepareRelease` step"
							.to_string(),
					)
				})?;
				let source = load_workspace_configuration(root)?.source.ok_or_else(|| {
					MonochangeError::Config(
						"`OpenReleaseRequest` requires `[source]` configuration".to_string(),
					)
				})?;
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				let request = build_source_change_request(&source, &manifest);
				if context.dry_run {
					context.release_request_result = Some(format!(
						"dry-run {} {} -> {} via {}",
						request.repository,
						request.head_branch,
						request.base_branch,
						request.provider
					));
				} else {
					let tracked_paths = tracked_release_pull_request_paths(&context, &manifest);
					let result =
						publish_source_change_request(&source, root, &request, &tracked_paths)?;
					context.release_request_result = Some(format!(
						"{} #{} ({}) via {}",
						result.repository,
						result.number,
						format_change_request_operation(&result.operation),
						result.provider
					));
				}
				context.release_request = Some(request);
				output = None;
			}
			CliStepDefinition::CommentReleasedIssues { .. } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`CommentReleasedIssues` requires a previous `PrepareRelease` step"
							.to_string(),
					)
				})?;
				let source = load_workspace_configuration(root)?
					.source
					.filter(|source| source.provider == SourceProvider::GitHub)
					.ok_or_else(|| {
						MonochangeError::Config(
							"`CommentReleasedIssues` requires `[source].provider = \"github\"` configuration"
								.to_string(),
						)
					})?;
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				context.issue_comment_plans =
					github_provider::plan_released_issue_comments(&source, &manifest);
				if context.dry_run {
					context.issue_comment_results = context
						.issue_comment_plans
						.iter()
						.map(|plan| format!("dry-run {} {}", plan.repository, plan.issue_id))
						.collect();
				} else {
					context.issue_comment_results =
						github_provider::comment_released_issues(&source, &manifest)?
							.into_iter()
							.map(|result| {
								format!(
									"{} {} ({})",
									result.repository,
									result.issue_id,
									match result.operation {
										monochange_github::GitHubIssueCommentOperation::Created => "created",
										monochange_github::GitHubIssueCommentOperation::SkippedExisting => {
											"skipped_existing"
										}
									}
								)
							})
							.collect();
				}
				output = None;
			}
			CliStepDefinition::AffectedPackages { .. } => {
				let since = step_inputs
					.get("since")
					.and_then(|values| values.first().cloned());
				let explicit_paths = step_inputs
					.get("changed_paths")
					.cloned()
					.unwrap_or_default();
				let changed_paths = if let Some(rev) = &since {
					if !explicit_paths.is_empty() {
						eprintln!("warning: --since takes priority; --changed-paths was ignored");
					}
					compute_changed_paths_since(root, rev)?
				} else {
					explicit_paths
				};
				let labels = step_inputs.get("label").cloned().unwrap_or_default();
				let enforce = step_inputs
					.get("verify")
					.is_some_and(|values| values.iter().any(|v| v == "true"));
				let mut evaluation = affected_packages(root, &changed_paths, &labels)?;
				evaluation.enforce = enforce;
				context.changeset_policy_evaluation = Some(evaluation);
				output = None;
			}
			CliStepDefinition::DiagnoseChangesets { .. } => {
				let requested = step_inputs.get("changeset").cloned().unwrap_or_default();
				let report = diagnose_changesets(root, &requested)?;
				context.changeset_diagnostics = Some(report);
				output = None;
			}
			CliStepDefinition::RetargetRelease { .. } => {
				let from = step_inputs
					.get("from")
					.and_then(|values| values.first())
					.cloned()
					.ok_or_else(|| {
						MonochangeError::Config(
							"`RetargetRelease` requires a `from` input".to_string(),
						)
					})?;
				let target = step_inputs
					.get("target")
					.and_then(|values| values.first())
					.cloned()
					.unwrap_or_else(|| "HEAD".to_string());
				let force = parse_boolean_step_input(&step_inputs, "force")?.unwrap_or(false);
				let sync_provider =
					parse_boolean_step_input(&step_inputs, "sync_provider")?.unwrap_or(true);
				let discovery = discover_release_record(root, &from)?;
				let source = inferred_retarget_source_configuration(
					configuration.source.as_ref(),
					&discovery,
					sync_provider,
				);
				let plan = plan_release_retarget(
					root,
					&discovery,
					&target,
					force,
					sync_provider,
					context.dry_run,
					source.as_ref(),
				)?;
				let result = execute_release_retarget(root, source.as_ref(), &plan)?;
				context.retarget_report = Some(build_retarget_release_report(
					&from,
					&target,
					&discovery,
					plan.is_descendant,
					&result,
				));
				output = None;
			}
			CliStepDefinition::Command {
				command,
				dry_run_command,
				shell,
				id,
				variables,
				..
			} => run_cli_command_command(
				&mut context,
				command,
				dry_run_command.as_deref(),
				shell,
				id.as_deref(),
				variables.as_ref(),
				&step_inputs,
			)?,
		}
	}

	if let Some(prepared_release) = &context.prepared_release {
		let format = cli_command_output_format(&context.last_step_inputs)?;
		return match format {
			OutputFormat::Json => {
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				render_release_cli_command_json(
					&manifest,
					&context.release_requests,
					context.release_request.as_ref(),
					&context.issue_comment_plans,
					context.release_commit_report.as_ref(),
				)
			}
			OutputFormat::Text => Ok(render_cli_command_result(cli_command, &context)),
		};
	}
	if let Some(evaluation) = &context.changeset_policy_evaluation {
		let format = cli_command_output_format(&context.last_step_inputs)?;
		let rendered = match format {
			OutputFormat::Json => serde_json::to_string_pretty(evaluation).map_err(|error| {
				MonochangeError::Config(format!(
					"failed to render changeset policy evaluation as json: {error}"
				))
			})?,
			OutputFormat::Text => render_cli_command_result(cli_command, &context),
		};
		if evaluation.enforce && evaluation.status == ChangesetPolicyStatus::Failed {
			println!("{rendered}");
			return Err(MonochangeError::Config(evaluation.summary.clone()));
		}
		return Ok(rendered);
	}
	if let Some(report) = &context.changeset_diagnostics {
		let format = context
			.inputs
			.get("format")
			.and_then(|values| values.first())
			.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;
		let rendered = match format {
			OutputFormat::Json => serde_json::to_string_pretty(report).map_err(|error| {
				MonochangeError::Config(format!(
					"failed to render changeset diagnostics as json: {error}"
				))
			})?,
			OutputFormat::Text => render_changeset_diagnostics(report),
		};
		return Ok(rendered);
	}
	if let Some(report) = &context.retarget_report {
		let format = cli_command_output_format(&context.last_step_inputs)?;
		let rendered = match format {
			OutputFormat::Json => serde_json::to_string_pretty(report)
				.unwrap_or_else(|error| panic!("retarget report serialization bug: {error}")),
			OutputFormat::Text => render_retarget_release_report(report),
		};
		return Ok(rendered);
	}
	if !context.command_logs.is_empty() {
		return Ok(render_cli_command_result(cli_command, &context));
	}

	Ok(output.unwrap_or_else(|| {
		format!(
			"command `{}` completed{}",
			cli_command.name,
			if dry_run { " (dry-run)" } else { "" }
		)
	}))
}

fn run_cli_command_command(
	context: &mut CliContext,
	command: &str,
	dry_run_command: Option<&str>,
	shell: &ShellConfig,
	step_id: Option<&str>,
	variables: Option<&BTreeMap<String, CommandVariable>>,
	step_inputs: &BTreeMap<String, Vec<String>>,
) -> MonochangeResult<()> {
	let command_to_run = if context.dry_run {
		if let Some(command) = dry_run_command {
			command
		} else {
			let skipped = interpolate_cli_command_command(context, command, variables, step_inputs);
			context
				.command_logs
				.push(format!("skipped command `{skipped}` (dry-run)"));
			return Ok(());
		}
	} else {
		command
	};
	let interpolated =
		interpolate_cli_command_command(context, command_to_run, variables, step_inputs);

	let output = if let Some(shell_binary) = shell.shell_binary() {
		ProcessCommand::new(shell_binary)
			.arg("-c")
			.arg(&interpolated)
			.current_dir(&context.root)
			.output()
	} else {
		let parts = shlex::split(&interpolated).ok_or_else(|| {
			MonochangeError::Config(format!("failed to parse command `{interpolated}`"))
		})?;
		let Some((program, args)) = parts.split_first() else {
			return Err(MonochangeError::Config(
				"command must not be empty".to_string(),
			));
		};
		ProcessCommand::new(program)
			.args(args)
			.current_dir(&context.root)
			.output()
	};
	let output = output.map_err(|error| {
		MonochangeError::Io(format!("failed to run command `{interpolated}`: {error}"))
	})?;
	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
		let details = if stderr.is_empty() {
			format!("exit status {}", output.status)
		} else {
			stderr
		};
		return Err(MonochangeError::Discovery(format!(
			"command `{interpolated}` failed: {details}"
		)));
	}

	let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
	let stderr_text = String::from_utf8_lossy(&output.stderr).trim().to_string();

	if let Some(id) = step_id {
		context.step_outputs.insert(
			id.to_string(),
			CommandStepOutput {
				stdout: stdout.clone(),
				stderr: stderr_text,
			},
		);
	}

	if stdout.is_empty() {
		context.command_logs.push(format!("ran `{interpolated}`"));
	} else {
		context.command_logs.push(stdout);
	}
	Ok(())
}

fn cli_inputs_template_value(
	inputs: &BTreeMap<String, Vec<String>>,
) -> serde_json::Map<String, serde_json::Value> {
	inputs
		.iter()
		.map(|(input_name, input_values)| {
			(input_name.clone(), cli_input_template_value(input_values))
		})
		.collect()
}

fn cli_input_template_value(input_values: &[String]) -> serde_json::Value {
	if input_values.len() == 1 {
		let value = input_values.first().map_or("", String::as_str);
		if value == "true" || value == "false" {
			return serde_json::Value::Bool(value == "true");
		}
		return serde_json::Value::String(value.to_string());
	}
	if input_values.is_empty() {
		return serde_json::Value::Bool(false);
	}
	serde_json::Value::Array(
		input_values
			.iter()
			.cloned()
			.map(serde_json::Value::String)
			.collect(),
	)
}

fn build_cli_template_context(
	context: &CliContext,
	inputs: &BTreeMap<String, Vec<String>>,
	variables: Option<&BTreeMap<String, CommandVariable>>,
) -> serde_json::Map<String, serde_json::Value> {
	let mut template_context = serde_json::Map::new();

	template_context.insert(
		"version".to_string(),
		serde_json::Value::String(cli_command_variable_value(
			context,
			CommandVariable::Version,
		)),
	);
	template_context.insert(
		"group_version".to_string(),
		serde_json::Value::String(cli_command_variable_value(
			context,
			CommandVariable::GroupVersion,
		)),
	);
	template_context.insert(
		"released_packages".to_string(),
		serde_json::Value::String(cli_command_variable_value(
			context,
			CommandVariable::ReleasedPackages,
		)),
	);
	template_context.insert(
		"changed_files".to_string(),
		serde_json::Value::String(cli_command_variable_value(
			context,
			CommandVariable::ChangedFiles,
		)),
	);
	template_context.insert(
		"changesets".to_string(),
		serde_json::Value::String(cli_command_variable_value(
			context,
			CommandVariable::Changesets,
		)),
	);

	if let Some(prepared) = &context.prepared_release {
		template_context.insert(
			"released_packages_list".to_string(),
			serde_json::Value::Array(
				prepared
					.released_packages
					.iter()
					.cloned()
					.map(serde_json::Value::String)
					.collect(),
			),
		);
	}

	// Structured release.* namespace
	template_context.insert("release".to_string(), build_release_template_value(context));

	// Structured manifest.* namespace
	if let Some(path) = &context.release_manifest_path {
		let mut manifest_map = serde_json::Map::new();
		manifest_map.insert(
			"path".to_string(),
			serde_json::Value::String(path.display().to_string()),
		);
		template_context.insert(
			"manifest".to_string(),
			serde_json::Value::Object(manifest_map),
		);
	}

	// Structured affected.* namespace
	if let Some(evaluation) = &context.changeset_policy_evaluation {
		let mut affected_map = serde_json::Map::new();
		affected_map.insert(
			"status".to_string(),
			serde_json::Value::String(evaluation.status.to_string()),
		);
		affected_map.insert(
			"summary".to_string(),
			serde_json::Value::String(evaluation.summary.clone()),
		);
		template_context.insert(
			"affected".to_string(),
			serde_json::Value::Object(affected_map),
		);
	}

	// Structured retarget.* namespace
	if let Some(report) = &context.retarget_report {
		template_context.insert(
			"retarget".to_string(),
			build_retarget_template_value(report),
		);
	}

	// Structured release_commit.* namespace
	if let Some(report) = &context.release_commit_report {
		template_context.insert(
			"release_commit".to_string(),
			build_release_commit_template_value(report),
		);
	}

	// Structured steps.<id>.* namespace from command step outputs
	if !context.step_outputs.is_empty() {
		let mut steps_map = serde_json::Map::new();
		for (id, output) in &context.step_outputs {
			let mut output_map = serde_json::Map::new();
			output_map.insert(
				"stdout".to_string(),
				serde_json::Value::String(output.stdout.clone()),
			);
			output_map.insert(
				"stderr".to_string(),
				serde_json::Value::String(output.stderr.clone()),
			);
			steps_map.insert(id.clone(), serde_json::Value::Object(output_map));
		}
		template_context.insert("steps".to_string(), serde_json::Value::Object(steps_map));
	}

	let input_context = cli_inputs_template_value(inputs);
	for (input_name, input_value) in &input_context {
		template_context.insert(input_name.clone(), input_value.clone());
	}
	template_context.insert(
		"inputs".to_string(),
		serde_json::Value::Object(input_context),
	);

	if let Some(variables) = variables {
		for (needle, variable) in variables {
			template_context.insert(
				needle.clone(),
				serde_json::Value::String(cli_command_variable_value(context, *variable)),
			);
		}
	}

	template_context
}

fn build_release_template_value(context: &CliContext) -> serde_json::Value {
	let Some(prepared) = &context.prepared_release else {
		return serde_json::Value::Null;
	};

	let mut release_map = serde_json::Map::new();
	release_map.insert(
		"version".to_string(),
		prepared
			.version
			.as_deref()
			.map_or(serde_json::Value::Null, |v| {
				serde_json::Value::String(v.to_string())
			}),
	);
	release_map.insert(
		"group_version".to_string(),
		prepared
			.group_version
			.as_deref()
			.map_or(serde_json::Value::Null, |v| {
				serde_json::Value::String(v.to_string())
			}),
	);
	release_map.insert(
		"dry_run".to_string(),
		serde_json::Value::Bool(prepared.dry_run),
	);
	release_map.insert(
		"released_packages".to_string(),
		serde_json::Value::Array(
			prepared
				.released_packages
				.iter()
				.cloned()
				.map(serde_json::Value::String)
				.collect(),
		),
	);
	release_map.insert(
		"changed_files".to_string(),
		serde_json::Value::Array(
			prepared
				.changed_files
				.iter()
				.map(|p| serde_json::Value::String(p.display().to_string()))
				.collect(),
		),
	);
	release_map.insert(
		"updated_changelogs".to_string(),
		serde_json::Value::Array(
			prepared
				.updated_changelogs
				.iter()
				.map(|p| serde_json::Value::String(p.display().to_string()))
				.collect(),
		),
	);
	release_map.insert(
		"deleted_changesets".to_string(),
		serde_json::Value::Array(
			prepared
				.deleted_changesets
				.iter()
				.map(|p| serde_json::Value::String(p.display().to_string()))
				.collect(),
		),
	);
	release_map.insert(
		"changeset_paths".to_string(),
		serde_json::Value::Array(
			prepared
				.changeset_paths
				.iter()
				.map(|p| serde_json::Value::String(p.display().to_string()))
				.collect(),
		),
	);

	let targets: Vec<serde_json::Value> = prepared
		.release_targets
		.iter()
		.map(|target| {
			let mut target_map = serde_json::Map::new();
			target_map.insert(
				"id".to_string(),
				serde_json::Value::String(target.id.clone()),
			);
			target_map.insert(
				"version".to_string(),
				serde_json::Value::String(target.tag_name.clone()),
			);
			target_map.insert(
				"kind".to_string(),
				serde_json::Value::String(target.kind.to_string()),
			);
			target_map.insert("tag".to_string(), serde_json::Value::Bool(target.tag));
			serde_json::Value::Object(target_map)
		})
		.collect();
	release_map.insert("targets".to_string(), serde_json::Value::Array(targets));

	serde_json::Value::Object(release_map)
}

fn build_retarget_template_value(report: &RetargetReleaseReport) -> serde_json::Value {
	serde_json::to_value(report).unwrap_or(serde_json::Value::Null)
}

fn build_release_commit_template_value(report: &CommitReleaseReport) -> serde_json::Value {
	serde_json::to_value(report).unwrap_or(serde_json::Value::Null)
}

fn parse_boolean_step_input(
	inputs: &BTreeMap<String, Vec<String>>,
	name: &str,
) -> MonochangeResult<Option<bool>> {
	inputs
		.get(name)
		.and_then(|values| values.first())
		.map(|value| match value.as_str() {
			"true" => Ok(true),
			"false" => Ok(false),
			other => Err(MonochangeError::Config(format!(
				"invalid boolean value `{other}` for `{name}`"
			))),
		})
		.transpose()
}

fn inferred_retarget_source_configuration(
	configured_source: Option<&SourceConfiguration>,
	discovery: &ReleaseRecordDiscovery,
	sync_provider: bool,
) -> Option<SourceConfiguration> {
	if let Some(source) = configured_source {
		return Some(source.clone());
	}
	if !sync_provider {
		return None;
	}
	let provider = discovery.record.provider.as_ref()?;
	Some(SourceConfiguration {
		provider: provider.kind,
		owner: provider.owner.clone(),
		repo: provider.repo.clone(),
		host: provider.host.clone(),
		api_url: None,
		releases: monochange_core::ReleaseProviderSettings::default(),
		pull_requests: monochange_core::ChangeRequestSettings::default(),
		bot: monochange_core::BotSettings::default(),
	})
}

fn build_retarget_release_report(
	from: &str,
	target: &str,
	discovery: &ReleaseRecordDiscovery,
	is_descendant: bool,
	result: &RetargetResult,
) -> RetargetReleaseReport {
	RetargetReleaseReport {
		from: from.to_string(),
		target: target.to_string(),
		resolved_from_commit: discovery.resolved_commit.clone(),
		record_commit: result.record_commit.clone(),
		target_commit: result.target_commit.clone(),
		distance: discovery.distance,
		is_descendant,
		force: result.force,
		dry_run: result.dry_run,
		sync_provider: result.sync_provider,
		tags: result
			.git_tag_results
			.iter()
			.map(|tag_result| tag_result.tag_name.clone())
			.collect(),
		git_tag_results: result.git_tag_results.clone(),
		provider_results: result.provider_results.clone(),
		status: if result.dry_run {
			"dry_run".to_string()
		} else {
			"completed".to_string()
		},
	}
}

fn render_release_commit_report(report: &CommitReleaseReport) -> Vec<String> {
	let mut lines = vec!["release commit:".to_string()];
	lines.push(format!("  subject: {}", report.subject));
	if let Some(commit) = &report.commit {
		lines.push(format!("  commit: {}", short_commit_sha(commit)));
	}
	lines.extend((!report.tracked_paths.is_empty()).then_some("  tracked paths:".to_string()));
	#[rustfmt::skip]
	lines.extend(report.tracked_paths.iter().map(|path| format!("    - {}", path.display())));
	lines.push(format!("  status: {}", report.status.replace('_', "-")));
	lines
}

fn render_retarget_release_report(report: &RetargetReleaseReport) -> String {
	let mut lines = vec!["repair release:".to_string()];
	lines.push(format!("  from: {}", report.from));
	lines.push(format!(
		"  resolved commit: {}",
		short_commit_sha(&report.resolved_from_commit)
	));
	lines.push(format!(
		"  record commit: {}",
		short_commit_sha(&report.record_commit)
	));
	lines.push(format!(
		"  target: {}",
		short_commit_sha(&report.target_commit)
	));
	lines.push(format!(
		"  descendant: {}",
		if report.is_descendant { "yes" } else { "no" }
	));
	lines.push(format!(
		"  force: {}",
		if report.force { "yes" } else { "no" }
	));
	if !report.git_tag_results.is_empty() {
		lines.push("  tags to move:".to_string());
		for tag_result in &report.git_tag_results {
			lines.push(format!(
				"    - {} ({} -> {}) [{}]",
				tag_result.tag_name,
				short_commit_sha(&tag_result.from_commit),
				short_commit_sha(&tag_result.to_commit),
				retarget_operation_label(tag_result.operation),
			));
		}
	}
	lines.push(format!(
		"  provider sync: {}",
		if !report.sync_provider {
			"disabled".to_string()
		} else if let Some(provider_result) = report.provider_results.first() {
			provider_result.provider.to_string()
		} else {
			"none".to_string()
		}
	));
	lines.push(format!("  status: {}", report.status.replace('_', "-")));
	lines.join("\n")
}

fn retarget_operation_label(operation: RetargetOperation) -> &'static str {
	match operation {
		RetargetOperation::Planned => "planned",
		RetargetOperation::Moved => "moved",
		RetargetOperation::AlreadyUpToDate => "already_up_to_date",
		RetargetOperation::Skipped => "skipped",
		RetargetOperation::Failed => "failed",
	}
}

fn interpolate_cli_command_command(
	context: &CliContext,
	command: &str,
	variables: Option<&BTreeMap<String, CommandVariable>>,
	step_inputs: &BTreeMap<String, Vec<String>>,
) -> String {
	let template_context = build_cli_template_context(context, step_inputs, variables);
	let jinja_context =
		minijinja::Value::from_serialize(serde_json::Value::Object(template_context));
	render_jinja_template(command, &jinja_context).unwrap_or_else(|_| command.to_string())
}

fn cli_command_variable_value(context: &CliContext, variable: CommandVariable) -> String {
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

fn render_cli_command_result(cli_command: &CliCommandDefinition, context: &CliContext) -> String {
	if let Some(report) = &context.retarget_report {
		return render_retarget_release_report(report);
	}

	let mut lines = vec![format!(
		"command `{}` completed{}",
		cli_command.name,
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
		if !context.release_results.is_empty() {
			lines.push("releases:".to_string());
			for release in &context.release_results {
				lines.push(format!("- {release}"));
			}
		}
		if let Some(release_commit_report) = &context.release_commit_report {
			lines.extend(render_release_commit_report(release_commit_report));
		}
		if let Some(release_request_result) = &context.release_request_result {
			lines.push("release request:".to_string());
			lines.push(format!("- {release_request_result}"));
		}
		if !context.issue_comment_results.is_empty() {
			lines.push("issue comments:".to_string());
			for issue_comment in &context.issue_comment_results {
				lines.push(format!("- {issue_comment}"));
			}
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
	if let Some(evaluation) = &context.changeset_policy_evaluation {
		lines.push(format!("changeset policy: {}", evaluation.status));
		lines.push(evaluation.summary.clone());
		if !evaluation.matched_skip_labels.is_empty() {
			lines.push(format!(
				"matched skip labels: {}",
				evaluation.matched_skip_labels.join(", ")
			));
		}
		if !evaluation.matched_paths.is_empty() {
			lines.push("matched paths:".to_string());
			for path in &evaluation.matched_paths {
				lines.push(format!("- {path}"));
			}
		}
		if !evaluation.changeset_paths.is_empty() {
			lines.push("changeset files:".to_string());
			for path in &evaluation.changeset_paths {
				lines.push(format!("- {path}"));
			}
		}
		if !evaluation.errors.is_empty() {
			lines.push("errors:".to_string());
			for error in &evaluation.errors {
				lines.push(format!("- {error}"));
			}
		}
	}
	if !context.command_logs.is_empty() {
		lines.push("commands:".to_string());
		for log in &context.command_logs {
			lines.push(format!("- {log}"));
		}
	}
	lines.join(
		"
",
	)
}

fn cli_command_output_format(
	inputs: &BTreeMap<String, Vec<String>>,
) -> MonochangeResult<OutputFormat> {
	inputs
		.get("format")
		.and_then(|values| values.first())
		.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))
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
		"none" => Ok(ChangeBump::None),
		"patch" => Ok(ChangeBump::Patch),
		"minor" => Ok(ChangeBump::Minor),
		"major" => Ok(ChangeBump::Major),
		other => Err(MonochangeError::Config(format!(
			"unsupported bump `{other}`"
		))),
	}
}

fn diagnose_changesets(
	root: &Path,
	requested: &[String],
) -> MonochangeResult<ChangesetDiagnosticsReport> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;
	let changeset_paths = if requested.is_empty() {
		discover_changeset_paths(root)?
			.into_iter()
			.map(|path| root.join(path))
			.collect::<Vec<_>>()
	} else {
		let mut resolved = Vec::new();
		for path in requested {
			resolved.push(resolve_changeset_path(root, path)?);
		}
		resolved.sort();
		resolved.dedup();
		resolved
	};

	let loaded_changesets = changeset_paths
		.iter()
		.map(|path| load_changeset_file(path, &configuration, &discovery.packages))
		.collect::<MonochangeResult<Vec<_>>>()?;
	let mut changesets = build_prepared_changesets(root, &loaded_changesets);
	if let Some(source) = configuration
		.source
		.as_ref()
		.filter(|source| source.provider == SourceProvider::GitHub)
	{
		github_provider::enrich_changeset_context(source, &mut changesets);
	}

	let requested_changesets = changeset_paths
		.iter()
		.map(|path| root_relative(root, path))
		.collect();
	Ok(ChangesetDiagnosticsReport {
		requested_changesets,
		changesets,
	})
}

fn resolve_changeset_path(root: &Path, requested: &str) -> MonochangeResult<PathBuf> {
	let requested_is_absolute = Path::new(requested).is_absolute();
	let normalized = if requested_is_absolute {
		requested.to_string()
	} else {
		normalize_changed_path(requested)
	};
	if normalized.is_empty() {
		return Err(MonochangeError::Config(
			"changeset path cannot be empty".to_string(),
		));
	}

	let candidate = if requested_is_absolute {
		Path::new(requested)
	} else {
		Path::new(&normalized)
	};
	let candidates = if candidate.is_absolute() {
		vec![candidate.to_path_buf()]
	} else {
		let mut candidates = vec![root.join(candidate)];
		if !normalized.starts_with(CHANGESET_DIR) {
			candidates.push(root.join(CHANGESET_DIR).join(candidate));
		}
		candidates
	};

	for candidate in candidates {
		let Some(relative_candidate) = relative_to_root(root, &candidate) else {
			continue;
		};
		if !is_changeset_markdown_path(&relative_candidate.to_string_lossy()) {
			continue;
		}
		if candidate.exists() {
			return Ok(candidate);
		}
	}
	Err(MonochangeError::Config(format!(
		"requested changeset `{requested}` does not exist"
	)))
}

fn render_changeset_diagnostics(report: &ChangesetDiagnosticsReport) -> String {
	if report.changesets.is_empty() {
		return "no matching changesets found".to_string();
	}

	let mut lines = Vec::new();
	for changeset in &report.changesets {
		let change_summary = changeset.summary.as_deref().unwrap_or("<missing summary>");
		lines.push(format!("changeset: {}", changeset.path.display()));
		lines.push(format!("  summary: {change_summary}"));
		if let Some(details) = &changeset.details {
			lines.push(format!("  details: {details}"));
		}
		if !changeset.targets.is_empty() {
			lines.push("  targets:".to_string());
			for target in &changeset.targets {
				let bump = target
					.bump
					.map_or_else(|| "auto".to_string(), |bump| bump.to_string());
				lines.push(format!(
					"  - {} {} (bump: {}, origin: {})",
					target.kind, target.id, bump, target.origin,
				));
				if !target.evidence_refs.is_empty() {
					lines.push(format!("    evidence: {}", target.evidence_refs.join(", ")));
				}
			}
		}
		if let Some(context) = &changeset.context {
			if let Some(introduced) = context
				.introduced
				.as_ref()
				.and_then(|revision| revision.commit.as_ref())
			{
				lines.push(format!("  introduced: {}", introduced.short_sha));
			}
			if let Some(last_updated) = context
				.last_updated
				.as_ref()
				.and_then(|revision| revision.commit.as_ref())
			{
				lines.push(format!("  last-updated: {}", last_updated.short_sha));
			}
			let review_request = context
				.introduced
				.as_ref()
				.and_then(|revision| revision.review_request.as_ref())
				.or_else(|| {
					context
						.last_updated
						.as_ref()
						.and_then(|revision| revision.review_request.as_ref())
				});
			if let Some(review_request) = review_request {
				if let Some(url) = &review_request.url {
					lines.push(format!("  review request: {} ({})", review_request.id, url));
				} else {
					lines.push(format!("  review request: {}", review_request.id));
				}
			}
			if !context.related_issues.is_empty() {
				let issues = context
					.related_issues
					.iter()
					.map(|issue| issue.id.as_str())
					.collect::<Vec<_>>()
					.join(", ");
				lines.push(format!("  related issues: {issues}"));
			}
		}
		lines.push(String::new());
	}
	lines.pop();
	lines.join("\n")
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

fn build_prepared_changesets(
	root: &Path,
	loaded_changesets: &[monochange_config::LoadedChangesetFile],
) -> Vec<PreparedChangeset> {
	loaded_changesets
		.iter()
		.map(|changeset| PreparedChangeset {
			path: root_relative(root, &changeset.path),
			summary: changeset.summary.clone(),
			details: changeset.details.clone(),
			targets: changeset
				.targets
				.iter()
				.map(|target| PreparedChangesetTarget {
					id: target.id.clone(),
					kind: target.kind,
					bump: target.bump,
					origin: target.origin.clone(),
					evidence_refs: target.evidence_refs.clone(),
					change_type: target.change_type.clone(),
				})
				.collect(),
			context: Some(build_generic_changeset_context(root, &changeset.path)),
		})
		.collect()
}

fn build_generic_changeset_context(root: &Path, changeset_path: &Path) -> ChangesetContext {
	ChangesetContext {
		provider: HostingProviderKind::GenericGit,
		host: None,
		capabilities: HostingCapabilities::default(),
		introduced: load_git_changeset_revision(root, changeset_path, true),
		last_updated: load_git_changeset_revision(root, changeset_path, false),
		related_issues: Vec::new(),
	}
}

fn load_git_changeset_revision(
	root: &Path,
	changeset_path: &Path,
	introduced: bool,
) -> Option<ChangesetRevision> {
	let relative_path = root_relative(root, changeset_path);
	let mut command = ProcessCommand::new("git");
	command.current_dir(root).arg("log").arg("--follow");
	if introduced {
		command.arg("--diff-filter=A");
	}
	command
		.arg("-n")
		.arg("1")
		.arg("--format=%H%x1f%an%x1f%ae%x1f%aI%x1f%cI")
		.arg("--")
		.arg(&relative_path);
	let output = command.output().ok()?;
	if !output.status.success() {
		return None;
	}
	let stdout = String::from_utf8(output.stdout).ok()?;
	let trimmed = stdout.trim();
	if trimmed.is_empty() {
		return None;
	}
	let parts = trimmed.split('\u{1f}').collect::<Vec<_>>();
	let [sha, author_name, author_email, authored_at, committed_at] = parts.as_slice() else {
		return None;
	};
	let author_name = (*author_name).to_string();
	let author_email = (*author_email).to_string();
	Some(ChangesetRevision {
		actor: Some(HostedActorRef {
			provider: HostingProviderKind::GenericGit,
			host: None,
			id: None,
			login: None,
			display_name: Some(author_name.clone()),
			url: None,
			source: HostedActorSourceKind::CommitAuthor,
		}),
		commit: Some(HostedCommitRef {
			provider: HostingProviderKind::GenericGit,
			host: None,
			sha: (*sha).to_string(),
			short_sha: short_commit_sha(sha),
			url: None,
			authored_at: Some((*authored_at).to_string()),
			committed_at: Some((*committed_at).to_string()),
			author_name: Some(author_name),
			author_email: Some(author_email),
		}),
		review_request: None,
	})
}

fn short_commit_sha(sha: &str) -> String {
	sha.chars().take(7).collect()
}

fn build_release_plan_from_signals(
	configuration: &monochange_core::WorkspaceConfiguration,
	discovery: &DiscoveryReport,
	change_signals: &[ChangeSignal],
) -> MonochangeResult<ReleasePlan> {
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
		configuration.defaults.strict_version_conflicts,
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

#[allow(clippy::too_many_arguments)]
fn build_changelog_updates(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
	change_signals: &[ChangeSignal],
	changesets: &[PreparedChangeset],
	changelog_targets: &(PackageChangelogTargets, GroupChangelogTargets),
	release_targets: &[ReleaseTarget],
) -> MonochangeResult<Vec<ChangelogUpdate>> {
	let changeset_context_by_path = changesets
		.iter()
		.map(|changeset| {
			(
				changeset.path.clone(),
				build_rendered_changeset_context(root, changeset),
			)
		})
		.collect::<BTreeMap<_, _>>();
	let changeset_targets_by_path = changesets
		.iter()
		.map(|changeset| (changeset.path.clone(), changeset.targets.clone()))
		.collect::<BTreeMap<_, _>>();
	let release_note_changes = change_signals
		.iter()
		.filter_map(|signal| {
			build_release_note_change(signal, packages, root, &changeset_context_by_path)
		})
		.fold(
			BTreeMap::<String, Vec<ReleaseNoteChange>>::new(),
			|mut acc, change| {
				acc.entry(change.package_id.clone())
					.or_default()
					.push(change);
				acc
			},
		);

	let group_definitions_by_id = configuration
		.groups
		.iter()
		.map(|group| (group.id.as_str(), group))
		.collect::<BTreeMap<_, _>>();
	let package_definitions_by_record_id = packages
		.iter()
		.filter_map(|package| {
			package.metadata.get("config_id").and_then(|config_id| {
				configuration
					.package_by_id(config_id)
					.map(|definition| (package.id.as_str(), definition))
			})
		})
		.collect::<BTreeMap<_, _>>();

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
		let package_id = config_package_id(package);
		let package_definition = package_definitions_by_record_id
			.get(decision.package_id.as_str())
			.copied();
		let group_definition = decision
			.group_id
			.as_deref()
			.and_then(|group_id| group_definitions_by_id.get(group_id).copied());
		let changes = package_release_note_changes(
			configuration,
			package_definition,
			group_definition,
			decision,
			package,
			release_note_changes.get(&decision.package_id),
			&planned_version.to_string(),
		);
		let changelog_title = release_targets
			.iter()
			.find(|rt| {
				(rt.kind == ReleaseOwnerKind::Package && rt.id == package_id)
					|| (rt.kind == ReleaseOwnerKind::Group && rt.members.contains(&package_id))
			})
			.map_or_else(
				|| planned_version.to_string(),
				|rt| rt.rendered_changelog_title.clone(),
			);
		let document = build_release_notes_document(
			&package_id,
			&changelog_title,
			Vec::new(),
			package_definition.map_or(&[][..], |package| {
				package.extra_changelog_sections.as_slice()
			}),
			&configuration.release_notes.change_templates,
			&changes,
		);
		let rendered = render_release_notes(changelog_target.format, &document);
		updates.push(ChangelogUpdate {
			file: FileUpdate {
				path: changelog_target.path.clone(),
				content: append_changelog_section(&changelog_target.path, &rendered)?.into_bytes(),
			},
			owner_id: package_id,
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
		let member_ids = configuration
			.groups
			.iter()
			.find(|group| group.id == planned_group.group_id)
			.map(|group| group.packages.clone())
			.unwrap_or_default();
		let group_definition = group_definitions_by_id
			.get(planned_group.group_id.as_str())
			.copied();
		let changes = group_release_note_changes(
			configuration,
			group_definition,
			planned_group,
			&release_note_changes,
			&changeset_targets_by_path,
			packages,
			&planned_version.to_string(),
		);
		let changed_members = group_changed_members(planned_group, &release_note_changes, packages);
		let changelog_title = release_targets
			.iter()
			.find(|rt| rt.kind == ReleaseOwnerKind::Group && rt.id == planned_group.group_id)
			.map_or_else(
				|| planned_version.to_string(),
				|rt| rt.rendered_changelog_title.clone(),
			);
		let document = build_release_notes_document(
			&planned_group.group_id,
			&changelog_title,
			group_release_summary(&planned_group.group_id, &member_ids, &changed_members),
			group_definition.map_or(&[][..], |group| group.extra_changelog_sections.as_slice()),
			&configuration.release_notes.change_templates,
			&changes,
		);
		let rendered = render_release_notes(changelog_target.format, &document);
		updates.push(ChangelogUpdate {
			file: FileUpdate {
				path: changelog_target.path.clone(),
				content: append_changelog_section(&changelog_target.path, &rendered)?.into_bytes(),
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

fn build_release_note_change(
	signal: &ChangeSignal,
	packages: &[PackageRecord],
	root: &Path,
	changeset_context_by_path: &BTreeMap<PathBuf, RenderedChangesetContext>,
) -> Option<ReleaseNoteChange> {
	let summary = signal.notes.clone()?;
	let package = packages
		.iter()
		.find(|package| package.id == signal.package_id)?;
	let package_id = config_package_id(package);
	let source_path = root_relative(root, &signal.source_path);
	let rendered_context = changeset_context_by_path.get(&source_path);
	Some(ReleaseNoteChange {
		package_id: signal.package_id.clone(),
		package_name: package_id.clone(),
		package_labels: Vec::new(),
		source_path: Some(source_path.display().to_string()),
		summary,
		details: signal.details.clone(),
		bump: signal.requested_bump.unwrap_or(BumpSeverity::Patch),
		change_type: signal.change_type.clone(),
		context: rendered_context.map(|context| context.context.clone()),
		changeset_path: rendered_context.map(|context| context.changeset_path.clone()),
		change_owner: rendered_context.and_then(|context| context.change_owner.clone()),
		change_owner_link: rendered_context.and_then(|context| context.change_owner_link.clone()),
		review_request: rendered_context.and_then(|context| context.review_request.clone()),
		review_request_link: rendered_context
			.and_then(|context| context.review_request_link.clone()),
		introduced_commit: rendered_context.and_then(|context| context.introduced_commit.clone()),
		introduced_commit_link: rendered_context
			.and_then(|context| context.introduced_commit_link.clone()),
		last_updated_commit: rendered_context
			.and_then(|context| context.last_updated_commit.clone()),
		last_updated_commit_link: rendered_context
			.and_then(|context| context.last_updated_commit_link.clone()),
		related_issues: rendered_context.and_then(|context| context.related_issues.clone()),
		related_issue_links: rendered_context
			.and_then(|context| context.related_issue_links.clone()),
		closed_issues: rendered_context.and_then(|context| context.closed_issues.clone()),
		closed_issue_links: rendered_context.and_then(|context| context.closed_issue_links.clone()),
	})
}

fn build_rendered_changeset_context(
	root: &Path,
	changeset: &PreparedChangeset,
) -> RenderedChangesetContext {
	let changeset_path = root_relative(root, &changeset.path).display().to_string();
	let mut rendered = RenderedChangesetContext {
		changeset_path: changeset_path.clone(),
		..RenderedChangesetContext::default()
	};
	let mut lines = Vec::new();
	let Some(context) = changeset.context.as_ref() else {
		rendered.context = lines.join("\n");
		return rendered;
	};

	let primary_revision = context
		.introduced
		.as_ref()
		.or(context.last_updated.as_ref());
	if let Some(actor) = primary_revision.and_then(|revision| revision.actor.as_ref()) {
		let label = render_actor_label(actor);
		let link = render_markdown_link(&label, actor.url.as_deref());
		rendered.change_owner = Some(label.clone());
		rendered.change_owner_link = Some(link.clone());
		lines.push(format!("> _Owner:_ {link}"));
	}

	let review_request = context
		.introduced
		.as_ref()
		.and_then(|revision| revision.review_request.as_ref())
		.or_else(|| {
			context
				.last_updated
				.as_ref()
				.and_then(|revision| revision.review_request.as_ref())
		});
	if let Some(review_request) = review_request {
		let label = render_review_request_label(review_request);
		let link = render_markdown_link(&label, review_request.url.as_deref());
		rendered.review_request = Some(label.clone());
		rendered.review_request_link = Some(link.clone());
		lines.push(format!("> _Review:_ {link}"));
	}

	if let Some(commit) = context
		.introduced
		.as_ref()
		.and_then(|revision| revision.commit.as_ref())
	{
		let label = commit.short_sha.clone();
		let link = render_markdown_link(&format!("`{label}`"), commit.url.as_deref());
		rendered.introduced_commit = Some(label);
		rendered.introduced_commit_link = Some(link.clone());
		lines.push(format!("> _Introduced in:_ {link}"));
	}

	let introduced_sha = context
		.introduced
		.as_ref()
		.and_then(|revision| revision.commit.as_ref())
		.map(|commit| commit.sha.as_str());
	if let Some(commit) = context
		.last_updated
		.as_ref()
		.and_then(|revision| revision.commit.as_ref())
		.filter(|commit| Some(commit.sha.as_str()) != introduced_sha)
	{
		let label = commit.short_sha.clone();
		let link = render_markdown_link(&format!("`{label}`"), commit.url.as_deref());
		rendered.last_updated_commit = Some(label);
		rendered.last_updated_commit_link = Some(link.clone());
		lines.push(format!("> _Last updated in:_ {link}"));
	}

	let closed_issues = context
		.related_issues
		.iter()
		.filter(|issue| issue.relationship == HostedIssueRelationshipKind::ClosedByReviewRequest)
		.collect::<Vec<_>>();
	if !closed_issues.is_empty() {
		let labels = render_issue_labels(&closed_issues);
		let issue_links = render_issue_links(&closed_issues);
		rendered.closed_issues = Some(labels.clone());
		rendered.closed_issue_links = Some(issue_links.clone());
		lines.push(format!("> _Closed issues:_ {issue_links}"));
	}

	let related_issues = context
		.related_issues
		.iter()
		.filter(|issue| issue.relationship != HostedIssueRelationshipKind::ClosedByReviewRequest)
		.collect::<Vec<_>>();
	if !related_issues.is_empty() {
		let labels = render_issue_labels(&related_issues);
		let issue_links = render_issue_links(&related_issues);
		rendered.related_issues = Some(labels.clone());
		rendered.related_issue_links = Some(issue_links.clone());
		lines.push(format!("> _Related issues:_ {issue_links}"));
	}

	rendered.context = lines.join("\n");
	rendered
}

fn render_actor_label(actor: &HostedActorRef) -> String {
	if let Some(login) = actor.login.as_deref() {
		format!("@{login}")
	} else if let Some(display_name) = actor.display_name.as_deref() {
		display_name.to_string()
	} else {
		"unknown".to_string()
	}
}

fn render_review_request_label(review_request: &HostedReviewRequestRef) -> String {
	match review_request.kind {
		monochange_core::HostedReviewRequestKind::PullRequest => {
			format!("PR {}", review_request.id)
		}
		monochange_core::HostedReviewRequestKind::MergeRequest => {
			format!("MR {}", review_request.id)
		}
	}
}

fn render_markdown_link(label: &str, url: Option<&str>) -> String {
	url.map_or_else(|| label.to_string(), |url| format!("[{label}]({url})"))
}

fn render_issue_labels(issues: &[&HostedIssueRef]) -> String {
	issues
		.iter()
		.map(|issue| issue.id.clone())
		.collect::<Vec<_>>()
		.join(", ")
}

fn render_issue_links(issues: &[&HostedIssueRef]) -> String {
	issues
		.iter()
		.map(|issue| render_markdown_link(&issue.id, issue.url.as_deref()))
		.collect::<Vec<_>>()
		.join(", ")
}

fn render_package_empty_update_message(
	configuration: &monochange_core::WorkspaceConfiguration,
	package_definition: Option<&monochange_core::PackageDefinition>,
	group_definition: Option<&monochange_core::GroupDefinition>,
	package: &PackageRecord,
	decision: &monochange_core::ReleaseDecision,
	planned_version: &str,
) -> String {
	let template = select_empty_update_message(
		package_definition.and_then(|definition| definition.empty_update_message.as_deref()),
		group_definition.and_then(|definition| definition.empty_update_message.as_deref()),
		configuration.defaults.empty_update_message.as_deref(),
		if group_definition.is_some() {
			"No package-specific changes were recorded; `{{ package }}` was updated to {{ version }} as part of group `{{ group }}`."
		} else {
			"No package-specific changes were recorded; `{{ package }}` was updated to {{ version }}."
		},
	);
	let mut metadata = BTreeMap::new();
	metadata.insert("package", package.name.clone());
	metadata.insert("package_name", package.name.clone());
	metadata.insert("package_id", decision.package_id.clone());
	metadata.insert("group", decision.group_id.clone().unwrap_or_default());
	metadata.insert("group_name", decision.group_id.clone().unwrap_or_default());
	metadata.insert("group_id", decision.group_id.clone().unwrap_or_default());
	metadata.insert("version", planned_version.to_string());
	metadata.insert("new_version", planned_version.to_string());
	metadata.insert(
		"previous_version",
		package
			.current_version
			.as_ref()
			.map_or_else(String::new, ToString::to_string),
	);
	metadata.insert(
		"current_version",
		package
			.current_version
			.as_ref()
			.map_or_else(String::new, ToString::to_string),
	);
	metadata.insert("bump", decision.recommended_bump.to_string());
	metadata.insert("trigger", decision.trigger_type.clone());
	metadata.insert("ecosystem", package.ecosystem.to_string());
	metadata.insert(
		"release_owner",
		decision
			.group_id
			.clone()
			.unwrap_or_else(|| decision.package_id.clone()),
	);
	metadata.insert(
		"release_owner_kind",
		if decision.group_id.is_some() {
			"group".to_string()
		} else {
			"package".to_string()
		},
	);
	metadata.insert("reasons", decision.reasons.join("; "));
	render_message_template(template, &metadata)
}

fn render_group_empty_update_message(
	configuration: &monochange_core::WorkspaceConfiguration,
	group_definition: Option<&monochange_core::GroupDefinition>,
	planned_group: &monochange_core::PlannedVersionGroup,
	planned_version: &str,
	packages: &[PackageRecord],
) -> String {
	let template = select_empty_update_message(
		group_definition.and_then(|definition| definition.empty_update_message.as_deref()),
		None,
		configuration.defaults.empty_update_message.as_deref(),
		"No package-specific changes were recorded; group `{{ group }}` was updated to {{ version }}.",
	);
	let previous_version = planned_group.members.iter().find_map(|member_id| {
		packages
			.iter()
			.find(|package| package.id == *member_id)
			.and_then(|package| package.current_version.as_ref())
			.map(ToString::to_string)
	});
	let mut metadata = BTreeMap::new();
	metadata.insert("group", planned_group.group_id.clone());
	metadata.insert("group_name", planned_group.group_id.clone());
	metadata.insert("group_id", planned_group.group_id.clone());
	metadata.insert("version", planned_version.to_string());
	metadata.insert("new_version", planned_version.to_string());
	metadata.insert(
		"previous_version",
		previous_version.clone().unwrap_or_default(),
	);
	metadata.insert("current_version", previous_version.unwrap_or_default());
	metadata.insert("bump", planned_group.recommended_bump.to_string());
	metadata.insert("members", planned_group.members.join(", "));
	metadata.insert("member_count", planned_group.members.len().to_string());
	metadata.insert("release_owner", planned_group.group_id.clone());
	metadata.insert("release_owner_kind", "group".to_string());
	render_message_template(template, &metadata)
}

fn select_empty_update_message<'value>(
	primary: Option<&'value str>,
	secondary: Option<&'value str>,
	default_value: Option<&'value str>,
	built_in_default: &'value str,
) -> &'value str {
	primary
		.filter(|message| !message.trim().is_empty())
		.or_else(|| secondary.filter(|message| !message.trim().is_empty()))
		.or_else(|| default_value.filter(|message| !message.trim().is_empty()))
		.unwrap_or(built_in_default)
}

fn render_jinja_template(template: &str, context: &minijinja::Value) -> MonochangeResult<String> {
	render_jinja_template_with_behavior(template, context, UndefinedBehavior::Lenient)
}

fn render_jinja_template_strict(
	template: &str,
	context: &minijinja::Value,
) -> MonochangeResult<String> {
	render_jinja_template_with_behavior(template, context, UndefinedBehavior::Strict)
}

fn render_jinja_template_with_behavior(
	template: &str,
	context: &minijinja::Value,
	undefined_behavior: UndefinedBehavior,
) -> MonochangeResult<String> {
	let mut env = Environment::new();
	env.set_undefined_behavior(undefined_behavior);
	let rendered = env
		.render_str(template, context)
		.map_err(|error| MonochangeError::Config(format!("template rendering failed: {error}")))?;
	Ok(rendered)
}

fn render_message_template(template: &str, metadata: &BTreeMap<&str, String>) -> String {
	let context = minijinja::Value::from_serialize(metadata);
	render_jinja_template(template, &context).unwrap_or_else(|_| template.to_string())
}

fn package_release_note_changes(
	configuration: &monochange_core::WorkspaceConfiguration,
	package_definition: Option<&monochange_core::PackageDefinition>,
	group_definition: Option<&monochange_core::GroupDefinition>,
	decision: &monochange_core::ReleaseDecision,
	package: &PackageRecord,
	direct_changes: Option<&Vec<ReleaseNoteChange>>,
	planned_version: &str,
) -> Vec<ReleaseNoteChange> {
	let mut changes = direct_changes.cloned().unwrap_or_default();
	if changes.is_empty() {
		changes.push(ReleaseNoteChange {
			package_id: decision.package_id.clone(),
			package_name: config_package_id(package),
			package_labels: Vec::new(),
			source_path: None,
			summary: render_package_empty_update_message(
				configuration,
				package_definition,
				group_definition,
				package,
				decision,
				planned_version,
			),
			details: None,
			bump: decision.recommended_bump,
			change_type: None,
			context: None,
			changeset_path: None,
			change_owner: None,
			change_owner_link: None,
			review_request: None,
			review_request_link: None,
			introduced_commit: None,
			introduced_commit_link: None,
			last_updated_commit: None,
			last_updated_commit_link: None,
			related_issues: None,
			related_issue_links: None,
			closed_issues: None,
			closed_issue_links: None,
		});
	}
	changes
}

fn group_release_note_changes(
	configuration: &monochange_core::WorkspaceConfiguration,
	group_definition: Option<&monochange_core::GroupDefinition>,
	planned_group: &monochange_core::PlannedVersionGroup,
	release_note_changes: &BTreeMap<String, Vec<ReleaseNoteChange>>,
	changeset_targets_by_path: &BTreeMap<PathBuf, Vec<PreparedChangesetTarget>>,
	packages: &[PackageRecord],
	planned_version: &str,
) -> Vec<ReleaseNoteChange> {
	let unfiltered_changes = planned_group
		.members
		.iter()
		.flat_map(|member_id| {
			release_note_changes
				.get(member_id)
				.into_iter()
				.flatten()
				.cloned()
		})
		.collect::<Vec<_>>();
	let mut changes = unfiltered_changes
		.iter()
		.filter_map(|change| {
			filter_group_release_note_change(
				change,
				group_definition,
				planned_group,
				changeset_targets_by_path,
			)
		})
		.collect::<Vec<_>>();
	if changes.is_empty() {
		let summary = if unfiltered_changes.is_empty() {
			render_group_empty_update_message(
				configuration,
				group_definition,
				planned_group,
				planned_version,
				packages,
			)
		} else {
			render_group_filtered_update_message(&planned_group.group_id)
		};
		changes.push(ReleaseNoteChange {
			package_id: planned_group.group_id.clone(),
			package_name: planned_group.group_id.clone(),
			package_labels: Vec::new(),
			source_path: None,
			summary,
			details: None,
			bump: planned_group.recommended_bump,
			change_type: None,
			context: None,
			changeset_path: None,
			change_owner: None,
			change_owner_link: None,
			review_request: None,
			review_request_link: None,
			introduced_commit: None,
			introduced_commit_link: None,
			last_updated_commit: None,
			last_updated_commit_link: None,
			related_issues: None,
			related_issue_links: None,
			closed_issues: None,
			closed_issue_links: None,
		});
	} else {
		changes = aggregate_group_release_note_changes(changes);
	}
	changes
}

fn filter_group_release_note_change(
	change: &ReleaseNoteChange,
	group_definition: Option<&monochange_core::GroupDefinition>,
	planned_group: &monochange_core::PlannedVersionGroup,
	changeset_targets_by_path: &BTreeMap<PathBuf, Vec<PreparedChangesetTarget>>,
) -> Option<ReleaseNoteChange> {
	let source_path = change.source_path.as_ref().map(PathBuf::from)?;
	let targets = changeset_targets_by_path.get(&source_path)?;
	if targets.iter().any(|target| {
		target.kind == ChangesetTargetKind::Group && target.id == planned_group.group_id
	}) {
		let mut change = change.clone();
		change.package_name.clone_from(&planned_group.group_id);
		return Some(change);
	}
	let in_group_targets = targets
		.iter()
		.filter(|target| {
			target.kind == ChangesetTargetKind::Package
				&& group_definition
					.is_some_and(|group| group.packages.iter().any(|member| member == &target.id))
		})
		.map(|target| target.id.clone())
		.collect::<BTreeSet<_>>();
	if in_group_targets.is_empty() {
		return None;
	}
	let default_include = GroupChangelogInclude::All;
	let include = group_definition.map_or(&default_include, |group| &group.changelog_include);
	if group_changelog_include_allows(include, &in_group_targets) {
		Some(change.clone())
	} else {
		None
	}
}

fn group_changelog_include_allows(
	include: &GroupChangelogInclude,
	in_group_targets: &BTreeSet<String>,
) -> bool {
	match include {
		GroupChangelogInclude::All => true,
		GroupChangelogInclude::GroupOnly => false,
		GroupChangelogInclude::Selected(selected) => in_group_targets
			.iter()
			.all(|package_id| selected.contains(package_id)),
	}
}

fn render_group_filtered_update_message(group_id: &str) -> String {
	format!(
		"No group-facing notes were recorded for this release. Member packages were updated as part of the synchronized group `{group_id}` version, but their changes are not configured for inclusion in this changelog."
	)
}

fn aggregate_group_release_note_changes(changes: Vec<ReleaseNoteChange>) -> Vec<ReleaseNoteChange> {
	let mut aggregated = Vec::<ReleaseNoteChange>::new();
	let mut indexes = BTreeMap::<GroupReleaseNoteKey, usize>::new();
	for change in changes {
		let key = GroupReleaseNoteKey {
			source_path: change.source_path.clone(),
			summary: change.summary.clone(),
			details: change.details.clone(),
			bump: change.bump,
			change_type: change.change_type.clone(),
			context: change.context.clone(),
		};
		if let Some(index) = indexes.get(&key).copied() {
			let entry = &mut aggregated[index];
			if !entry
				.package_labels
				.iter()
				.any(|label| label == &change.package_name)
			{
				entry.package_labels.push(change.package_name.clone());
				entry.package_name = entry.package_labels.join(", ");
			}
			continue;
		}
		let mut change = change;
		change.package_labels = vec![change.package_name.clone()];
		change.package_name = change.package_labels.join(", ");
		indexes.insert(key, aggregated.len());
		aggregated.push(change);
	}
	aggregated
}

fn group_changed_members(
	planned_group: &monochange_core::PlannedVersionGroup,
	release_note_changes: &BTreeMap<String, Vec<ReleaseNoteChange>>,
	packages: &[PackageRecord],
) -> BTreeSet<String> {
	planned_group
		.members
		.iter()
		.filter(|member_id| {
			release_note_changes
				.get(*member_id)
				.is_some_and(|changes| !changes.is_empty())
		})
		.filter_map(|member_id| {
			packages
				.iter()
				.find(|package| package.id == *member_id)
				.map(config_package_id)
		})
		.collect()
}

fn group_release_summary(
	group_name: &str,
	members: &[String],
	changed_members: &BTreeSet<String>,
) -> Vec<String> {
	let mut summary = vec![format!("Grouped release for `{group_name}`.")];
	if members.is_empty() {
		return summary;
	}
	if changed_members.is_empty() {
		summary.push(format!("Members: {}", members.join(", ")));
		return summary;
	}
	let changed = members
		.iter()
		.filter(|member| changed_members.contains(member.as_str()))
		.cloned()
		.collect::<Vec<_>>();
	let synchronized = members
		.iter()
		.filter(|member| !changed_members.contains(member.as_str()))
		.cloned()
		.collect::<Vec<_>>();
	if !changed.is_empty() {
		summary.push(format!("Changed members: {}", changed.join(", ")));
	}
	if !synchronized.is_empty() {
		summary.push(format!("Synchronized members: {}", synchronized.join(", ")));
	}
	summary
}

fn build_release_notes_document(
	target_id: &str,
	version: &str,
	summary: Vec<String>,
	extra_sections: &[ExtraChangelogSection],
	change_templates: &[String],
	changes: &[ReleaseNoteChange],
) -> ReleaseNotesDocument {
	ReleaseNotesDocument {
		title: version.to_string(),
		summary,
		sections: render_release_note_sections(
			target_id,
			version,
			extra_sections,
			change_templates,
			changes,
		),
	}
}

fn render_release_note_sections(
	target_id: &str,
	version: &str,
	extra_sections: &[ExtraChangelogSection],
	change_templates: &[String],
	changes: &[ReleaseNoteChange],
) -> Vec<ReleaseNotesSection> {
	let overridden_builtins = extra_sections
		.iter()
		.flat_map(|section| {
			section
				.types
				.iter()
				.map(|change_type| change_type.trim().to_string())
		})
		.collect::<BTreeSet<_>>();
	let resolved_extra_sections = extra_sections
		.iter()
		.map(|section| ResolvedSectionDefinition {
			title: section.name.clone(),
			types: section.types.clone(),
		})
		.collect::<Vec<_>>();
	let mut builtin_entries = BTreeMap::<BuiltinReleaseSection, Vec<String>>::new();
	let mut extra_entries = vec![Vec::<String>::new(); resolved_extra_sections.len()];

	for change in changes {
		let rendered = render_change_entry(change, target_id, version, change_templates);
		match classify_release_note_change(change, &resolved_extra_sections) {
			ResolvedReleaseSectionTarget::Builtin(section) => {
				push_unique_release_note_entry(
					builtin_entries.entry(section).or_default(),
					rendered,
				);
			}
			ResolvedReleaseSectionTarget::Extra(index) => {
				push_unique_release_note_entry(&mut extra_entries[index], rendered);
			}
		}
	}

	let mut sections = Vec::new();
	for builtin in builtin_release_sections() {
		if overridden_builtins.contains(builtin.selector()) {
			continue;
		}
		if let Some(entries) = builtin_entries
			.remove(&builtin)
			.filter(|entries| !entries.is_empty())
		{
			sections.push(ReleaseNotesSection {
				title: builtin.title().to_string(),
				entries,
			});
		}
	}
	for (index, section) in resolved_extra_sections.iter().enumerate() {
		if extra_entries[index].is_empty() {
			continue;
		}
		sections.push(ReleaseNotesSection {
			title: section.title.clone(),
			entries: extra_entries[index].clone(),
		});
	}
	if sections.is_empty() {
		sections.push(ReleaseNotesSection {
			title: "Changed".to_string(),
			entries: vec!["- prepare release".to_string()],
		});
	}
	sections
}

#[allow(variant_size_differences)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ResolvedReleaseSectionTarget {
	Builtin(BuiltinReleaseSection),
	Extra(usize),
}

fn classify_release_note_change(
	change: &ReleaseNoteChange,
	extra_sections: &[ResolvedSectionDefinition],
) -> ResolvedReleaseSectionTarget {
	if let Some(change_type) = change.change_type.as_deref() {
		if let Some(index) = extra_sections
			.iter()
			.position(|section| section_matches_resolved_type(section, change_type))
		{
			return ResolvedReleaseSectionTarget::Extra(index);
		}
		if change_type == BuiltinReleaseSection::Note.selector() {
			return ResolvedReleaseSectionTarget::Builtin(BuiltinReleaseSection::Note);
		}
	}
	let builtin = BuiltinReleaseSection::from_bump(change.bump);
	if let Some(index) = extra_sections
		.iter()
		.position(|section| section_matches_resolved_type(section, builtin.selector()))
	{
		return ResolvedReleaseSectionTarget::Extra(index);
	}
	ResolvedReleaseSectionTarget::Builtin(builtin)
}

fn section_matches_resolved_type(section: &ResolvedSectionDefinition, change_type: &str) -> bool {
	section
		.types
		.iter()
		.any(|candidate| candidate.trim() == change_type)
}

fn render_change_entry(
	change: &ReleaseNoteChange,
	target_id: &str,
	version: &str,
	change_templates: &[String],
) -> String {
	for template in change_templates
		.iter()
		.map(String::as_str)
		.chain(DEFAULT_CHANGE_TEMPLATES)
	{
		if let Some(rendered) = apply_change_template(template, change, target_id, version) {
			return format_group_labeled_entry(change, &rendered);
		}
	}
	format_group_labeled_entry(change, &format!("- {}", change.summary))
}

fn format_group_labeled_entry(change: &ReleaseNoteChange, rendered: &str) -> String {
	if change.package_labels.is_empty() {
		return rendered.to_string();
	}
	if change.package_labels.len() == 1 && !rendered.contains('\n') {
		if let Some(entry) = rendered.strip_prefix("- ") {
			return format!("- **{}**: {}", change.package_labels[0], entry);
		}
	}
	let labels = change
		.package_labels
		.iter()
		.map(|package| format!("*{package}*"))
		.collect::<Vec<_>>()
		.join(", ");
	format!("> [!NOTE]\n> {labels}\n\n{rendered}")
}

const DEFAULT_CHANGE_TEMPLATES: [&str; 3] = [
	"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ details }}",
	"- {{ summary }}",
];

fn apply_change_template(
	template: &str,
	change: &ReleaseNoteChange,
	target_id: &str,
	version: &str,
) -> Option<String> {
	let bump = change.bump.to_string();
	let mut context = BTreeMap::<&str, &str>::new();
	context.insert("summary", &change.summary);
	context.insert("package", &change.package_name);
	context.insert("version", version);
	context.insert("target_id", target_id);
	context.insert("bump", &bump);
	if let Some(value) = change.details.as_deref() {
		context.insert("details", value);
	}
	if let Some(value) = change.change_type.as_deref() {
		context.insert("type", value);
	}
	if let Some(value) = change.context.as_deref() {
		context.insert("context", value);
		context.insert("context", value);
	}
	if let Some(value) = change.changeset_path.as_deref() {
		context.insert("changeset_path", value);
	}
	if let Some(value) = change.change_owner.as_deref() {
		context.insert("change_owner", value);
	}
	if let Some(value) = change.change_owner_link.as_deref() {
		context.insert("change_owner_link", value);
	}
	if let Some(value) = change.review_request.as_deref() {
		context.insert("review_request", value);
	}
	if let Some(value) = change.review_request_link.as_deref() {
		context.insert("review_request_link", value);
	}
	if let Some(value) = change.introduced_commit.as_deref() {
		context.insert("introduced_commit", value);
	}
	if let Some(value) = change.introduced_commit_link.as_deref() {
		context.insert("introduced_commit_link", value);
	}
	if let Some(value) = change.last_updated_commit.as_deref() {
		context.insert("last_updated_commit", value);
	}
	if let Some(value) = change.last_updated_commit_link.as_deref() {
		context.insert("last_updated_commit_link", value);
	}
	if let Some(value) = change.related_issues.as_deref() {
		context.insert("related_issues", value);
	}
	if let Some(value) = change.related_issue_links.as_deref() {
		context.insert("related_issue_links", value);
	}
	if let Some(value) = change.closed_issues.as_deref() {
		context.insert("closed_issues", value);
	}
	if let Some(value) = change.closed_issue_links.as_deref() {
		context.insert("closed_issue_links", value);
	}
	let jinja_context = minijinja::Value::from_serialize(&context);
	let rendered = render_jinja_template_strict(template, &jinja_context).ok()?;
	let rendered = rendered.trim().to_string();
	if rendered.is_empty() {
		None
	} else {
		Some(rendered)
	}
}

fn push_unique_release_note_entry(entries: &mut Vec<String>, entry: String) {
	if !entries.iter().any(|existing| existing == &entry) {
		entries.push(entry);
	}
}

fn config_package_id(package: &PackageRecord) -> String {
	package
		.metadata
		.get("config_id")
		.cloned()
		.unwrap_or_else(|| package.name.clone())
}

impl BuiltinReleaseSection {
	fn from_bump(bump: BumpSeverity) -> Self {
		match bump {
			BumpSeverity::Major => Self::Major,
			BumpSeverity::Minor => Self::Minor,
			BumpSeverity::None | BumpSeverity::Patch => Self::Patch,
		}
	}

	fn selector(self) -> &'static str {
		match self {
			Self::Major => "major",
			Self::Minor => "minor",
			Self::Patch => "patch",
			Self::Note => "note",
		}
	}

	fn title(self) -> &'static str {
		match self {
			Self::Major => "Breaking changes",
			Self::Minor => "Features",
			Self::Patch => "Fixes",
			Self::Note => "Notes",
		}
	}
}

fn builtin_release_sections() -> [BuiltinReleaseSection; 4] {
	[
		BuiltinReleaseSection::Major,
		BuiltinReleaseSection::Minor,
		BuiltinReleaseSection::Patch,
		BuiltinReleaseSection::Note,
	]
}

struct VersionedFileUpdateContext<'a> {
	package_by_record_id: BTreeMap<&'a str, &'a PackageRecord>,
	released_versions_by_native_name: BTreeMap<String, String>,
	configuration: &'a monochange_core::WorkspaceConfiguration,
}

#[derive(Debug)]
enum CachedDocument {
	Toml(Value),
	Json(serde_json::Value),
	Yaml(serde_yaml_ng::Mapping),
	Text(String),
	Bytes(Vec<u8>),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum VersionedFileKind {
	Cargo(monochange_cargo::CargoVersionedFileKind),
	Npm(monochange_npm::NpmVersionedFileKind),
	Deno(monochange_deno::DenoVersionedFileKind),
	Dart(monochange_dart::DartVersionedFileKind),
}

fn versioned_file_kind(
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
				package_definition.id.as_str(),
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
				group_definition.id.as_str(),
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

fn serialize_cached_document(
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

fn read_cached_document(
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

fn update_json_dependency_fields(
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

fn resolve_versioned_prefix(
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

#[allow(clippy::too_many_arguments)]
fn apply_versioned_file_definition(
	root: &Path,
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
	definition: &VersionedFileDefinition,
	_owner_id: &str,
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
	changeset_paths: &[PathBuf],
) -> Vec<ReleaseTarget> {
	let changes_count = changeset_paths.len();
	let source = configuration.source.as_ref();
	let defaults_release_title = configuration.defaults.release_title.as_deref();
	let defaults_changelog_title = configuration.defaults.changelog_version_title.as_deref();

	let mut release_targets = configuration
		.groups
		.iter()
		.filter_map(|group| {
			plan.groups
				.iter()
				.find(|pg| pg.group_id == group.id && pg.recommended_bump.is_release())
				.and_then(|pg| {
					pg.planned_version.as_ref().map(|version| {
						let vs = version.to_string();
						let tag = render_tag_name(&group.id, &vs, group.version_format);
						let prev = find_previous_tag(&configuration.root_path, &tag);
						let ctx = TitleRenderContext::new(
							&group.id,
							&vs,
							changes_count,
							source,
							&tag,
							prev.as_deref(),
						);
						let rt = effective_title_template(
							group.release_title.as_deref(),
							defaults_release_title,
							default_release_title_for_format(group.version_format),
						);
						let ct = effective_title_template(
							group.changelog_version_title.as_deref(),
							defaults_changelog_title,
							default_changelog_version_title_for_format(group.version_format),
						);
						ReleaseTarget {
							id: group.id.clone(),
							kind: ReleaseOwnerKind::Group,
							version: vs,
							tag: group.tag,
							release: group.release,
							version_format: group.version_format,
							tag_name: tag,
							members: group.packages.clone(),
							rendered_title: ctx.render(rt),
							rendered_changelog_title: ctx.render(ct),
						}
					})
				})
		})
		.collect::<Vec<_>>();
	for decision in plan
		.decisions
		.iter()
		.filter(|d| d.recommended_bump.is_release() && d.group_id.is_none())
	{
		let Some(package) = packages.iter().find(|p| p.id == decision.package_id) else {
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
		let vs = version.to_string();
		let tag = render_tag_name(&identity.owner_id, &vs, identity.version_format);
		let prev = find_previous_tag(&configuration.root_path, &tag);
		let pkg_def = configuration.package_by_id(&config_id);
		let ctx = TitleRenderContext::new(
			&identity.owner_id,
			&vs,
			changes_count,
			source,
			&tag,
			prev.as_deref(),
		);
		let rt = effective_title_template(
			pkg_def.and_then(|p| p.release_title.as_deref()),
			defaults_release_title,
			default_release_title_for_format(identity.version_format),
		);
		let ct = effective_title_template(
			pkg_def.and_then(|p| p.changelog_version_title.as_deref()),
			defaults_changelog_title,
			default_changelog_version_title_for_format(identity.version_format),
		);
		release_targets.push(ReleaseTarget {
			id: identity.owner_id.clone(),
			kind: identity.owner_kind,
			version: vs,
			tag: identity.tag,
			release: identity.release,
			version_format: identity.version_format,
			tag_name: tag,
			members: identity.members,
			rendered_title: ctx.render(rt),
			rendered_changelog_title: ctx.render(ct),
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

/// Dispatch tag URL generation to the appropriate provider crate.
fn tag_url_for_provider(source: &SourceConfiguration, tag_name: &str) -> String {
	match source.provider {
		SourceProvider::GitHub => github_provider::tag_url(source, tag_name),
		SourceProvider::GitLab => gitlab_provider::tag_url(source, tag_name),
		SourceProvider::Gitea => gitea_provider::tag_url(source, tag_name),
	}
}

/// Dispatch compare URL generation to the appropriate provider crate.
fn compare_url_for_provider(
	source: &SourceConfiguration,
	previous_tag: &str,
	current_tag: &str,
) -> String {
	match source.provider {
		SourceProvider::GitHub => github_provider::compare_url(source, previous_tag, current_tag),
		SourceProvider::GitLab => gitlab_provider::compare_url(source, previous_tag, current_tag),
		SourceProvider::Gitea => gitea_provider::compare_url(source, previous_tag, current_tag),
	}
}

fn find_previous_tag(root: &Path, current_tag: &str) -> Option<String> {
	let output = std::process::Command::new("git")
		.current_dir(root)
		.args(["tag", "--list", "--sort=-v:refname"])
		.output()
		.ok()?;
	if !output.status.success() {
		return None;
	}
	let tags_text = String::from_utf8_lossy(&output.stdout);
	let all_tags: Vec<&str> = tags_text.lines().map(str::trim).collect();
	let (prefix, current_version) = parse_tag_prefix_and_version(current_tag)?;
	all_tags
		.into_iter()
		.filter(|tag| *tag != current_tag)
		.filter_map(|tag| {
			let (p, v) = parse_tag_prefix_and_version(tag)?;
			(p == prefix && v < current_version).then(|| (tag.to_string(), v))
		})
		.max_by(|a, b| a.1.cmp(&b.1))
		.map(|(tag, _)| tag)
}

fn parse_tag_prefix_and_version(tag: &str) -> Option<(String, semver::Version)> {
	let v_pos = tag.rfind('v')?;
	let prefix = &tag[..=v_pos];
	let version_str = &tag[v_pos + 1..];
	let version = semver::Version::parse(version_str).ok()?;
	Some((prefix.to_string(), version))
}

struct TitleRenderContext {
	id: String,
	version: String,
	previous_version: String,
	date: String,
	time: String,
	datetime: String,
	changes_count: usize,
	tag_url: String,
	compare_url: String,
}

impl TitleRenderContext {
	fn new(
		id: &str,
		version: &str,
		changes_count: usize,
		source: Option<&SourceConfiguration>,
		tag_name: &str,
		previous_tag_name: Option<&str>,
	) -> Self {
		let now = resolve_release_datetime();
		let date = now.format("%Y-%m-%d").to_string();
		let time = now.format("%H:%M:%S").to_string();
		let datetime = now.format("%Y-%m-%dT%H:%M:%S").to_string();
		let tag_url = source
			.map(|s| tag_url_for_provider(s, tag_name))
			.unwrap_or_default();
		let compare_url = match (source, previous_tag_name) {
			(Some(s), Some(prev)) => compare_url_for_provider(s, prev, tag_name),
			_ => tag_url.clone(),
		};
		// Extract the bare semver string from the previous tag (e.g. "pkg/v1.1.0" → "1.1.0").
		let previous_version = previous_tag_name
			.and_then(|t| parse_tag_prefix_and_version(t).map(|(_, v)| v.to_string()))
			.unwrap_or_default();
		Self {
			id: id.to_string(),
			version: version.to_string(),
			previous_version,
			date,
			time,
			datetime,
			changes_count,
			tag_url,
			compare_url,
		}
	}

	fn render(&self, template: &str) -> String {
		let context = minijinja::context! {
			id => &self.id,
			version => &self.version,
			previous_version => &self.previous_version,
			date => &self.date,
			time => &self.time,
			datetime => &self.datetime,
			changes_count => self.changes_count,
			tag_url => &self.tag_url,
			compare_url => &self.compare_url,
		};
		let jinja_value = minijinja::Value::from_serialize(&context);
		render_jinja_template(template, &jinja_value).unwrap_or_else(|_| self.version.clone())
	}
}

fn resolve_release_datetime() -> chrono::NaiveDateTime {
	use chrono::NaiveDate;
	use chrono::NaiveDateTime;

	if let Ok(env_date) = std::env::var("MONOCHANGE_RELEASE_DATE") {
		if let Ok(ndt) = NaiveDateTime::parse_from_str(&env_date, "%Y-%m-%dT%H:%M:%S") {
			return ndt;
		}
		if let Ok(nd) = NaiveDate::parse_from_str(&env_date, "%Y-%m-%d") {
			return nd.and_hms_opt(0, 0, 0).unwrap_or_default();
		}
	}
	chrono::Local::now().naive_local()
}

fn effective_title_template<'a>(
	specific: Option<&'a str>,
	defaults: Option<&'a str>,
	builtin: &'a str,
) -> &'a str {
	specific.or(defaults).unwrap_or(builtin)
}

fn default_release_title_for_format(version_format: VersionFormat) -> &'static str {
	match version_format {
		VersionFormat::Primary => DEFAULT_RELEASE_TITLE_PRIMARY,
		VersionFormat::Namespaced => DEFAULT_RELEASE_TITLE_NAMESPACED,
	}
}

fn default_changelog_version_title_for_format(version_format: VersionFormat) -> &'static str {
	match version_format {
		VersionFormat::Primary => DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY,
		VersionFormat::Namespaced => DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED,
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
				.map(|content| FileUpdate {
					path,
					content: content.into_bytes(),
				})
				.map_err(|error| MonochangeError::Config(error.to_string()))
		})
		.collect()
}

fn build_npm_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	let released_versions = released_versions_by_record_id(plan);
	let mut updates = Vec::new();
	for package in packages
		.iter()
		.filter(|package| package.ecosystem == Ecosystem::Npm)
	{
		let Some(version) = released_versions.get(&package.id) else {
			continue;
		};
		let contents = fs::read_to_string(&package.manifest_path).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read {}: {error}",
				package.manifest_path.display()
			))
		})?;
		let mut parsed = serde_json::from_str::<serde_json::Value>(&contents).map_err(|error| {
			MonochangeError::Config(format!(
				"failed to parse {}: {error}",
				package.manifest_path.display()
			))
		})?;
		if let Some(obj) = parsed.as_object_mut() {
			obj.insert(
				"version".to_string(),
				serde_json::Value::String(version.clone()),
			);
		}
		let mut rendered = serde_json::to_string_pretty(&parsed)
			.map_err(|error| MonochangeError::Config(error.to_string()))?;
		rendered.push('\n');
		updates.push(FileUpdate {
			path: package.manifest_path.clone(),
			content: rendered.into_bytes(),
		});
	}
	Ok(updates)
}

fn build_dart_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	let released_versions = released_versions_by_record_id(plan);
	let mut updates = Vec::new();
	for package in packages.iter().filter(|package| {
		package.ecosystem == Ecosystem::Dart || package.ecosystem == Ecosystem::Flutter
	}) {
		let Some(version) = released_versions.get(&package.id) else {
			continue;
		};
		let contents = fs::read_to_string(&package.manifest_path).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read {}: {error}",
				package.manifest_path.display()
			))
		})?;
		let mut mapping =
			serde_yaml_ng::from_str::<serde_yaml_ng::Mapping>(&contents).map_err(|error| {
				MonochangeError::Config(format!(
					"failed to parse {}: {error}",
					package.manifest_path.display()
				))
			})?;
		mapping.insert(
			serde_yaml_ng::Value::String("version".to_string()),
			serde_yaml_ng::Value::String(version.clone()),
		);
		let rendered = serde_yaml_ng::to_string(&mapping)
			.map_err(|error| MonochangeError::Config(error.to_string()))?;
		updates.push(FileUpdate {
			path: package.manifest_path.clone(),
			content: rendered.into_bytes(),
		});
	}
	Ok(updates)
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
	cli_command: &CliCommandDefinition,
	prepared_release: &PreparedRelease,
	_command_logs: &[String],
) -> ReleaseManifest {
	ReleaseManifest {
		command: cli_command.name.clone(),
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
				rendered_title: target.rendered_title.clone(),
				rendered_changelog_title: target.rendered_changelog_title.clone(),
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
		changesets: prepared_release.changesets.clone(),
		deleted_changesets: prepared_release.deleted_changesets.clone(),
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

fn build_release_record(
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> ReleaseRecord {
	ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: resolve_release_datetime()
			.and_utc()
			.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
		command: manifest.command.clone(),
		version: manifest.version.clone(),
		group_version: manifest.group_version.clone(),
		release_targets: manifest
			.release_targets
			.iter()
			.map(|target| ReleaseRecordTarget {
				id: target.id.clone(),
				kind: target.kind,
				version: target.version.clone(),
				version_format: target.version_format,
				tag: target.tag,
				release: target.release,
				tag_name: target.tag_name.clone(),
				members: target.members.clone(),
			})
			.collect(),
		released_packages: manifest.released_packages.clone(),
		changed_files: manifest.changed_files.clone(),
		updated_changelogs: manifest
			.changelogs
			.iter()
			.map(|changelog| changelog.path.clone())
			.collect(),
		deleted_changesets: manifest.deleted_changesets.clone(),
		provider: source.map(|source| ReleaseRecordProvider {
			kind: source.provider,
			owner: source.owner.clone(),
			repo: source.repo.clone(),
			host: source.host.clone(),
		}),
	}
}

fn build_release_commit_message(
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> CommitMessage {
	CommitMessage {
		subject: source.map_or_else(
			|| monochange_core::ChangeRequestSettings::default().title,
			|source| source.pull_requests.title.clone(),
		),
		body: Some(render_release_commit_body(source, manifest)),
	}
}

fn render_release_commit_body(
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> String {
	let mut lines = vec!["Prepare release.".to_string()];
	if !manifest.release_targets.is_empty() {
		lines.push(String::new());
		lines.push(format!(
			"- release targets: {}",
			manifest
				.release_targets
				.iter()
				.map(|target| format!("{} ({})", target.id, target.version))
				.collect::<Vec<_>>()
				.join(", ")
		));
	}
	if !manifest.released_packages.is_empty() {
		lines.push(format!(
			"- released packages: {}",
			manifest.released_packages.join(", ")
		));
	}
	if !manifest.changelogs.is_empty() {
		lines.push(format!(
			"- updated changelogs: {}",
			manifest
				.changelogs
				.iter()
				.map(|changelog| changelog.path.display().to_string())
				.collect::<Vec<_>>()
				.join(", ")
		));
	}
	if !manifest.deleted_changesets.is_empty() {
		lines.push(format!(
			"- deleted changesets: {}",
			manifest
				.deleted_changesets
				.iter()
				.map(|path| path.display().to_string())
				.collect::<Vec<_>>()
				.join(", ")
		));
	}
	let release_record = build_release_record(source, manifest);
	let release_record_block = render_release_record_block(&release_record)
		.unwrap_or_else(|error| panic!("release record generation bug: {error}"));
	format!("{}\n\n{}", lines.join("\n"), release_record_block)
}

fn render_release_manifest_json(manifest: &ReleaseManifest) -> MonochangeResult<String> {
	serde_json::to_string_pretty(manifest)
		.map_err(|error| MonochangeError::Discovery(error.to_string()))
}

fn build_source_release_requests(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> Vec<SourceReleaseRequest> {
	match source.provider {
		SourceProvider::GitHub => github_provider::build_release_requests(source, manifest),
		SourceProvider::GitLab => gitlab_provider::build_release_requests(source, manifest),
		SourceProvider::Gitea => gitea_provider::build_release_requests(source, manifest),
	}
}

fn build_source_change_request(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> SourceChangeRequest {
	let mut request = match source.provider {
		SourceProvider::GitHub => {
			github_provider::build_release_pull_request_request(source, manifest)
		}
		SourceProvider::GitLab => {
			gitlab_provider::build_release_pull_request_request(source, manifest)
		}
		SourceProvider::Gitea => {
			gitea_provider::build_release_pull_request_request(source, manifest)
		}
	};
	request.commit_message = build_release_commit_message(Some(source), manifest);
	request
}

fn publish_source_release_requests(
	source: &SourceConfiguration,
	requests: &[SourceReleaseRequest],
) -> MonochangeResult<Vec<SourceReleaseOutcome>> {
	match source.provider {
		SourceProvider::GitHub => github_provider::publish_release_requests(source, requests),
		SourceProvider::GitLab => gitlab_provider::publish_release_requests(source, requests),
		SourceProvider::Gitea => gitea_provider::publish_release_requests(source, requests),
	}
}

fn publish_source_change_request(
	source: &SourceConfiguration,
	root: &Path,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<SourceChangeRequestOutcome> {
	match source.provider {
		SourceProvider::GitHub => {
			github_provider::publish_release_pull_request(source, root, request, tracked_paths)
		}
		SourceProvider::GitLab => {
			gitlab_provider::publish_release_pull_request(source, root, request, tracked_paths)
		}
		SourceProvider::Gitea => {
			gitea_provider::publish_release_pull_request(source, root, request, tracked_paths)
		}
	}
}

fn format_source_operation(operation: &SourceReleaseOperation) -> &'static str {
	match operation {
		SourceReleaseOperation::Created => "created",
		SourceReleaseOperation::Updated => "updated",
	}
}

fn format_change_request_operation(operation: &SourceChangeRequestOperation) -> &'static str {
	match operation {
		SourceChangeRequestOperation::Created => "created",
		SourceChangeRequestOperation::Updated => "updated",
	}
}

fn render_release_cli_command_json(
	manifest: &ReleaseManifest,
	releases: &[SourceReleaseRequest],
	release_request: Option<&SourceChangeRequest>,
	issue_comments: &[github_provider::GitHubIssueCommentPlan],
	release_commit: Option<&CommitReleaseReport>,
) -> MonochangeResult<String> {
	if releases.is_empty()
		&& release_request.is_none()
		&& issue_comments.is_empty()
		&& release_commit.is_none()
	{
		return render_release_manifest_json(manifest);
	}
	serde_json::to_string_pretty(&json!({
		"manifest": manifest,
		"releaseCommit": release_commit,
		"releases": releases,
		"releaseRequest": release_request,
		"issueComments": issue_comments,
	}))
	.map_err(|error| MonochangeError::Discovery(error.to_string()))
}

fn commit_release(
	root: &Path,
	context: &CliContext,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> MonochangeResult<CommitReleaseReport> {
	let tracked_paths = tracked_release_pull_request_paths(context, manifest);
	let message = build_release_commit_message(source, manifest);
	if !context.dry_run {
		git_stage_paths(root, &tracked_paths)?;
		git_commit_paths(root, &message)?;
	}
	Ok(CommitReleaseReport {
		subject: message.subject,
		body: message.body.unwrap_or_default(),
		commit: if context.dry_run {
			None
		} else {
			Some(git_head_commit(root)?)
		},
		tracked_paths,
		dry_run: context.dry_run,
		status: if context.dry_run {
			"dry_run".to_string()
		} else {
			"completed".to_string()
		},
	})
}

fn tracked_release_pull_request_paths(
	context: &CliContext,
	manifest: &ReleaseManifest,
) -> Vec<PathBuf> {
	let mut tracked_paths = manifest.changed_files.clone();
	tracked_paths.extend(manifest.deleted_changesets.clone());
	if let Some(path) = &context.release_manifest_path {
		tracked_paths.push(path.clone());
	}
	tracked_paths.sort();
	tracked_paths.dedup();
	tracked_paths
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
	configuration: &monochange_core::WorkspaceConfiguration,
	package_refs: &[String],
	bump: BumpSeverity,
	version: Option<&str>,
	reason: &str,
	change_type: Option<&str>,
	details: Option<&str>,
) -> MonochangeResult<String> {
	let mut lines = vec!["---".to_string()];
	for package in package_refs {
		lines.extend(render_change_target_markdown(
			configuration,
			package,
			bump,
			version,
			change_type,
		)?);
	}
	lines.push("---".to_string());
	lines.push(String::new());
	lines.push(format!("# {reason}"));
	if let Some(details) = details.filter(|value| !value.trim().is_empty()) {
		lines.push(String::new());
		lines.push(details.trim().to_string());
	}
	lines.push(String::new());
	Ok(lines.join("\n"))
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
