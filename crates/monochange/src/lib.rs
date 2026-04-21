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
//! mc skill -a pi -y
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
//! - start from the built-in default CLI commands and let matching config entries replace them
//! - resolve change input files
//! - render discovery and release command output in text or JSON
//! - execute configured CLI commands plus built-in MCP commands
//! - preview or publish provider releases from prepared release data
//! - evaluate pull-request changeset policy from CI-supplied changed paths and labels
//! - expose JSON-first MCP tools for assistant workflows
//! <!-- {/monochangeCrateDocs} -->

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use analyze::render_analyze_report;
pub(crate) use changelog::*;
pub use changeset_policy::affected_packages;
pub(crate) use changeset_policy::compute_changed_paths_since;
pub use changeset_policy::evaluate_changeset_policy;
pub(crate) use changeset_policy::is_changeset_markdown_path;
pub(crate) use changeset_policy::normalize_changed_path;
pub use changeset_policy::verify_changesets;
pub(crate) use changesets::*;
use clap::ValueEnum;
use clap::error::ErrorKind;
#[cfg(test)]
pub(crate) use cli::apply_runtime_change_type_choices;
#[cfg(test)]
pub(crate) use cli::apply_runtime_prepare_release_markdown_defaults;
#[cfg(test)]
pub(crate) use cli::build_cli_command_subcommand;
pub use cli::build_command;
#[cfg(test)]
pub(crate) use cli::build_command_for_root;
use cli::build_command_with_cli;
#[cfg(test)]
pub(crate) use cli::build_release_record_subcommand;
#[cfg(test)]
pub(crate) use cli::build_skill_subcommand;
#[cfg(test)]
pub(crate) use cli::build_subagents_subcommand;
#[cfg(test)]
pub(crate) use cli::cli_command_after_help;
#[cfg(test)]
use cli::cli_commands_for_root;
use cli::cli_commands_from_config;
#[cfg(test)]
pub(crate) use cli::configured_change_type_choices;
use cli::current_dir_or_dot;
#[cfg(test)]
pub(crate) use cli_runtime::build_cli_template_context;
#[cfg(test)]
pub(crate) use cli_runtime::build_retarget_release_report;
#[cfg(test)]
pub(crate) use cli_runtime::collect_cli_command_inputs;
#[cfg(test)]
pub(crate) use cli_runtime::execute_cli_command;
use cli_runtime::execute_matches;
#[cfg(test)]
pub(crate) use cli_runtime::inferred_retarget_source_configuration;
#[cfg(test)]
pub(crate) use cli_runtime::lookup_template_value;
#[cfg(test)]
pub(crate) use cli_runtime::parse_boolean_step_input;
#[cfg(test)]
pub(crate) use cli_runtime::parse_change_bump;
#[cfg(test)]
pub(crate) use cli_runtime::parse_direct_template_reference;
#[cfg(test)]
pub(crate) use cli_runtime::parse_output_format;
#[cfg(test)]
pub(crate) use cli_runtime::render_cli_command_markdown_result;
#[cfg(test)]
pub(crate) use cli_runtime::render_cli_command_result;
#[cfg(test)]
pub(crate) use cli_runtime::render_retarget_release_report;
#[cfg(test)]
pub(crate) use cli_runtime::retarget_operation_label;
#[cfg(test)]
pub(crate) use cli_runtime::template_value_to_input_values;
use git_support::git_commit_paths;
use git_support::git_head_commit;
use git_support::git_stage_paths;
#[cfg(test)]
pub(crate) use git_support::read_git_commit_message;
#[cfg(test)]
pub(crate) use git_support::run_git_capture;
#[cfg(test)]
pub(crate) use git_support::run_git_process;
#[cfg(test)]
pub(crate) use git_support::run_git_status;
use minijinja::Environment;
use minijinja::UndefinedBehavior;
#[cfg(feature = "cargo")]
use monochange_cargo::RustSemverProvider;
use monochange_config::load_changeset_file;
use monochange_config::load_workspace_configuration;
use monochange_config::resolve_package_reference;
use monochange_config::validate_versioned_files_content;
use monochange_config::validate_workspace;
use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogSettings;
use monochange_core::ChangelogTarget;
use monochange_core::ChangesetContext;
use monochange_core::ChangesetPolicyEvaluation;
use monochange_core::ChangesetRevision;
use monochange_core::ChangesetTargetKind;
use monochange_core::CliCommandDefinition;
use monochange_core::CommitMessage;
use monochange_core::DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED;
use monochange_core::DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY;
use monochange_core::DEFAULT_RELEASE_TITLE_NAMESPACED;
use monochange_core::DEFAULT_RELEASE_TITLE_PRIMARY;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::GroupChangelogInclude;
use monochange_core::HostedActorRef;
use monochange_core::HostedActorSourceKind;
use monochange_core::HostedCommitRef;
use monochange_core::HostedIssueCommentPlan;
use monochange_core::HostedIssueRef;
use monochange_core::HostedIssueRelationshipKind;
use monochange_core::HostedReviewRequestRef;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackagePublicationTarget;
use monochange_core::PackageRecord;
use monochange_core::PreparedChangeset;
use monochange_core::PreparedChangesetTarget;
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
use monochange_core::materialize_dependency_edges;
use monochange_core::relative_to_root;
use monochange_core::render_release_notes;
use monochange_core::render_release_record_block;
#[cfg(feature = "gitea")]
use monochange_gitea as gitea_provider;
#[cfg(feature = "github")]
use monochange_github as github_provider;
#[cfg(feature = "gitlab")]
use monochange_gitlab as gitlab_provider;
use monochange_graph::build_release_plan;
use monochange_semver::CompatibilityProvider;
use monochange_semver::collect_assessments;
pub(crate) use release_artifacts::*;
pub use release_record::discover_release_record;
pub use release_record::execute_release_retarget;
pub use release_record::plan_release_retarget;
use release_record::render_release_record_discovery;
use release_record::render_release_tag_report;
pub use release_record::retarget_release;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use skill::SkillOptions;
use skill::run_skill;
use subagents::SubagentOptions;
use subagents::run_subagents;
pub(crate) use versioned_files::*;
pub use workspace_ops::AddChangeFileRequest;
pub use workspace_ops::add_change_file;
pub(crate) use workspace_ops::add_interactive_change_file;
#[cfg(test)]
pub(crate) use workspace_ops::build_lockfile_command_executions;
#[cfg(test)]
pub(crate) use workspace_ops::change_type_default_bump;
pub use workspace_ops::discover_workspace;
use workspace_ops::init_workspace;
pub use workspace_ops::plan_release;
use workspace_ops::populate_workspace;
pub use workspace_ops::prepare_release;
#[cfg(test)]
pub(crate) use workspace_ops::prepare_release_execution;
pub(crate) use workspace_ops::prepare_release_execution_with_file_diffs;
pub(crate) use workspace_ops::render_change_target_markdown;
#[cfg(test)]
pub(crate) use workspace_ops::render_cli_commands_toml;
#[cfg(test)]
pub(crate) use workspace_ops::render_interactive_changeset_markdown;
#[cfg(feature = "cargo")]
pub(crate) use workspace_ops::validate_cargo_workspace_version_groups;

