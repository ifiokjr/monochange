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
use monochange_core::ChangesetRevision;
use monochange_core::CliCommandDefinition;
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
use cli_runtime::execute_matches;
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
pub use workspace_ops::AddChangeFileRequest;

pub(crate) use changelog::*;
pub(crate) use changeset_policy::compute_changed_paths_since;
pub(crate) use changeset_policy::is_changeset_markdown_path;
pub(crate) use changeset_policy::normalize_changed_path;
pub(crate) use changesets::*;
pub(crate) use release_artifacts::*;
pub(crate) use versioned_files::*;
pub(crate) use workspace_ops::add_interactive_change_file;
pub(crate) use workspace_ops::render_change_target_markdown;
pub(crate) use workspace_ops::validate_cargo_workspace_version_groups;

#[cfg(test)]
pub(crate) use assist::assistant_display_name;
#[cfg(test)]
pub(crate) use assist::assistant_setup_payload;
#[cfg(test)]
pub(crate) use cli::apply_runtime_change_type_choices;
#[cfg(test)]
pub(crate) use cli::build_assist_subcommand;
#[cfg(test)]
pub(crate) use cli::build_cli_command_subcommand;
#[cfg(test)]
pub(crate) use cli::build_command_for_root;
#[cfg(test)]
pub(crate) use cli::build_release_record_subcommand;
#[cfg(test)]
pub(crate) use cli::cli_command_after_help;
#[cfg(test)]
pub(crate) use cli::configured_change_type_choices;
#[cfg(test)]
pub(crate) use cli_runtime::build_cli_template_context;
#[cfg(test)]
pub(crate) use cli_runtime::build_retarget_release_report;
#[cfg(test)]
pub(crate) use cli_runtime::collect_cli_command_inputs;
#[cfg(test)]
pub(crate) use cli_runtime::execute_cli_command;
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
pub(crate) use cli_runtime::render_cli_command_result;
#[cfg(test)]
pub(crate) use cli_runtime::render_retarget_release_report;
#[cfg(test)]
pub(crate) use cli_runtime::retarget_operation_label;
#[cfg(test)]
pub(crate) use cli_runtime::template_value_to_input_values;
#[cfg(test)]
pub(crate) use git_support::read_git_commit_message;
#[cfg(test)]
pub(crate) use git_support::run_git_capture;
#[cfg(test)]
pub(crate) use git_support::run_git_process;
#[cfg(test)]
pub(crate) use git_support::run_git_status;
#[cfg(test)]
pub(crate) use workspace_ops::change_type_default_bump;
#[cfg(test)]
pub(crate) use workspace_ops::render_interactive_changeset_markdown;

mod assist;
mod changelog;
mod changeset_policy;
mod changesets;
mod cli;
mod cli_runtime;
mod git_support;
mod interactive;
mod mcp;
mod release_artifacts;
mod release_record;
mod versioned_files;
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