mod analyze;
mod changelog;
mod changeset_policy;
mod changesets;
mod cli;
mod cli_help;
mod cli_progress;
mod cli_runtime;
mod git_support;
mod hosted_sources;
mod interactive;
mod lint;
mod mcp;
mod package_publish;
mod prepared_release_cache;
mod publish_rate_limits;
mod release_artifacts;
mod release_record;
mod skill;
mod subagents;
mod tracing_setup;
mod versioned_files;
mod workspace_ops;

pub(crate) use prepared_release_cache::ensure_monochange_artifact_ignored;
pub(crate) use prepared_release_cache::maybe_load_prepared_release_execution;
pub(crate) use prepared_release_cache::save_prepared_release_execution;

/// Output renderer used by CLI commands and preview helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
	Text,
	Markdown,
	Json,
}

/// Semver bump accepted by `mc change` and related APIs.
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

/// Repo-local subagent target understood by `mc subagents`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentTarget {
	Claude,
	Vscode,
	Copilot,
	Pi,
	Codex,
	Cursor,
}

impl SubagentTarget {
	fn all() -> Vec<Self> {
		vec![
			Self::Claude,
			Self::Vscode,
			Self::Copilot,
			Self::Pi,
			Self::Codex,
			Self::Cursor,
		]
	}

	fn from_cli_value(value: &str) -> Option<Self> {
		match value {
			"claude" => Some(Self::Claude),
			"vscode" => Some(Self::Vscode),
			"copilot" => Some(Self::Copilot),
			"pi" => Some(Self::Pi),
			"codex" => Some(Self::Codex),
			"cursor" => Some(Self::Cursor),
			_ => None,
		}
	}
}

/// Output renderer for `mc subagents`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentOutputFormat {
	Text,
	Json,
}

fn parse_subagent_output_format_or_default(value: Option<&String>) -> SubagentOutputFormat {
	match value.map_or("text", String::as_str) {
		"json" => SubagentOutputFormat::Json,
		_ => SubagentOutputFormat::Text,
	}
}

fn parse_subagent_targets<'value, I>(values: Option<I>) -> MonochangeResult<Vec<SubagentTarget>>
where
	I: IntoIterator<Item = &'value String>,
{
	let mut targets = Vec::new();

	for value in values.into_iter().flatten() {
		let Some(target) = SubagentTarget::from_cli_value(value) else {
			return Err(MonochangeError::Config(format!(
				"unsupported subagent target `{value}`"
			)));
		};

		if targets.contains(&target) {
			continue;
		}

		targets.push(target);
	}

	if targets.is_empty() {
		return Err(MonochangeError::Config(
			"expected at least one subagent target or `--all`".to_string(),
		));
	}

	Ok(targets)
}

/// Outward release target derived from a prepared release.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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

/// Rendered changelog payload produced during release preparation.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreparedChangelog {
	pub owner_id: String,
	pub owner_kind: ReleaseOwnerKind,
	pub path: PathBuf,
	pub format: ChangelogFormat,
	pub notes: ReleaseNotesDocument,
	pub rendered: String,
}

/// Structured result returned by release preparation APIs.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreparedRelease {
	pub plan: ReleasePlan,
	pub changeset_paths: Vec<PathBuf>,
	pub changesets: Vec<PreparedChangeset>,
	pub released_packages: Vec<String>,
	pub package_publications: Vec<PackagePublicationTarget>,
	pub version: Option<String>,
	pub group_version: Option<String>,
	pub release_targets: Vec<ReleaseTarget>,
	pub changed_files: Vec<PathBuf>,
	pub changelogs: Vec<PreparedChangelog>,
	pub updated_changelogs: Vec<PathBuf>,
	pub deleted_changesets: Vec<PathBuf>,
	pub dry_run: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparedFileDiff {
	path: PathBuf,
	diff: String,
	#[serde(skip_serializing)]
	display_diff: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct StepPhaseTiming {
	label: String,
	duration: Duration,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PreparedReleaseExecution {
	prepared_release: PreparedRelease,
	file_diffs: Vec<PreparedFileDiff>,
	phase_timings: Vec<StepPhaseTiming>,
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangesetDiagnosticsReport {
	pub(crate) requested_changesets: Vec<PathBuf>,
	pub(crate) changesets: Vec<PreparedChangeset>,
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
	quiet: bool,
	show_diff: bool,
	inputs: BTreeMap<String, Vec<String>>,
	last_step_inputs: BTreeMap<String, Vec<String>>,
	prepared_release: Option<PreparedRelease>,
	prepared_file_diffs: Vec<PreparedFileDiff>,
	release_manifest_path: Option<PathBuf>,
	release_requests: Vec<SourceReleaseRequest>,
	release_results: Vec<String>,
	release_request: Option<SourceChangeRequest>,
	release_request_result: Option<String>,
	release_commit_report: Option<CommitReleaseReport>,
	package_publish_report: Option<package_publish::PackagePublishReport>,
	rate_limit_report: Option<monochange_core::PublishRateLimitReport>,
	issue_comment_plans: Vec<HostedIssueCommentPlan>,
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

/// Run the `monochange` CLI from the current process environment.
///
/// This initializes tracing, parses `std::env::args_os()`, executes the
/// matching subcommand, and prints any non-empty stdout payload unless
/// `--quiet` was requested.
#[must_use = "the run result must be checked"]
pub fn run_from_env(bin_name: &'static str) -> MonochangeResult<()> {
	let log_level = extract_log_level_from_args();
	tracing_setup::init_tracing(log_level.as_deref());

	let quiet = extract_quiet_from_args(std::env::args_os());
	let args = std::env::args_os();
	let output = run_with_args(bin_name, args)?;
	if !quiet && !output.is_empty() {
		println!("{output}");
	}
	Ok(())
}

fn extract_log_level_from_args() -> Option<String> {
	extract_log_level(std::env::args())
}

fn quiet_from_os_arg(arg: &OsString) -> bool {
	matches!(arg.to_str(), Some("--quiet" | "-q"))
}

fn extract_quiet_from_args<I>(args: I) -> bool
where
	I: IntoIterator<Item = OsString>,
{
	args.into_iter().any(|arg| quiet_from_os_arg(&arg))
}

fn extract_log_level<I>(args: I) -> Option<String>
where
	I: IntoIterator<Item = String>,
{
	let mut args = args.into_iter();

	while let Some(arg) = args.next() {
		if arg == "--log-level" {
			return args.next();
		}

		if let Some(value) = arg.strip_prefix("--log-level=") {
			return Some(value.to_string());
		}
	}

	None
}

/// Execute the `monochange` CLI with an explicit argument iterator.
#[must_use = "the run result must be checked"]
pub fn run_with_args<I>(bin_name: &'static str, args: I) -> MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	let root = current_dir_or_dot();
	run_with_args_in_dir(bin_name, args, &root)
}

#[tracing::instrument(skip_all, fields(bin_name))]
/// Execute the `monochange` CLI against an explicit repository root.
///
/// This is primarily useful for tests and embedding, where the caller wants to
/// control both the argv payload and the workspace root used for config loading
/// and command execution.
#[doc(hidden)]
pub fn run_with_args_in_dir<I>(
	bin_name: &'static str,
	args: I,
	root: &Path,
) -> MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	let args = args.into_iter().collect::<Vec<_>>();
	let configuration = load_workspace_configuration(root);
	let cli = cli_commands_from_config(&configuration);
	let quiet = extract_quiet_from_args(args.iter().cloned());
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
		Some(("help", help_matches)) => {
			let command_name = help_matches
				.get_one::<String>("command")
				.map_or("", String::as_str);
			let output = if command_name.is_empty() {
				cli_help::render_overview_help(bin_name)
			} else {
				cli_help::render_command_help(bin_name, command_name)
			};
			Ok(output)
		}
		Some(("init", init_matches)) => {
			let provider = init_matches
				.get_one::<String>("provider")
				.map(String::as_str);
			let result = init_workspace(root, init_matches.get_flag("force"), provider)?;
			if quiet {
				Ok(String::new())
			} else {
				Ok(result.summary())
			}
		}
		Some(("populate", _)) => {
			if quiet {
				return Ok(String::new());
			}
			let result = populate_workspace(root)?;
			if result.added_commands.is_empty() {
				Ok(format!(
					"{} already defines all default CLI commands",
					result.path.display()
				))
			} else {
				Ok(format!(
					"updated {} and added {} default CLI commands: {}",
					result.path.display(),
					result.added_commands.len(),
					result.added_commands.join(", ")
				))
			}
		}
		Some(("skill", skill_matches)) => {
			let forwarded_args = skill_matches
				.get_many::<String>("args")
				.into_iter()
				.flatten()
				.cloned()
				.collect();
			let options = SkillOptions { forwarded_args };
			run_skill(root, &options)
		}
		Some(("subagents", subagent_matches)) => {
			let targets = if subagent_matches.get_flag("all") {
				SubagentTarget::all()
			} else {
				parse_subagent_targets(subagent_matches.get_many::<String>("target"))?
			};
			let format = parse_subagent_output_format_or_default(
				subagent_matches.get_one::<String>("format"),
			);
			let options = SubagentOptions {
				targets,
				force: subagent_matches.get_flag("force"),
				dry_run: quiet || subagent_matches.get_flag("dry-run"),
				format,
				generate_mcp: !subagent_matches.get_flag("no-mcp"),
			};
			let output = run_subagents(root, &options)?;
			if quiet { Ok(String::new()) } else { Ok(output) }
		}
		Some(("analyze", analyze_matches)) => {
			if quiet {
				return Ok(String::new());
			}
			let package = analyze_matches
				.get_one::<String>("package")
				.map(String::as_str)
				.ok_or_else(|| MonochangeError::Config("missing analyze package".to_string()))?;
			let release_ref = analyze_matches
				.get_one::<String>("release-ref")
				.map(String::as_str);
			let main_ref = analyze_matches
				.get_one::<String>("main-ref")
				.map(String::as_str);
			let head_ref = analyze_matches
				.get_one::<String>("head-ref")
				.map(String::as_str);
			let detection_level = analyze_matches
				.get_one::<String>("detection-level")
				.map_or("signature", String::as_str);
			let format = if analyze_matches
				.get_one::<String>("format")
				.is_some_and(|value| value == "json")
			{
				OutputFormat::Json
			} else {
				OutputFormat::Text
			};
			render_analyze_report(
				root,
				package,
				release_ref,
				main_ref,
				head_ref,
				detection_level,
				format,
			)
		}
		Some(("mcp", _)) => run_mcp_command_with(quiet, mcp::run_server),
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
		Some(("tag-release", tag_release_matches)) => {
			let from = tag_release_matches
				.get_one::<String>("from")
				.map(String::as_str)
				.ok_or_else(|| MonochangeError::Config("missing tag-release ref".to_string()))?;
			let format = if tag_release_matches
				.get_one::<String>("format")
				.is_some_and(|value| value == "json")
			{
				OutputFormat::Json
			} else {
				OutputFormat::Text
			};
			let push = tag_release_matches
				.get_one::<String>("push")
				.is_none_or(|value| value == "true");
			let dry_run = quiet || tag_release_matches.get_flag("dry-run");
			render_release_tag_report(root, from, format, push, dry_run)
		}
		Some(("check", check_matches)) => {
			if quiet {
				return Ok(String::new());
			}
			let fix = check_matches.get_flag("fix");
			let format = if check_matches
				.get_one::<String>("format")
				.is_some_and(|value| value == "json")
			{
				OutputFormat::Json
			} else {
				OutputFormat::Text
			};
			let ecosystems: Vec<String> = check_matches
				.get_many::<String>("ecosystem")
				.map(|values| values.map(String::as_str).map(String::from).collect())
				.unwrap_or_default();
			let only_rules: Vec<String> = check_matches
				.get_many::<String>("only")
				.map(|values| values.map(String::as_str).map(String::from).collect())
				.unwrap_or_default();
			lint::run_check_command(root, fix, &ecosystems, &only_rules, format)
		}
		Some(("lint", lint_matches)) => {
			if quiet {
				return Ok(String::new());
			}
			lint::handle_lint_subcommand(root, lint_matches)
		}
		Some((cli_command_name, cli_command_matches)) => {
			let configuration = configuration?;
			execute_matches(
				root,
				&configuration,
				cli_command_name,
				cli_command_matches,
				quiet,
			)
		}
		None => Err(MonochangeError::Config("Usage: mc".to_string())),
	}
}

fn run_mcp_command_with<F, Fut>(quiet: bool, run_server: F) -> MonochangeResult<String>
where
	F: FnOnce() -> Fut,
	Fut: Future<Output = ()>,
{
	if quiet {
		return Ok(String::new());
	}

	let runtime = tokio::runtime::Runtime::new()
		.map_err(|error| MonochangeError::Config(error.to_string()))?;
	runtime.block_on(run_server());
	Ok(String::new())
}

fn format_publish_state(publish_state: monochange_core::PublishState) -> &'static str {
	match publish_state {
		monochange_core::PublishState::Public => "public",
		monochange_core::PublishState::Private => "private",
		monochange_core::PublishState::Unpublished => "unpublished",
		monochange_core::PublishState::Excluded => "excluded",
		_ => "unknown",
	}
}

#[cfg(test)]
mod __tests;
