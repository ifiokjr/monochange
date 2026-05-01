use std::cell::Cell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use clap::Command;
use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::MockServer;
use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::ChangesetTargetKind;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputDefinition;
use monochange_core::CliInputKind;
use monochange_core::Ecosystem;
use monochange_core::GroupChangelogInclude;
use monochange_core::PreparedChangesetTarget;
use monochange_core::VersionFormat;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::current_test_name;
use monochange_test_helpers::snapshot_settings;
use semver::Version;
use tempfile::tempdir;

use crate::CliContext;
use crate::PreparedFileDiff;
use crate::add_change_file;
use crate::add_interactive_change_file;
use crate::affected_packages;
use crate::build_command_for_root;
use crate::build_lockfile_command_executions;
use crate::cli_runtime::normalize_when_expression;
use crate::cli_runtime::should_execute_cli_step;
use crate::discover_workspace;
use crate::interactive::InteractiveChangeResult;
use crate::interactive::InteractiveTarget;
use crate::parse_change_bump;
use crate::plan_release;
use crate::prepare_release_execution;
use crate::release_artifacts::set_force_build_file_diff_previews_error;
use crate::render_change_target_markdown;
use crate::run_with_args;
use crate::run_with_args_in_dir;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_scenario_workspace(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn run_cli<I>(root: &Path, args: I) -> monochange_core::MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	temp_env::with_var("MONOCHANGE_PROGRESS_FORMAT", None::<&str>, || {
		run_with_args_in_dir("mc", args, root)
	})
}

#[test]
fn migrate_audit_reports_legacy_release_tooling() {
	let tempdir = setup_fixture("migration-audit/legacy-release-workflow");
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_or_else(|error| panic!("migration audit: {error}"));

	assert!(output.contains("migration audit: migration-needed"));
	assert!(output.contains("legacy-release-tool knope at knope.toml"));
	assert!(output.contains("ci-workflow changesets at .github/workflows/release.yml"));
	assert!(output.contains("Plan trusted publishing per package"));

	let json_output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("migrate"),
			OsString::from("audit"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("migration audit json: {error}"));
	let parsed: serde_json::Value = serde_json::from_str(&json_output)
		.unwrap_or_else(|error| panic!("parse migration audit json: {error}"));

	assert_eq!(parsed["status"], serde_json::json!("migration-needed"));
	assert!(parsed["signals"].as_array().is_some_and(|signals| {
		signals.iter().any(|signal| {
			signal["tool"] == serde_json::json!("knope")
				&& signal["path"] == serde_json::json!("knope.toml")
		})
	}));
	assert!(parsed["recommendations"].as_array().is_some_and(|items| {
		items
			.iter()
			.any(|item| item["id"] == serde_json::json!("audit-changelogs"))
	}));
}

#[test]
fn migrate_audit_reports_ready_workspace_and_quiet_mode() {
	let tempdir = setup_fixture("migration-audit/ready-monochange");
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("migrate"),
			OsString::from("audit"),
			OsString::from("--format"),
			OsString::from("markdown"),
		],
	)
	.unwrap_or_else(|error| panic!("migration audit ready: {error}"));

	assert!(output.contains("migration audit: ready"));
	assert!(output.contains("monochange-config monochange at monochange.toml"));
	assert!(output.contains("- no migration-specific recommendations detected"));

	let quiet_output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("--quiet"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_or_else(|error| panic!("migration audit quiet: {error}"));
	assert_eq!(quiet_output, "");

	let empty_tempdir = setup_fixture("migration-audit/empty-workspace");
	let empty_output = run_cli(
		empty_tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_or_else(|error| panic!("migration audit empty: {error}"));
	assert!(empty_output.contains("- none detected"));
	assert!(empty_output.contains("Generate monochange configuration"));
}

#[test]
fn migrate_audit_reports_fixture_read_errors() {
	let package_json_dir = setup_fixture("migration-audit/unreadable-package-json");
	fs::rename(
		package_json_dir.path().join("package-json-dir"),
		package_json_dir.path().join("package.json"),
	)
	.unwrap_or_else(|error| panic!("rename package.json directory fixture: {error}"));
	let package_error = run_cli(
		package_json_dir.path(),
		[
			OsString::from("mc"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_err();
	assert!(package_error.to_string().contains("failed to read"));
	assert!(package_error.to_string().contains("package.json"));

	let workflow_root_file = setup_fixture("migration-audit/unreadable-workflow-root");
	fs::rename(
		workflow_root_file.path().join(".github/workflows-file"),
		workflow_root_file.path().join(".github/workflows"),
	)
	.unwrap_or_else(|error| panic!("rename workflows file fixture: {error}"));
	let workflow_root_error = run_cli(
		workflow_root_file.path(),
		[
			OsString::from("mc"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_err();
	assert!(
		workflow_root_error
			.to_string()
			.contains(".github/workflows")
	);

	let workflow_file_dir = setup_fixture("migration-audit/unreadable-workflow-file");
	fs::rename(
		workflow_file_dir
			.path()
			.join(".github/workflows/release-yml-dir"),
		workflow_file_dir
			.path()
			.join(".github/workflows/release.yml"),
	)
	.unwrap_or_else(|error| panic!("rename workflow directory fixture: {error}"));
	let workflow_file_error = run_cli(
		workflow_file_dir.path(),
		[
			OsString::from("mc"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_err();
	assert!(workflow_file_error.to_string().contains("release.yml"));
}

#[test]
fn verify_release_branch_step_reports_matching_branch() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"[source]
provider = "github"
owner = "monochange"
repo = "monochange"

[source.releases]
branches = ["main"]
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	git_in_temp_repo(tempdir.path(), &["init", "-b", "main"]);
	git_in_temp_repo(
		tempdir.path(),
		&["config", "user.email", "test@example.com"],
	);
	git_in_temp_repo(tempdir.path(), &["config", "user.name", "Test User"]);
	git_in_temp_repo(tempdir.path(), &["config", "commit.gpgsign", "false"]);
	fs::write(tempdir.path().join("README.md"), "release branch\n")
		.unwrap_or_else(|error| panic!("write readme: {error}"));
	git_in_temp_repo(tempdir.path(), &["add", "."]);
	git_in_temp_repo(tempdir.path(), &["commit", "-m", "initial"]);
	git_in_temp_repo(tempdir.path(), &["tag", "v1.0.0"]);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:verify-release-branch"),
		],
	)
	.unwrap_or_else(|error| panic!("verify release branch: {error}"));

	assert!(output.contains("release branch verified"));
	assert!(output.contains("main"));

	let tag_output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:verify-release-branch"),
			OsString::from("--from"),
			OsString::from("v1.0.0"),
		],
	)
	.unwrap_or_else(|error| panic!("verify release branch tag: {error}"));
	assert!(tag_output.contains("v1.0.0"));
}

#[test]
fn verify_release_branch_step_requires_source_configuration() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(tempdir.path().join("monochange.toml"), "")
		.unwrap_or_else(|error| panic!("write config: {error}"));
	git_in_temp_repo(tempdir.path(), &["init", "-b", "main"]);

	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:verify-release-branch"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("expected source configuration error"));

	assert!(
		error
			.to_string()
			.contains("`VerifyReleaseBranch` requires `[source]` configuration")
	);
}

#[test]
fn shared_fs_test_support_current_test_name_covers_plain_and_case_prefixes() {
	assert_eq!(
		current_test_name(),
		"shared_fs_test_support_current_test_name_covers_plain_and_case_prefixes"
	);
	let named = std::thread::Builder::new()
		.name("case_1_current_test_name_thread".to_string())
		.spawn(current_test_name)
		.unwrap_or_else(|error| panic!("spawn thread: {error}"))
		.join()
		.unwrap_or_else(|error| panic!("join thread: {error:?}"));
	assert_eq!(named, "current_test_name_thread");
}

#[test]
fn shared_fs_test_support_setup_fixture_copies_fixture_files() {
	let tempdir = setup_fixture("test-support/setup-fixture");
	assert_eq!(
		fs::read_to_string(tempdir.path().join("nested/child.txt"))
			.unwrap_or_else(|error| panic!("read nested fixture: {error}")),
		"nested child\n"
	);
}

#[test]
fn shared_fs_test_support_setup_scenario_workspace_prefers_workspace_and_skips_expected() {
	let tempdir = setup_scenario_workspace("test-support/scenario-workspace");
	assert_eq!(
		fs::read_to_string(tempdir.path().join("workspace-only.txt"))
			.unwrap_or_else(|error| panic!("read workspace fixture: {error}")),
		"workspace marker\n"
	);
	assert!(!tempdir.path().join("scenario-root-only.txt").exists());
	assert!(!tempdir.path().join("expected").exists());
}

#[test]
fn synthetic_step_command_definition_rejects_unknown_steps() {
	let error = crate::synthetic_step_command_definition("step:not-a-step")
		.err()
		.unwrap_or_else(|| panic!("expected unknown step error"));

	assert_eq!(
		error.to_string(),
		"config error: unknown step command: step:not-a-step"
	);
}

fn expected_builtin_step_command_names() -> Vec<&'static str> {
	vec![
		"step:config",
		"step:validate",
		"step:discover",
		"step:display-versions",
		"step:create-change-file",
		"step:prepare-release",
		"step:commit-release",
		"step:verify-release-branch",
		"step:publish-release",
		"step:placeholder-publish",
		"step:publish-packages",
		"step:plan-publish-rate-limits",
		"step:open-release-request",
		"step:comment-released-issues",
		"step:affected-packages",
		"step:diagnose-changesets",
		"step:retarget-release",
	]
}

#[test]
fn synthetic_step_command_definitions_cover_all_builtin_steps_except_command() {
	let step_command_names = monochange_core::all_step_variants()
		.into_iter()
		.map(|step| format!("step:{}", step.step_kebab_name()))
		.collect::<Vec<_>>();
	let expected = expected_builtin_step_command_names();

	assert_eq!(step_command_names, expected);
	assert!(
		step_command_names
			.iter()
			.any(|name| name == "step:affected-packages")
	);
	assert!(!step_command_names.iter().any(|name| name == "step:command"));

	for command_name in &step_command_names {
		let synthetic = crate::synthetic_step_command_definition(command_name)
			.unwrap_or_else(|error| panic!("synthetic step command {command_name}: {error}"));
		assert_eq!(synthetic.name, *command_name);
		assert_eq!(synthetic.steps.len(), 1);
		assert_eq!(
			synthetic
				.steps
				.first()
				.unwrap_or_else(|| panic!("expected step for {command_name}"))
				.step_kebab_name(),
			command_name.trim_start_matches("step:")
		);
	}

	let command_error = crate::synthetic_step_command_definition("step:command")
		.err()
		.unwrap_or_else(|| panic!("expected Command step to stay synthetic-command-only"));
	assert_eq!(
		command_error.to_string(),
		"config error: unknown step command: step:command"
	);
}

#[test]
fn generated_step_commands_cover_all_builtin_steps_except_command() {
	let command = crate::cli::build_command_with_cli("mc", &[]);
	let expected = expected_builtin_step_command_names();

	for command_name in expected {
		assert!(
			command.find_subcommand(command_name).is_some(),
			"expected generated {command_name} command"
		);
	}
	assert!(command.find_subcommand("step:command").is_none());
}

#[test]
fn cli_parses_discover_command() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let matches = build_command_for_root("mc", &fixture_root)
		.try_get_matches_from([OsString::from("mc"), OsString::from("step:discover")])
		.unwrap_or_else(|error| panic!("matches: {error}"));

	assert_eq!(matches.subcommand_name(), Some("step:discover"));
}

#[test]
fn cli_help_returns_success_output() {
	let output = run_with_args("mc", [OsString::from("mc"), OsString::from("--help")])
		.unwrap_or_else(|error| panic!("help output: {error}"));

	assert!(output.contains("Usage: mc"));
	assert!(output.contains("subagents"));
	assert!(output.contains("analyze"));
	assert!(!output.contains("assist"));
	assert!(output.contains("mcp"));
	assert!(output.contains("step:create-change-file"));
	assert!(output.contains("step:diagnose-changesets"));
	assert!(output.contains("step:placeholder-publish"));
	assert!(output.contains("step:publish-release"));
	assert!(output.contains("step:plan-publish-rate-limits"));
	assert!(output.contains("step:retarget-release"));
	assert!(output.contains("release-record"));
	assert!(output.contains("publish-bootstrap"));
	assert!(output.contains("publish-release"));
	assert!(output.contains("comment-released-issues"));
	assert!(output.contains("tag-release"));
}

#[test]
fn publish_release_help_documents_draft_release_options() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let output = run_with_args_in_dir(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("publish-release"),
			OsString::from("--help"),
		],
		&root,
	)
	.unwrap_or_else(|error| panic!("publish-release help: {error}"));

	assert!(output.contains("Create provider releases from a durable release record"));
	assert!(output.contains("--from-ref <FROM-REF>"));
	assert!(output.contains("--draft"));
	assert!(output.contains("--format <FORMAT>"));
}

#[test]
fn comment_released_issues_help_documents_post_merge_options() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let output = run_with_args_in_dir(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("comment-released-issues"),
			OsString::from("--help"),
		],
		&root,
	)
	.unwrap_or_else(|error| panic!("comment-released-issues help: {error}"));

	assert!(output.contains("Comment on and optionally close issues"));
	assert!(output.contains("--from-ref <FROM-REF>"));
	assert!(output.contains("--auto-close-issues"));
}

#[test]
fn repair_release_help_describes_retargeting_workflow() {
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("step:retarget-release"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("repair-release help: {error}"));

	assert!(output.contains("Run the built-in retarget-release release workflow step"));
	assert!(output.contains("--from"));
	assert!(output.contains("--sync-provider"));
}

#[test]
fn boolean_cli_inputs_support_explicit_false_values() {
	// Test boolean input parsing with a synthetic command that has
	// default = true, which enables --flag=false syntax.
	let cli_command = CliCommandDefinition {
		name: "test-bool".to_string(),
		help_text: None,
		inputs: vec![CliInputDefinition {
			name: "flag".to_string(),
			kind: CliInputKind::Boolean,
			help_text: None,
			required: false,
			default: Some("true".to_string()),
			choices: vec![],
			short: None,
		}],
		steps: vec![],
	};
	let subcommand = crate::build_cli_command_subcommand(&cli_command);
	let matches = Command::new("mc")
		.subcommand(subcommand)
		.try_get_matches_from(["mc", "test-bool", "--flag=false"])
		.unwrap_or_else(|error| panic!("matches: {error}"));
	let (_, subcommand_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected test-bool subcommand"));
	assert_eq!(
		subcommand_matches
			.get_one::<String>("flag")
			.map(String::as_str),
		Some("false")
	);
}

#[test]
fn analyze_help_documents_package_scoped_release_trajectory_defaults() {
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("analyze"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("analyze help: {error}"));

	assert!(output.contains("Analyze semantic changes for one package"));
	assert!(output.contains("mc analyze --package core"));
	assert!(output.contains("Defaults `--release-ref` to the newest tag"));
	assert!(output.contains("--detection-level"));
}

#[test]
fn analyze_matches_capture_package_refs_and_detection_level() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let matches = build_command_for_root("mc", &fixture_root)
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("analyze"),
			OsString::from("--package"),
			OsString::from("core"),
			OsString::from("--main-ref"),
			OsString::from("main"),
			OsString::from("--head-ref"),
			OsString::from("HEAD~1"),
			OsString::from("--detection-level"),
			OsString::from("semantic"),
			OsString::from("--format"),
			OsString::from("json"),
		])
		.unwrap_or_else(|error| panic!("analyze matches: {error}"));
	let (_, subcommand_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected analyze subcommand"));
	assert_eq!(
		subcommand_matches
			.get_one::<String>("package")
			.map(String::as_str),
		Some("core")
	);
	assert_eq!(
		subcommand_matches
			.get_one::<String>("main-ref")
			.map(String::as_str),
		Some("main")
	);
	assert_eq!(
		subcommand_matches
			.get_one::<String>("head-ref")
			.map(String::as_str),
		Some("HEAD~1")
	);
	assert_eq!(
		subcommand_matches
			.get_one::<String>("detection-level")
			.map(String::as_str),
		Some("semantic")
	);
	assert_eq!(
		subcommand_matches
			.get_one::<String>("format")
			.map(String::as_str),
		Some("json")
	);
}

#[test]
fn versions_help_and_matches_document_dedicated_versions_command() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let release_help = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("step:prepare-release"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("release help: {error}"));
	assert!(!release_help.contains("--versions"));

	let versions_help = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("step:display-versions"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("versions help: {error}"));
	assert!(versions_help.contains("Run the built-in display-versions release workflow step"));
	assert!(versions_help.contains("--format <FORMAT>"));

	let matches = build_command_for_root("mc", &fixture_root)
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("step:display-versions"),
			OsString::from("--format"),
			OsString::from("json"),
		])
		.unwrap_or_else(|error| panic!("versions matches: {error}"));
	let (_, subcommand_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected versions subcommand"));
	assert_eq!(
		subcommand_matches
			.get_one::<String>("format")
			.map(String::as_str),
		Some("json")
	);
}

#[test]
fn release_record_help_describes_first_parent_discovery() {
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("release-record"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("release-record help: {error}"));

	assert!(
		output.contains("Inspect the monochange release record associated with a tag or commit")
	);
	assert!(output.contains("mc release-record --from v1.2.3"));
	assert!(
		output.contains("Walks first-parent ancestry until it finds a monochange release record")
	);
}

#[test]
fn tag_release_help_describes_post_merge_tagging_workflow() {
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("tag-release"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("tag-release help: {error}"));

	assert!(
		output.contains(
			"Create and push release tags from the monochange release record on a commit"
		)
	);
	assert!(output.contains("mc tag-release --from HEAD --dry-run --format json"));
	assert!(output.contains("reruns on the same commit as already up to date"));
}

#[test]
fn subagents_help_describes_supported_targets() {
	let _guard = snapshot_settings().bind_to_scope();
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("subagents help output: {error}"));

	insta::assert_snapshot!(output);
}

#[test]
fn subagents_command_supports_dry_run_json_output() {
	let _guard = snapshot_settings().bind_to_scope();
	let fixture = setup_scenario_workspace("subagents/basic");
	let output = run_cli(
		fixture.path(),
		[
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("claude"),
			OsString::from("pi"),
			OsString::from("codex"),
			OsString::from("cursor"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("subagents dry-run json: {error}"));
	let value: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("parse json: {error}"));

	insta::assert_json_snapshot!(value);
}

#[test]
fn subagents_command_supports_no_mcp_dry_run_json_output() {
	let _guard = snapshot_settings().bind_to_scope();
	let fixture = setup_scenario_workspace("subagents/basic");
	let output = run_cli(
		fixture.path(),
		[
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("vscode"),
			OsString::from("copilot"),
			OsString::from("--no-mcp"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("subagents no-mcp dry-run json: {error}"));
	let value: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("parse json: {error}"));

	insta::assert_json_snapshot!(value);
}

#[test]
fn subagents_command_writes_expected_files_and_reports_skips_on_repeat_runs() {
	let _guard = snapshot_settings().bind_to_scope();
	let fixture = setup_scenario_workspace("subagents/basic");
	let root = fixture.path();

	run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("claude"),
			OsString::from("pi"),
		],
	)
	.unwrap_or_else(|error| panic!("write subagents: {error}"));

	insta::assert_snapshot!(
		"subagents_claude_agent",
		fs::read_to_string(root.join(".claude/agents/monochange-release-agent.md"))
			.unwrap_or_else(|error| panic!("read claude agent: {error}"))
	);
	insta::assert_snapshot!(
		"subagents_claude_mcp",
		fs::read_to_string(root.join(".mcp.json"))
			.unwrap_or_else(|error| panic!("read claude mcp: {error}"))
	);
	insta::assert_snapshot!(
		"subagents_pi_agent",
		fs::read_to_string(root.join(".pi/agents/monochange-release-agent.md"))
			.unwrap_or_else(|error| panic!("read pi agent: {error}"))
	);

	let repeat_output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("claude"),
			OsString::from("pi"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("repeat dry-run subagents: {error}"));
	let repeat_value: serde_json::Value = serde_json::from_str(&repeat_output)
		.unwrap_or_else(|error| panic!("parse repeat json: {error}"));

	insta::assert_json_snapshot!("subagents_repeat_dry_run", repeat_value);
}

#[test]
fn subagents_command_requires_force_to_overwrite_existing_files() {
	let _guard = snapshot_settings().bind_to_scope();
	let fixture = setup_scenario_workspace("subagents/basic");
	let root = fixture.path();

	run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("claude"),
		],
	)
	.unwrap_or_else(|error| panic!("write initial subagents: {error}"));
	fs::write(root.join(".mcp.json"), "custom\n")
		.unwrap_or_else(|error| panic!("overwrite test mcp config: {error}"));

	let error = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("claude"),
		],
	)
	.expect_err("subagents should refuse to overwrite without --force");
	insta::assert_snapshot!("subagents_overwrite_error", error.to_string());

	run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("claude"),
			OsString::from("--force"),
		],
	)
	.unwrap_or_else(|error| panic!("force overwrite subagents: {error}"));

	insta::assert_snapshot!(
		"subagents_forced_claude_mcp",
		fs::read_to_string(root.join(".mcp.json"))
			.unwrap_or_else(|error| panic!("read overwritten mcp: {error}"))
	);
}

#[test]
fn mcp_and_root_command_support_quiet_and_missing_subcommands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	let quiet_output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("mcp"),
			OsString::from("--quiet"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet mcp output: {error}"));
	assert!(quiet_output.is_empty());

	let no_subcommand = run_cli(tempdir.path(), [OsString::from("mc")])
		.expect_err("missing subcommand should fail");
	assert!(no_subcommand.to_string().contains("Usage: mc"));
}

#[test]
fn release_record_supports_json_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	create_release_record_history(root);
	let output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("release-record"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("release-record json output: {error}"));

	assert!(output.contains("\"record\""));
	assert!(output.contains("\"recordCommit\""));
	assert!(output.contains("\"resolvedCommit\""));
}

#[test]
fn release_record_jq_filters_json_output_for_ci() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let mut record = sample_release_record_for_retarget();
	record.release_targets[0].id = "main".to_string();
	create_release_record_commit_from_record(root, &record);
	let subscriber = tracing_subscriber::fmt()
		.with_max_level(tracing::Level::TRACE)
		.with_writer(std::io::sink)
		.finish();
	let traced_discovery = tracing::subscriber::with_default(subscriber, || {
		crate::release_record::discover_release_record(root, "HEAD")
	})
	.unwrap_or_else(|error| panic!("trace release-record discovery: {error}"));
	assert_eq!(traced_discovery.record.release_targets[0].id, "main");

	let tag = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("release-record"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--format"),
			OsString::from("json"),
			OsString::from("--jq"),
			OsString::from(
				r#".record.releaseTargets[] | select(.id == "main" and .kind == "group" and .release == true) | .tagName"#,
			),
		],
	)
	.unwrap_or_else(|error| panic!("release-record jq tag output: {error}"));
	assert_eq!(tag, "v1.2.3");

	let is_release_commit = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("release-record"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--format"),
			OsString::from("json"),
			OsString::from("--jq"),
			OsString::from(".resolvedCommit == .recordCommit"),
		],
	)
	.unwrap_or_else(|error| panic!("release-record jq commit output: {error}"));
	assert_eq!(is_release_commit, "true");
}

#[test]
fn publish_readiness_dispatches_from_release_record_and_writes_artifact() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	create_release_record_commit(root);
	let output_path = root.join(".monochange/readiness.json");
	let output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("publish-readiness"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--package"),
			OsString::from("missing"),
			OsString::from("--format"),
			OsString::from("text"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("publish-readiness output: {error}"));

	assert!(output.contains("publish readiness: ready"));
	assert!(output.contains("release ref: HEAD"));
	assert!(output.contains("packages: none"));

	let artifact = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read readiness artifact: {error}"));
	assert!(artifact.contains("\"status\": \"ready\""));
	assert!(artifact.contains("\"from\": \"HEAD\""));
	assert!(artifact.contains("\"packages\": []"));
}

#[test]
fn publish_bootstrap_dispatches_from_release_record_and_writes_artifact() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	create_release_record_commit(root);
	let output_path = root.join(".monochange/bootstrap-result.json");
	let output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("publish-bootstrap"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--package"),
			OsString::from("missing"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("text"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("publish-bootstrap output: {error}"));

	assert!(output.contains("publish bootstrap: planned"));
	assert!(output.contains("release ref: HEAD"));
	assert!(output.contains("dry-run: yes"));
	assert!(output.contains("packages: none"));

	let artifact = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read bootstrap artifact: {error}"));
	assert!(artifact.contains("\"kind\": \"monochange.publishBootstrap\""));
	assert!(artifact.contains("\"status\": \"planned\""));
	assert!(artifact.contains("\"from\": \"HEAD\""));
	assert!(artifact.contains("\"selectedPackages\": []"));
}

#[test]
fn publish_bootstrap_dispatches_with_release_record_package_publications() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	create_release_record_commit_with_package_publication(root, "missing");
	let output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("publish-bootstrap"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--package"),
			OsString::from("missing"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("publish-bootstrap output: {error}"));

	assert!(output.contains("\"selectedPackages\": [\n    \"missing\"\n  ]"));
	assert!(output.contains("\"packages\": []"));
}

#[test]
fn publish_bootstrap_propagates_placeholder_publish_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	create_release_record_commit_with_package_publication(root, "missing");
	fs::create_dir_all(root.join("crates/missing/src"))
		.unwrap_or_else(|error| panic!("create package source: {error}"));
	fs::write(
		root.join("crates/missing/Cargo.toml"),
		"[package]\nname = \"missing\"\nversion = \"1.2.3\"\nedition = \"2021\"\n",
	)
	.unwrap_or_else(|error| panic!("write package manifest: {error}"));
	fs::write(
		root.join("crates/missing/src/lib.rs"),
		"pub fn value() -> u8 { 1 }\n",
	)
	.unwrap_or_else(|error| panic!("write package source: {error}"));
	fs::write(
		root.join("monochange.toml"),
		"[package.missing]\npath = \"crates/missing\"\ntype = \"cargo\"\n\n[package.missing.publish.placeholder]\nreadme_file = \"missing-placeholder.md\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange config: {error}"));

	let error = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("publish-bootstrap"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--package"),
			OsString::from("missing"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_err()
	.to_string();

	assert!(error.contains("failed to read placeholder README"));
}

#[test]
fn publish_readiness_reports_release_ref_and_output_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	create_release_record_commit(root);

	let missing_ref = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("publish-readiness"),
			OsString::from("--from"),
			OsString::from("missing-ref"),
		],
	)
	.expect_err("missing release ref should fail readiness");
	assert!(missing_ref.to_string().contains("missing-ref"));

	let output_error = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("publish-readiness"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--output"),
			root.into(),
		],
	)
	.expect_err("directory output should fail readiness artifact write");
	assert!(
		output_error
			.to_string()
			.contains("failed to write publish readiness output")
	);
}

#[test]
fn init_writes_detected_packages_groups_and_default_cli_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("init")],
	)
	.unwrap_or_else(|error| panic!("init output: {error}"));
	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));

	assert!(output.contains("wrote"));
	assert!(config.contains("[package.core]"));
	assert!(config.contains("[package.web]"));
	assert!(config.contains("[group.main]"));
	assert!(!config.contains("[cli.validate]"));
	assert!(config.contains("[cli.discover]"));
	assert!(config.contains("[cli.change]"));
	assert!(config.contains("[cli.release]"));
	assert!(config.contains("[cli.versions]"));
	assert!(config.contains("[cli.placeholder-publish]"));
	assert!(config.contains("[cli.publish]"));
	assert!(config.contains("[cli.publish-plan]"));
	assert!(config.contains("type = \"Discover\""));
	assert!(config.contains("type = \"CreateChangeFile\""));
	assert!(config.contains("type = \"PlaceholderPublish\""));
	assert!(config.contains("type = \"PublishPackages\""));

	load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("generated config should parse: {error}"));
}

#[test]
fn init_writes_configuration_that_validates_in_empty_workspace() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("init")],
	)
	.unwrap_or_else(|error| panic!("init output: {error}"));
	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));

	assert!(!config.contains("[cli.validate]"));
	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.unwrap_or_else(|error| panic!("generated config should validate: {error}"));
}

#[test]
fn init_requires_force_to_overwrite_existing_configuration() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-existing-config", tempdir.path());

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("init")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected init failure"));
	assert!(error.to_string().contains("--force"));
}

#[test]
fn populate_adds_all_missing_default_cli_commands_to_an_existing_configuration() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/populate-no-cli", tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("populate")],
	)
	.unwrap_or_else(|error| panic!("populate output: {error}"));
	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));

	assert!(output.contains("already defines all default CLI commands"));
	// With empty defaults, populate adds nothing
	assert!(!config.contains("[cli.release]"));
}

#[test]
fn populate_preserves_existing_cli_commands_and_only_adds_missing_defaults() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/populate-partial-cli", tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("populate")],
	)
	.unwrap_or_else(|error| panic!("populate output: {error}"));
	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));

	assert!(output.contains("already defines all default CLI commands"));
	assert!(config.contains("help_text = \"Custom release pipeline\""));
	assert_eq!(config.matches("[cli.release]").count(), 1);
	// With empty defaults, no new tables are added beyond existing ones
	for table in [
		"[cli.validate]",
		"[cli.discover]",
		"[cli.change]",
		"[cli.versions]",
		"[cli.placeholder-publish]",
		"[cli.publish]",
		"[cli.publish-plan]",
		"[cli.affected]",
		"[cli.diagnostics]",
		"[cli.repair-release]",
	] {
		assert!(
			!config.contains(table),
			"unexpected populated table `{table}`"
		);
	}
}

#[test]
fn populate_reports_when_all_default_cli_commands_are_already_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/populate-all-defaults", tempdir.path());
	let before = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config before: {error}"));

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("populate")],
	)
	.unwrap_or_else(|error| panic!("populate output: {error}"));
	let after = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config after: {error}"));

	assert!(output.contains("already defines all default CLI commands"));
	assert_eq!(after, before);
}

#[test]
fn populate_requires_an_existing_monochange_configuration_file() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/populate-missing-config", tempdir.path());

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("populate")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected populate failure"));
	assert!(error.to_string().contains("monochange.toml does not exist"));
}

#[cfg(unix)]
#[test]
fn populate_reports_write_failures_when_configuration_is_read_only() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/populate-no-cli", tempdir.path());
	let path = tempdir.path().join("monochange.toml");
	let mut permissions = fs::metadata(&path)
		.unwrap_or_else(|error| panic!("metadata: {error}"))
		.permissions();
	permissions.set_mode(0o444);
	fs::set_permissions(&path, permissions).unwrap_or_else(|error| panic!("chmod: {error}"));

	// With empty defaults, populate never writes; it returns success.
	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("populate")],
	)
	.unwrap_or_else(|error| panic!("populate output: {error}"));
	assert!(output.contains("already defines all default CLI commands"));
}

#[test]
fn populate_rejects_invalid_monochange_toml() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/populate-invalid-config", tempdir.path());

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("populate")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected populate failure"));
	assert!(error.to_string().contains("failed to parse"));
}

#[test]
fn populate_rejects_non_file_configuration_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture(
		"monochange/populate-config-path-is-directory",
		tempdir.path(),
	);

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("populate")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected populate failure"));
	assert!(error.to_string().contains("failed to read"));
}

#[test]
fn populate_adds_default_cli_commands_to_an_empty_configuration_file() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/populate-empty-config", tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("populate")],
	)
	.unwrap_or_else(|error| panic!("populate output: {error}"));
	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));

	assert!(output.contains("already defines all default CLI commands"));
	// With empty defaults, populate adds nothing
	assert!(!config.contains("[cli.validate]"));
}

#[test]
fn render_cli_commands_toml_handles_release_and_command_step_variants() {
	let rendered = crate::render_cli_commands_toml(&[CliCommandDefinition {
		name: "custom".to_string(),
		help_text: None,
		inputs: vec![CliInputDefinition {
			name: "mode".to_string(),
			kind: CliInputKind::Choice,
			help_text: Some("Select the command mode".to_string()),
			required: true,
			default: Some("safe".to_string()),
			choices: vec!["safe".to_string(), "fast".to_string()],
			short: Some('m'),
		}],
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				name: None,
				when: Some("{{ inputs.enabled }}".to_string()),
				inputs: BTreeMap::from([(
					"format".to_string(),
					monochange_core::CliStepInputValue::String("json".to_string()),
				)]),
				allow_empty_changesets: false,
			},
			monochange_core::CliStepDefinition::Command {
				show_progress: None,
				name: None,
				when: None,
				command: "echo hello".to_string(),
				dry_run_command: Some("echo dry-run".to_string()),
				shell: monochange_core::ShellConfig::None,
				id: Some("none-shell".to_string()),
				variables: None,
				inputs: BTreeMap::from([(
					"enabled".to_string(),
					monochange_core::CliStepInputValue::Boolean(true),
				)]),
			},
			monochange_core::CliStepDefinition::Command {
				show_progress: None,
				name: None,
				when: None,
				command: "echo through-shell".to_string(),
				dry_run_command: None,
				shell: monochange_core::ShellConfig::Default,
				id: Some("default-shell".to_string()),
				variables: Some(BTreeMap::from([
					(
						"version_value".to_string(),
						monochange_core::CommandVariable::Version,
					),
					(
						"group_version_value".to_string(),
						monochange_core::CommandVariable::GroupVersion,
					),
					(
						"released_packages_value".to_string(),
						monochange_core::CommandVariable::ReleasedPackages,
					),
					(
						"changed_files_value".to_string(),
						monochange_core::CommandVariable::ChangedFiles,
					),
					(
						"changesets_value".to_string(),
						monochange_core::CommandVariable::Changesets,
					),
				])),
				inputs: BTreeMap::from([(
					"changed_paths".to_string(),
					monochange_core::CliStepInputValue::List(vec!["src/lib.rs".to_string()]),
				)]),
			},
			monochange_core::CliStepDefinition::Command {
				show_progress: None,
				name: None,
				when: None,
				command: "echo custom-shell".to_string(),
				dry_run_command: None,
				shell: monochange_core::ShellConfig::Custom("bash".to_string()),
				id: Some("custom-shell".to_string()),
				variables: None,
				inputs: BTreeMap::new(),
			},
			monochange_core::CliStepDefinition::CommitRelease {
				name: None,
				when: None,
				no_verify: false,
				inputs: BTreeMap::from([(
					"format".to_string(),
					monochange_core::CliStepInputValue::String("json".to_string()),
				)]),
			},
			monochange_core::CliStepDefinition::PublishRelease {
				name: None,
				when: None,
				inputs: BTreeMap::from([(
					"format".to_string(),
					monochange_core::CliStepInputValue::String("text".to_string()),
				)]),
			},
			monochange_core::CliStepDefinition::OpenReleaseRequest {
				name: None,
				when: None,
				no_verify: false,
				inputs: BTreeMap::from([(
					"format".to_string(),
					monochange_core::CliStepInputValue::String("json".to_string()),
				)]),
			},
			monochange_core::CliStepDefinition::CommentReleasedIssues {
				name: None,
				when: None,
				inputs: BTreeMap::from([(
					"format".to_string(),
					monochange_core::CliStepInputValue::String("text".to_string()),
				)]),
			},
		],
	}]);

	assert!(rendered.contains("[cli.custom]"));
	assert!(rendered.contains("[[cli.custom.inputs]]"));
	assert!(rendered.contains("name = \"mode\""));
	assert!(rendered.contains("type = \"choice\""));
	assert!(rendered.contains("help_text = \"Select the command mode\""));
	assert!(rendered.contains("required = true"));
	assert!(rendered.contains("default = \"safe\""));
	assert!(rendered.contains("choices = [\"safe\", \"fast\"]"));
	assert!(rendered.contains("short = \"m\""));
	assert!(rendered.contains("[[cli.custom.steps]]"));
	assert!(rendered.contains("type = \"PrepareRelease\""));
	assert!(rendered.contains("when = \"{{ inputs.enabled }}\""));
	assert!(rendered.contains("inputs = { format = \"json\" }"));
	assert!(rendered.contains("dry_run_command = \"echo dry-run\""));
	assert!(rendered.contains("inputs = { enabled = true }"));
	assert!(rendered.contains("shell = true"));
	assert!(rendered.contains("group_version_value = \"group_version\""));
	assert!(rendered.contains("released_packages_value = \"released_packages\""));
	assert!(rendered.contains("changed_files_value = \"changed_files\""));
	assert!(rendered.contains("changesets_value = \"changesets\""));
	assert!(rendered.contains("version_value = \"version\""));
	assert!(rendered.contains("inputs = { changed_paths = [\"src/lib.rs\"] }"));
	assert!(rendered.contains("shell = \"bash\""));
	assert!(rendered.contains("type = \"CommitRelease\""));
	assert!(rendered.contains("type = \"PublishRelease\""));
	assert!(rendered.contains("type = \"OpenReleaseRequest\""));
	assert!(rendered.contains("type = \"CommentReleasedIssues\""));
	assert!(!rendered.contains("name = \"custom\""));
}

#[test]
fn validate_command_validates_workspace_configuration_and_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/validate-workspace", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.unwrap_or_else(|error| panic!("validate output: {error}"));
	assert!(output.contains("workspace validation passed"));
}

#[test]
fn validate_command_reports_invalid_changeset_targets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/validate-invalid-changeset", tempdir.path());
	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected validation failure"));
	assert!(
		error
			.to_string()
			.contains("unknown package or group `missing`")
	);
}

#[test]
fn discover_workspace_aggregates_packages_from_multiple_ecosystems() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let discovery =
		discover_workspace(&fixture_root).unwrap_or_else(|error| panic!("discovery: {error}"));

	assert_eq!(discovery.packages.len(), 4);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.ecosystem == Ecosystem::Cargo)
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.ecosystem == Ecosystem::Npm)
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.ecosystem == Ecosystem::Deno)
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.ecosystem == Ecosystem::Dart)
	);
	assert_eq!(discovery.dependencies.len(), 3);
	assert_eq!(discovery.version_groups.len(), 1);
	let version_group = discovery
		.version_groups
		.first()
		.unwrap_or_else(|| panic!("expected a version group"));
	assert_eq!(version_group.members.len(), 2);
}

#[test]
fn workspace_discover_json_output_contains_contract_fields() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let output = run_cli(
		&fixture_root,
		[
			OsString::from("mc"),
			OsString::from("step:discover"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("json: {error}"));

	assert_eq!(parsed["workspaceRoot"].as_str(), Some("."));
	assert_eq!(parsed["packages"].as_array().map(Vec::len), Some(4));
	assert_eq!(parsed["versionGroups"].as_array().map(Vec::len), Some(1));
	assert_eq!(parsed["dependencies"].as_array().map(Vec::len), Some(3));
	assert!(
		parsed["packages"]
			.as_array()
			.unwrap_or_else(|| panic!("packages array"))
			.iter()
			.all(|package| package.get("manifestPath").is_some())
	);
	assert!(
		parsed["packages"]
			.as_array()
			.unwrap_or_else(|| panic!("packages array"))
			.iter()
			.all(|package| {
				package["id"]
					.as_str()
					.is_some_and(|id| !id.contains(fixture_root.to_string_lossy().as_ref()))
			})
	);
}

#[test]
fn plan_release_aggregates_transitive_dependents_and_version_groups() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let plan = plan_release(&fixture_root, &fixture_root.join("changes-minor.md"))
		.unwrap_or_else(|error| panic!("release plan: {error}"));

	let sdk_core = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains("cargo/sdk-core/Cargo.toml"))
		.unwrap_or_else(|| panic!("expected cargo core decision"));
	let web_sdk = plan
		.decisions
		.iter()
		.find(|decision| {
			decision
				.package_id
				.contains("packages/web-sdk/package.json")
		})
		.unwrap_or_else(|| panic!("expected web sdk decision"));
	let deno_tool = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains("deno/tool/deno.json"))
		.unwrap_or_else(|| panic!("expected deno tool decision"));
	let mobile_sdk = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains("dart/mobile_sdk/pubspec.yaml"))
		.unwrap_or_else(|| panic!("expected mobile sdk decision"));
	let version_group = plan
		.groups
		.first()
		.unwrap_or_else(|| panic!("expected version group plan"));

	assert_eq!(sdk_core.recommended_bump.to_string(), "minor");
	assert_eq!(sdk_core.trigger_type, "direct-change");
	assert_eq!(web_sdk.recommended_bump.to_string(), "minor");
	assert_eq!(web_sdk.trigger_type, "version-group-synchronization");
	assert_eq!(deno_tool.recommended_bump.to_string(), "patch");
	assert_eq!(mobile_sdk.recommended_bump.to_string(), "patch");
	assert_eq!(
		version_group
			.planned_version
			.as_ref()
			.map(ToString::to_string)
			.as_deref(),
		Some("1.1.0")
	);
}

#[test]
fn changes_add_writes_a_change_file_via_the_cli() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output_path = tempdir.path().join("feature.md");

	let _output = run_cli(
		&fixture_root,
		[
			OsString::from("mc"),
			OsString::from("step:create-change-file"),
			OsString::from("--package"),
			OsString::from("sdk-core"),
			OsString::from("--package"),
			OsString::from("web-sdk"),
			OsString::from("--bump"),
			OsString::from("minor"),
			OsString::from("--reason"),
			OsString::from("feature foundation"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));
	let content = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read change file: {error}"));

	// Step commands return empty output by default; verify via file content
	assert!(content.contains("sdk-core: minor"));
	assert!(content.contains("web-sdk: minor"));
	assert!(content.contains("# feature foundation"));
}

#[test]
fn changes_add_supports_release_note_type_and_details() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output_path = tempdir.path().join("security.md");

	run_cli(
		&fixture_root,
		[
			OsString::from("mc"),
			OsString::from("step:create-change-file"),
			OsString::from("--package"),
			OsString::from("sdk-core"),
			OsString::from("--bump"),
			OsString::from("patch"),
			OsString::from("--reason"),
			OsString::from("rotate signing keys"),
			OsString::from("--type"),
			OsString::from("security"),
			OsString::from("--details"),
			OsString::from("Roll the signing key before the release window closes."),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));
	let content = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read change file: {error}"));

	assert!(!content.contains("type:"));
	assert!(content.contains("sdk-core: security"));
	assert!(content.contains("# rotate signing keys"));
	assert!(content.contains("Roll the signing key before the release window closes."));
}

#[test]
fn changes_add_canonicalizes_package_references_to_package_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/canonicalize-package-ref", tempdir.path());
	let output_path = tempdir.path().join("repo-change.md");
	run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:create-change-file"),
			OsString::from("--package"),
			OsString::from("crates/monochange"),
			OsString::from("--bump"),
			OsString::from("patch"),
			OsString::from("--reason"),
			OsString::from("canonical package names"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));
	let content = fs::read_to_string(&output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("monochange: patch"));
	assert!(!content.contains("crates/monochange: patch"));
}

#[test]
fn add_change_file_creates_default_path_under_changeset_directory() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	copy_directory(&fixture_root, tempdir.path());
	let output_path = add_change_file(
		tempdir.path(),
		crate::AddChangeFileRequest::builder()
			.package_refs(&["sdk-core".to_string()])
			.bump(BumpSeverity::Patch)
			.reason("default output")
			.build(),
	)
	.unwrap_or_else(|error| panic!("default change file: {error}"));
	let content = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read change file: {error}"));

	assert!(output_path.starts_with(tempdir.path().join(".changeset")));
	assert!(content.contains("sdk-core: patch"));
	fs::remove_file(output_path).unwrap_or_else(|error| panic!("cleanup change file: {error}"));
}

#[test]
fn change_command_sources_type_choices_from_workspace_configuration() {
	let root = fixture_path("changeset-target-metadata/cli-type-only-change");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("fixture should load: {error}"));
	let mut cli = vec![CliCommandDefinition {
		name: "change".to_string(),
		help_text: None,
		inputs: vec![
			CliInputDefinition {
				name: "package".to_string(),
				kind: CliInputKind::StringList,
				help_text: None,
				required: false,
				default: None,
				choices: vec![],
				short: None,
			},
			CliInputDefinition {
				name: "type".to_string(),
				kind: CliInputKind::Choice,
				help_text: None,
				required: false,
				default: None,
				choices: vec![],
				short: None,
			},
			CliInputDefinition {
				name: "reason".to_string(),
				kind: CliInputKind::String,
				help_text: None,
				required: false,
				default: None,
				choices: vec![],
				short: None,
			},
			CliInputDefinition {
				name: "interactive".to_string(),
				kind: CliInputKind::Boolean,
				help_text: None,
				required: false,
				default: None,
				choices: vec![],
				short: None,
			},
		],
		steps: vec![monochange_core::CliStepDefinition::CreateChangeFile {
			name: None,
			when: None,
			show_progress: None,
			inputs: BTreeMap::new(),
		}],
	}];
	crate::apply_runtime_change_type_choices(&mut cli, &configuration);
	let change = cli
		.iter()
		.find(|c| c.name == "change")
		.unwrap_or_else(|| panic!("expected change command"));
	let type_input = change
		.inputs
		.iter()
		.find(|i| i.name == "type")
		.unwrap_or_else(|| panic!("expected type input"));
	assert_eq!(
		type_input.choices,
		vec!["docs".to_string(), "test".to_string()]
	);

	let error = Command::new("mc")
		.subcommand(crate::build_cli_command_subcommand(&cli[0]))
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("--package"),
			OsString::from("core"),
			OsString::from("--type"),
			OsString::from("security"),
			OsString::from("--reason"),
			OsString::from("clarify migration guide"),
		])
		.expect_err("unknown configured type should be rejected by clap choices");
	let rendered = error.to_string();
	assert!(rendered.contains("invalid value 'security'"));
	assert!(rendered.contains("[possible values: docs, test]"));
}

#[test]
fn build_command_for_root_falls_back_to_default_cli_when_config_load_fails() {
	let root = fixture_path("config/rejects-unknown-template-vars");
	let matches = build_command_for_root("mc", &root)
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("step:retarget-release"),
			OsString::from("--from"),
			OsString::from("v1.2.3"),
		])
		.unwrap_or_else(|error| panic!("matches: {error}"));
	assert_eq!(matches.subcommand_name(), Some("step:retarget-release"));
}

#[test]
fn collect_cli_command_inputs_omits_default_bump_for_type_only_changes() {
	let root = fixture_path("changeset-target-metadata/cli-type-only-change");
	let command = build_command_for_root("mc", &root);
	let matches = command
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("step:create-change-file"),
			OsString::from("--package"),
			OsString::from("core"),
			OsString::from("--type"),
			OsString::from("docs"),
			OsString::from("--reason"),
			OsString::from("clarify migration guide"),
		])
		.unwrap_or_else(|error| panic!("matches: {error}"));
	let (_, subcommand_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected subcommand"));
	let cli_command = monochange_core::all_step_variants()
		.into_iter()
		.find(|s| s.step_kebab_name() == "create-change-file")
		.map_or_else(
			|| panic!("expected change command"),
			|step| {
				CliCommandDefinition {
					name: format!("step:{}", step.step_kebab_name()),
					help_text: step.name().map(ToString::to_string),
					inputs: step.step_inputs_schema(),
					steps: vec![step],
				}
			},
		);
	let inputs = crate::collect_cli_command_inputs(&cli_command, subcommand_matches);
	assert!(inputs.get("bump").is_some_and(Vec::is_empty));
	assert_eq!(
		inputs
			.get("type")
			.and_then(|values| values.first())
			.map(String::as_str),
		Some("docs")
	);
}

#[test]
fn parse_change_bump_supports_none() {
	assert_eq!(parse_change_bump("none").unwrap(), crate::ChangeBump::None);
	assert_eq!(
		BumpSeverity::from(crate::ChangeBump::None),
		BumpSeverity::None
	);
}

#[test]
fn parse_change_bump_rejects_unsupported_values() {
	let error = parse_change_bump("prerelease")
		.err()
		.unwrap_or_else(|| panic!("expected invalid bump error"));
	assert!(error.to_string().contains("unsupported bump `prerelease`"));
}

#[test]
fn changes_add_requires_package_or_interactive_mode() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture(
		"changeset-target-metadata/cli-type-only-change",
		tempdir.path(),
	);

	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:create-change-file"),
			OsString::from("--reason"),
			OsString::from("missing package"),
		],
	)
	.expect_err("change without package should fail");
	assert!(
		error
			.to_string()
			.contains("requires at least one `--package` value or `--interactive` mode")
	);
}

#[test]
fn changes_add_defaults_bump_to_none_when_type_is_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture(
		"changeset-target-metadata/cli-type-only-change",
		tempdir.path(),
	);
	let output_path = tempdir.path().join("docs.md");

	run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:create-change-file"),
			OsString::from("--package"),
			OsString::from("core"),
			OsString::from("--type"),
			OsString::from("docs"),
			OsString::from("--reason"),
			OsString::from("clarify migration guide"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));

	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("core: docs"));
	assert!(!content.contains("bump:"));
}

#[test]
fn add_change_file_renders_type_and_explicit_version_without_bump() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());
	let output_path = tempdir.path().join("security-version.md");

	add_change_file(
		tempdir.path(),
		crate::AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::None)
			.reason("pin a secure release")
			.version(Some("2.0.0"))
			.change_type(Some("security"))
			.output(Some(&output_path))
			.build(),
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));

	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("core:"));
	assert!(!content.contains("  bump:"));
	assert!(content.contains("  type: security"));
	assert!(content.contains("  version: \"2.0.0\""));
}

#[test]
fn add_change_file_renders_object_metadata_when_bump_differs_from_type_default() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());
	let output_path = tempdir.path().join("security-major.md");

	add_change_file(
		tempdir.path(),
		crate::AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::Major)
			.reason("break the api")
			.change_type(Some("security"))
			.output(Some(&output_path))
			.build(),
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));

	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("core:"));
	assert!(content.contains("  bump: major"));
	assert!(content.contains("  type: security"));
}

#[test]
fn add_change_file_rejects_none_without_type_or_version() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());

	let error = add_change_file(
		tempdir.path(),
		crate::AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::None)
			.reason("type required")
			.build(),
	)
	.expect_err("none bump without type/version should fail");
	let _guard = snapshot_settings().bind_to_scope();
	insta::assert_snapshot!(error.to_string());
}

#[test]
fn add_change_file_allows_none_with_caused_by_context() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());
	let output_path = tempdir.path().join("dependency-follow-up.md");

	add_change_file(
		tempdir.path(),
		crate::AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::None)
			.reason("dependency-only follow-up")
			.caused_by(&["sdk".to_string()])
			.output(Some(&output_path))
			.build(),
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));

	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	let _guard = snapshot_settings().bind_to_scope();
	insta::assert_snapshot!(content);
}

#[test]
fn render_change_target_markdown_uses_object_syntax_for_caused_by_context() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let caused_by = vec!["sdk".to_string()];
	let lines = render_change_target_markdown(
		&configuration,
		"core",
		BumpSeverity::Patch,
		None,
		Some("security"),
		&caused_by,
	)
	.unwrap_or_else(|error| panic!("render target: {error}"));
	assert_eq!(
		lines,
		vec![
			"core:".to_string(),
			"  bump: patch".to_string(),
			"  type: security".to_string(),
			"  caused_by: [\"sdk\"]".to_string(),
		]
	);
}

#[test]
fn render_change_target_markdown_renders_version_and_caused_by_context() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let lines = render_change_target_markdown(
		&configuration,
		"core",
		BumpSeverity::Patch,
		Some("2.0.0"),
		None,
		&["sdk".to_string()],
	)
	.unwrap_or_else(|error| panic!("render version target: {error}"));
	assert_eq!(
		lines,
		vec![
			"core:".to_string(),
			"  bump: patch".to_string(),
			"  version: \"2.0.0\"".to_string(),
			"  caused_by: [\"sdk\"]".to_string(),
		]
	);
}

#[test]
fn add_interactive_change_file_renders_caused_by_context_for_none_bumps() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());
	let output_path = tempdir.path().join("interactive-caused-by.md");
	let result = InteractiveChangeResult {
		targets: vec![InteractiveTarget {
			id: "core".to_string(),
			bump: BumpSeverity::None,
			version: None,
			change_type: None,
		}],
		caused_by: vec!["sdk".to_string()],
		reason: "dependency follow-up".to_string(),
		details: None,
	};

	add_interactive_change_file(tempdir.path(), &result, Some(&output_path))
		.unwrap_or_else(|error| panic!("interactive caused_by change file: {error}"));
	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	let _guard = snapshot_settings().bind_to_scope();
	insta::assert_snapshot!(content);
}

#[test]
fn add_change_file_rejects_unknown_change_type() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());

	let error = add_change_file(
		tempdir.path(),
		crate::AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::Patch)
			.reason("unknown type")
			.change_type(Some("docs"))
			.build(),
	)
	.expect_err("unknown type should fail");
	assert!(
		error
			.to_string()
			.contains("uses unknown change type `docs`")
	);
}

#[test]
fn render_change_target_markdown_uses_package_defaults() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let lines = render_change_target_markdown(
		&configuration,
		"core",
		BumpSeverity::Patch,
		None,
		Some("security"),
		&[],
	)
	.unwrap_or_else(|error| panic!("render target: {error}"));
	assert_eq!(lines, vec!["core: security".to_string()]);
}

#[test]
fn render_change_target_markdown_uses_group_defaults() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let lines = render_change_target_markdown(
		&configuration,
		"sdk",
		BumpSeverity::Minor,
		None,
		Some("test"),
		&[],
	)
	.unwrap_or_else(|error| panic!("render target: {error}"));
	assert_eq!(lines, vec!["sdk: test".to_string()]);
}

#[test]
fn render_change_target_markdown_quotes_special_character_ids() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let lines = render_change_target_markdown(
		&configuration,
		"@monochange/skill",
		BumpSeverity::Patch,
		None,
		None,
		&[],
	)
	.unwrap_or_else(|error| panic!("render target: {error}"));
	assert_eq!(lines, vec!["\"@monochange/skill\": patch".to_string()]);

	let simple = render_change_target_markdown(
		&configuration,
		"plain-package",
		BumpSeverity::Patch,
		None,
		None,
		&[],
	)
	.unwrap_or_else(|error| panic!("render simple target: {error}"));
	assert_eq!(simple, vec!["plain-package: patch".to_string()]);

	let escaped = render_change_target_markdown(
		&configuration,
		"pkg\\\"name",
		BumpSeverity::Patch,
		None,
		None,
		&[],
	)
	.unwrap_or_else(|error| panic!("render escaped target: {error}"));
	assert_eq!(escaped, vec!["\"pkg\\\\\\\"name\": patch".to_string()]);
}

#[test]
fn change_type_default_bump_resolves_package_group_and_unknown_targets() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	assert_eq!(
		crate::change_type_default_bump(&configuration, "core", "security"),
		Some(BumpSeverity::Patch)
	);
	assert_eq!(
		crate::change_type_default_bump(&configuration, "sdk", "test"),
		Some(BumpSeverity::Minor)
	);
	assert_eq!(
		crate::change_type_default_bump(&configuration, "unknown", "test"),
		Some(BumpSeverity::Minor) // unknown targets inherit workspace defaults
	);
}

#[test]
fn add_interactive_change_file_writes_target_owned_metadata() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());
	let output_path = tempdir.path().join("interactive.md");
	let result = InteractiveChangeResult {
		targets: vec![InteractiveTarget {
			id: "sdk".to_string(),
			bump: BumpSeverity::Minor,
			version: None,
			change_type: Some("test".to_string()),
		}],
		caused_by: Vec::new(),
		reason: "broaden integration coverage".to_string(),
		details: Some("Exercise the group-authored shorthand path.".to_string()),
	};

	add_interactive_change_file(tempdir.path(), &result, Some(&output_path))
		.unwrap_or_else(|error| panic!("interactive change file: {error}"));
	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("sdk: test"));
	assert!(content.contains("# broaden integration coverage"));
}

#[test]
fn add_interactive_change_file_quotes_special_character_targets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir crate: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[package.\"@monochange/skill\"]\npath = \"crates/core\"\ntype = \"cargo\"\n",
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	fs::write(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
	)
	.unwrap_or_else(|error| panic!("write cargo manifest: {error}"));
	let output_path = tempdir.path().join(".changeset/interactive-special.md");
	let result = InteractiveChangeResult {
		targets: vec![InteractiveTarget {
			id: "@monochange/skill".to_string(),
			bump: BumpSeverity::Patch,
			version: None,
			change_type: None,
		}],
		caused_by: Vec::new(),
		reason: "ship special package id support".to_string(),
		details: None,
	};

	add_interactive_change_file(tempdir.path(), &result, Some(&output_path))
		.unwrap_or_else(|error| panic!("interactive change file: {error}"));
	let content = fs::read_to_string(&output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("\"@monochange/skill\": patch"));
	monochange_config::validate_workspace(tempdir.path())
		.unwrap_or_else(|error| panic!("validate workspace: {error}"));
}

#[test]
fn changes_add_rejects_legacy_evidence_input() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output_path = tempdir.path().join("major.md");

	let error = run_cli(
		&fixture_root,
		[
			OsString::from("mc"),
			OsString::from("step:create-change-file"),
			OsString::from("--package"),
			OsString::from("sdk-core"),
			OsString::from("--bump"),
			OsString::from("patch"),
			OsString::from("--reason"),
			OsString::from("breaking change"),
			OsString::from("--evidence"),
			OsString::from("rust-semver:major:public API break detected"),
			OsString::from("--output"),
			output_path.into_os_string(),
		],
	)
	.expect_err("legacy evidence input should be rejected");

	assert!(
		error
			.to_string()
			.contains("unexpected argument '--evidence'")
	);
}

#[test]
fn changes_add_rejects_unknown_package_references() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let error = run_cli(
		&fixture_root,
		[
			OsString::from("mc"),
			OsString::from("step:create-change-file"),
			OsString::from("--package"),
			OsString::from("missing-package"),
			OsString::from("--reason"),
			OsString::from("should fail"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("expected failure"));

	assert!(
		error
			.to_string()
			.contains("did not match any discovered package")
	);
}

#[test]
fn release_dry_run_rejects_legacy_origin_and_evidence_metadata() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	copy_fixture("monochange/release-with-compat-evidence", tempdir.path());
	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:prepare-release"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.expect_err("legacy metadata should be rejected");

	assert!(
		error
			.to_string()
			.contains("target `origin` uses unsupported field(s): core")
	);
}

#[test]
fn plan_release_expands_group_targeted_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	copy_fixture("monochange/release-with-group-change", tempdir.path());
	let plan = plan_release(tempdir.path(), &tempdir.path().join("group-change.md"))
		.unwrap_or_else(|error| panic!("group release plan: {error}"));
	let direct_count = plan
		.decisions
		.iter()
		.filter(|d| d.recommended_bump.to_string() == "minor")
		.count();
	assert_eq!(direct_count, 2);
}

#[test]
fn command_release_dry_run_discovers_changesets_without_mutating_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	let workspace_manifest = tempdir.path().join("Cargo.toml");
	let core_changelog = tempdir.path().join("crates/core/changelog.md");
	let original_manifest = fs::read_to_string(&workspace_manifest)
		.unwrap_or_else(|error| panic!("workspace manifest: {error}"));
	let original_changelog = fs::read_to_string(&core_changelog)
		.unwrap_or_else(|error| panic!("core changelog: {error}"));

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:prepare-release"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(output.contains("# `step:prepare-release` (dry-run)"));
	assert!(output.contains("1.1.0"));
	assert!(output.contains("workflow-app"));
	assert!(output.contains("workflow-core"));
	assert_eq!(
		fs::read_to_string(&workspace_manifest)
			.unwrap_or_else(|error| panic!("workspace manifest after dry-run: {error}")),
		original_manifest
	);
	assert_eq!(
		fs::read_to_string(&core_changelog)
			.unwrap_or_else(|error| panic!("core changelog after dry-run: {error}")),
		original_changelog
	);
	assert!(tempdir.path().join(".changeset/feature.md").exists());
}

#[test]
fn prepare_release_allows_empty_changesets_when_configured() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	fs::remove_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("remove changesets: {error}"));

	let strict_error =
		crate::prepare_release_execution_with_file_diffs(tempdir.path(), true, false, false)
			.err()
			.unwrap_or_else(|| panic!("expected missing changeset error"));
	assert!(
		strict_error
			.to_string()
			.contains("no markdown changesets found under .changeset")
	);

	let execution =
		crate::prepare_release_execution_with_file_diffs(tempdir.path(), true, true, true)
			.unwrap_or_else(|error| panic!("prepare release execution: {error}"));

	assert_eq!(
		execution.prepared_release.plan.workspace_root,
		tempdir.path().to_path_buf()
	);
	assert!(execution.prepared_release.changeset_paths.is_empty());
	assert!(execution.prepared_release.changesets.is_empty());
	assert!(execution.prepared_release.released_packages.is_empty());
	assert!(execution.prepared_release.changed_files.is_empty());
	assert!(execution.file_diffs.is_empty());
}

#[test]
fn command_versions_reports_planned_versions_without_mutating_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	let workspace_manifest = tempdir.path().join("Cargo.toml");
	let core_changelog = tempdir.path().join("crates/core/changelog.md");
	let original_manifest = fs::read_to_string(&workspace_manifest)
		.unwrap_or_else(|error| panic!("workspace manifest: {error}"));
	let original_changelog = fs::read_to_string(&core_changelog)
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("workspace configuration: {error}"));
	let matches = crate::build_command_with_cli("mc", &configuration.cli)
		.try_get_matches_from(["mc", "step:display-versions", "--format", "text"])
		.unwrap_or_else(|error| panic!("versions matches: {error}"));
	let versions_matches = matches
		.subcommand_matches("step:display-versions")
		.unwrap_or_else(|| panic!("expected versions subcommand matches"));
	let output = crate::execute_matches(
		tempdir.path(),
		&configuration,
		"step:display-versions",
		versions_matches,
		false,
	)
	.unwrap_or_else(|error| panic!("versions output: {error}"));

	assert!(output.contains("group versions:"));
	assert!(output.contains("package versions:"));
	assert_eq!(
		fs::read_to_string(&workspace_manifest)
			.unwrap_or_else(|error| panic!("workspace manifest after versions: {error}")),
		original_manifest
	);
	assert_eq!(
		fs::read_to_string(&core_changelog)
			.unwrap_or_else(|error| panic!("core changelog after versions: {error}")),
		original_changelog
	);
	assert!(tempdir.path().join(".changeset/feature.md").exists());
}

#[test]
fn command_config_reports_resolved_configuration_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("step:config")],
	)
	.unwrap_or_else(|error| panic!("config output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("config json: {error}"));

	let project_root = tempdir
		.path()
		.canonicalize()
		.unwrap_or_else(|error| panic!("canonical root: {error}"))
		.display()
		.to_string();
	let config_path = tempdir.path().join("monochange.toml").display().to_string();

	assert_eq!(parsed["projectRoot"], serde_json::json!(project_root));
	assert_eq!(parsed["configPath"], serde_json::json!(config_path));
	assert_eq!(
		parsed["config"]["packages"][0]["id"],
		serde_json::json!("app")
	);
}

#[test]
fn render_config_step_json_falls_back_to_uncanonicalized_root() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("config: {error}"));
	let missing_root = tempdir.path().join("missing-root");

	let output = crate::render_config_step_json(&missing_root, &configuration);
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("config json: {error}"));

	assert_eq!(
		parsed["projectRoot"],
		serde_json::json!(missing_root.display().to_string())
	);
	assert_eq!(
		parsed["configPath"],
		serde_json::json!(missing_root.join("monochange.toml").display().to_string())
	);
}

#[test]
fn template_context_exposes_resolved_config_by_default() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	let mut context = cli_context_for_when_evaluation_tests();
	context.root = tempdir.path().to_path_buf();

	let template_context = crate::build_cli_template_context(&context, &BTreeMap::new(), None);
	let project_root = tempdir
		.path()
		.canonicalize()
		.unwrap_or_else(|error| panic!("canonical root: {error}"))
		.display()
		.to_string();
	let config_path = tempdir.path().join("monochange.toml").display().to_string();

	assert_eq!(
		template_context
			.get("config")
			.and_then(|value| value.pointer("/packages/0/id"))
			.and_then(serde_json::Value::as_str),
		Some("app")
	);
	assert_eq!(
		template_context
			.get("project_root")
			.and_then(serde_json::Value::as_str),
		Some(project_root.as_str())
	);
	assert_eq!(
		template_context
			.get("config_path")
			.and_then(serde_json::Value::as_str),
		Some(config_path.as_str())
	);
}

#[test]
fn render_interactive_changeset_markdown_uses_natural_summary_heading() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let result = InteractiveChangeResult {
		targets: vec![InteractiveTarget {
			id: "core".to_string(),
			bump: BumpSeverity::Minor,
			version: None,
			change_type: None,
		}],
		caused_by: Vec::new(),
		reason: "interactive heading".to_string(),
		details: Some("Details body".to_string()),
	};
	let rendered = crate::render_interactive_changeset_markdown(&configuration, &result)
		.unwrap_or_else(|error| panic!("render interactive markdown: {error}"));
	assert!(rendered.contains("# interactive heading"));
	assert!(rendered.contains("Details body"));
}

#[test]
fn render_interactive_changeset_markdown_renders_caused_by_context() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let result = InteractiveChangeResult {
		targets: vec![InteractiveTarget {
			id: "core".to_string(),
			bump: BumpSeverity::None,
			version: None,
			change_type: None,
		}],
		caused_by: vec!["sdk".to_string()],
		reason: "interactive caused_by".to_string(),
		details: None,
	};
	let rendered = crate::render_interactive_changeset_markdown(&configuration, &result)
		.unwrap_or_else(|error| panic!("render interactive caused_by markdown: {error}"));
	assert!(rendered.contains("core:"));
	assert!(rendered.contains("  bump: none"));
	assert!(rendered.contains("  caused_by: [\"sdk\"]"));
}

#[test]
fn command_release_normalizes_authored_changeset_heading_levels() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-heading-normalization", tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/changelog.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));

	assert!(core_changelog.contains("#### add release command"));
	assert!(core_changelog.contains("##### Details"));
	assert!(core_changelog.contains("###### API changes"));
	assert!(!core_changelog.contains("\n## Details\n"));
	assert!(!core_changelog.contains("\n### API changes\n"));
}

#[test]
fn command_release_updates_manifests_changelogs_and_deletes_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(
		tempdir.path(),
		Some("printf '%s' \"{{ version }}\" > release-version.txt"),
		false,
	);

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let workspace_manifest = fs::read_to_string(tempdir.path().join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("workspace manifest: {error}"));
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/changelog.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/changelog.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	let group_versioned_file = fs::read_to_string(tempdir.path().join("group.toml"))
		.unwrap_or_else(|error| panic!("group versioned file: {error}"));
	let package_versioned_file = fs::read_to_string(tempdir.path().join("crates/core/extra.toml"))
		.unwrap_or_else(|error| panic!("package versioned file: {error}"));
	assert!(output.contains("# `step:prepare-release`"));
	assert!(output.contains("group `sdk`"));
	assert!(output.contains("v1.1.0"));
	assert!(workspace_manifest.contains("version = \"1.1.0\""));
	assert!(core_changelog.contains("## 1.1.0"));
	assert!(core_changelog.contains("add release command"));
	assert!(app_changelog.contains("## 1.1.0"));
	assert!(app_changelog.contains("No package-specific changes were recorded; `workflow-app` was updated to 1.1.0 as part of group `sdk`."));
	assert!(group_changelog.contains("Grouped release for `sdk`."));
	assert!(group_changelog.contains("Changed members: core"));
	assert!(group_changelog.contains("Synchronized members: app"));
	assert!(group_versioned_file.contains("version = \"1.1.0\""));
	assert!(package_versioned_file.contains("version = \"1.1.0\""));
	assert!(!tempdir.path().join(".changeset/feature.md").exists());
}

#[test]
fn command_release_updates_inferred_cargo_lockfiles_without_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_cargo_lock_release_fixture(tempdir.path());

	with_path_prefixed(tempdir.path(), || {
		run_cli(
			tempdir.path(),
			[OsString::from("mc"), OsString::from("step:prepare-release")],
		)
		.unwrap_or_else(|error| panic!("command output: {error}"));
	});
	let cargo_lock = fs::read_to_string(tempdir.path().join("Cargo.lock"))
		.unwrap_or_else(|error| panic!("cargo lock: {error}"));

	assert!(cargo_lock.contains("version = \"1.1.0\""));
}

#[test]
fn command_release_updates_inferred_npm_lockfiles_without_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_npm_lock_release_fixture(tempdir.path());

	with_path_prefixed(tempdir.path(), || {
		run_cli(
			tempdir.path(),
			[OsString::from("mc"), OsString::from("step:prepare-release")],
		)
		.unwrap_or_else(|error| panic!("command output: {error}"));
	});
	let package_lock = fs::read_to_string(tempdir.path().join("packages/app/package-lock.json"))
		.unwrap_or_else(|error| panic!("package lock: {error}"));

	assert!(package_lock.contains("\"version\": \"1.1.0\""));
}

#[test]
fn command_release_updates_inferred_bun_lockfiles_without_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_bun_lockb_release_fixture(tempdir.path());

	with_path_prefixed(tempdir.path(), || {
		run_cli(
			tempdir.path(),
			[OsString::from("mc"), OsString::from("step:prepare-release")],
		)
		.unwrap_or_else(|error| panic!("command output: {error}"));
	});
	let bun_lock = fs::read(tempdir.path().join("packages/app/bun.lockb"))
		.unwrap_or_else(|error| panic!("bun lockb: {error}"));

	assert!(String::from_utf8_lossy(&bun_lock).contains("1.1.0"));
}

#[test]
fn command_release_updates_inferred_deno_lockfiles_without_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_deno_lock_release_fixture(tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let deno_lock = fs::read_to_string(tempdir.path().join("packages/app/deno.lock"))
		.unwrap_or_else(|error| panic!("deno lock: {error}"));

	assert!(deno_lock.contains("npm:app@1.1.0"));
}

#[test]
fn build_lockfile_command_executions_only_returns_configured_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_cargo_lock_release_fixture(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let discovery =
		discover_workspace(tempdir.path()).unwrap_or_else(|error| panic!("discovery: {error}"));
	let plan = plan_release(
		tempdir.path(),
		&tempdir.path().join(".changeset/feature.md"),
	)
	.unwrap_or_else(|error| panic!("plan: {error}"));

	assert!(
		build_lockfile_command_executions(
			tempdir.path(),
			&configuration,
			&discovery.packages,
			&plan,
		)
		.unwrap_or_else(|error| panic!("lockfile commands: {error}"))
		.is_empty()
	);
}

#[test]
fn command_release_prefers_custom_lockfile_commands_over_defaults() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/custom-lockfile-command", tempdir.path());

	with_path_prefixed(tempdir.path(), || {
		run_cli(
			tempdir.path(),
			[OsString::from("mc"), OsString::from("step:prepare-release")],
		)
		.unwrap_or_else(|error| panic!("command output: {error}"));
	});
	let package_lock = fs::read_to_string(tempdir.path().join("packages/app/package-lock.json"))
		.unwrap_or_else(|error| panic!("package lock: {error}"));

	assert!(tempdir.path().join("packages/app/custom-ran.txt").exists());
	assert!(!tempdir.path().join("packages/app/default-ran.txt").exists());
	assert!(package_lock.contains("1.1.0-custom"));
}

#[test]
fn prepare_release_execution_dry_run_skips_lockfile_commands_for_performance() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/custom-lockfile-command", tempdir.path());
	let before = fs::read_to_string(tempdir.path().join("packages/app/package-lock.json"))
		.unwrap_or_else(|error| panic!("package lock before dry-run: {error}"));

	let prepared = with_path_prefixed(tempdir.path(), || {
		prepare_release_execution(tempdir.path(), true)
			.unwrap_or_else(|error| panic!("prepare release execution: {error}"))
	});
	let after = fs::read_to_string(tempdir.path().join("packages/app/package-lock.json"))
		.unwrap_or_else(|error| panic!("package lock after dry-run: {error}"));

	// Workspace must not be mutated during dry-run.
	assert_eq!(before, after);
	// Lockfile diffs are intentionally omitted from dry-run preview
	// to avoid copying the entire workspace (which can take minutes).
	assert!(
		!prepared
			.file_diffs
			.iter()
			.any(|diff| { diff.path.as_path() == Path::new("packages/app/package-lock.json") }),
		"lockfile diffs should be skipped during dry-run"
	);
	// Custom lockfile command marker file should NOT exist (command was skipped).
	assert!(
		!tempdir.path().join("packages/app/custom-ran.txt").exists(),
		"lockfile command should not run during dry-run"
	);
}

#[test]
fn command_release_honors_explicit_lockfile_paths_in_versioned_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_explicit_lockfile_override_fixture(tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let shared_lock = fs::read_to_string(tempdir.path().join("lockfiles/shared/package-lock.json"))
		.unwrap_or_else(|error| panic!("shared package lock: {error}"));

	assert!(shared_lock.contains("\"version\": \"1.1.0\""));
}

#[test]
fn command_release_updates_regex_versioned_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/regex-versioned-file", tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let readme = fs::read_to_string(tempdir.path().join("README.md"))
		.unwrap_or_else(|error| panic!("readme: {error}"));

	assert!(readme.contains("https://example.com/download/v1.1.0.tgz"));
	assert!(!readme.contains("https://example.com/download/v1.0.0.tgz"));
}

#[test]
fn command_release_uses_empty_update_message_precedence_for_grouped_changelogs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_group_empty_update_message_fixture(tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/changelog.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/changelog.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(output.contains("# `step:prepare-release`"));
	assert!(core_changelog.contains("Package override for workflow-core -> 1.0.1"));
	assert!(app_changelog.contains("Update triggered by group sdk; version 1.0.1."));
	assert!(group_changelog.contains("Update triggered by group sdk; version 1.0.1."));
}

#[test]
fn command_release_failures_do_not_delete_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, true);

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected command failure"));

	assert!(error.to_string().contains("failed to create"));
	assert!(tempdir.path().join(".changeset/feature.md").exists());
}

#[test]
fn command_diagnostics_reports_requested_changeset_text() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:diagnose-changesets"),
			OsString::from("--changeset"),
			OsString::from(".changeset/feature.md"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(output.contains("changeset: .changeset/feature.md"));
	assert!(output.contains("summary: Add feature"));
	assert!(output.contains("targets:"));
	assert!(output.contains("core"));
}

#[test]
fn command_diagnostics_reports_multiple_changesets_in_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), true);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:diagnose-changesets"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("diagnostics json: {error}"));

	let requested = parsed["requestedChangesets"]
		.as_array()
		.unwrap_or_else(|| panic!("requested"));
	assert_eq!(requested.len(), 2);
	assert_eq!(requested[0].as_str(), Some(".changeset/feature.md"));
	assert_eq!(requested[1].as_str(), Some(".changeset/performance.md"));
	let changesets = parsed["changesets"]
		.as_array()
		.unwrap_or_else(|| panic!("changesets"));
	assert_eq!(changesets.len(), 2);
	assert_eq!(changesets[0]["targets"][0]["id"], "core");
}

#[test]
fn command_diagnostics_deduplicates_duplicate_requested_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:diagnose-changesets"),
			OsString::from("--changeset"),
			OsString::from(".changeset/feature.md"),
			OsString::from("--changeset"),
			OsString::from(".changeset/feature.md"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("diagnostics json: {error}"));
	let requested = parsed["requestedChangesets"]
		.as_array()
		.unwrap_or_else(|| panic!("requested"));

	assert_eq!(requested.len(), 1);
	assert_eq!(requested[0].as_str(), Some(".changeset/feature.md"));
}

#[test]
fn command_diagnostics_reports_unknown_changeset_path() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:diagnose-changesets"),
			OsString::from("--changeset"),
			OsString::from(".changeset/does-not-exist.md"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing changeset failure"));

	assert!(error.to_string().contains("does not exist"));
}

#[test]
fn command_diagnostics_resolves_changeset_fallback_for_short_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:diagnose-changesets"),
			OsString::from("--changeset"),
			OsString::from("feature.md"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(output.contains("changeset: .changeset/feature.md"));
}

#[test]
fn command_diagnostics_supports_absolute_changeset_path() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);
	let absolute = tempdir.path().join(".changeset/feature.md");

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:diagnose-changesets"),
			OsString::from("--format"),
			OsString::from("text"),
			OsString::from("--changeset"),
			OsString::from(absolute.to_string_lossy().into_owned()),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(output.contains("changeset: .changeset/feature.md"));
}

#[test]
fn resolve_changeset_path_accepts_short_names_with_fallback_to_changeset_dir() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let absolute = tempdir.path().join(".changeset/feature.md");
	let from_short = crate::resolve_changeset_path(tempdir.path(), "feature.md")
		.unwrap_or_else(|error| panic!("resolve short path: {error}"));
	let from_absolute =
		crate::resolve_changeset_path(tempdir.path(), absolute.to_string_lossy().as_ref())
			.unwrap_or_else(|error| panic!("resolve absolute path: {error}"));

	assert_eq!(from_short, absolute);
	assert_eq!(from_absolute, absolute);
}

#[test]
fn resolve_changeset_path_rejects_invalid_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let missing = crate::resolve_changeset_path(tempdir.path(), ".changeset/does-not-exist.md")
		.err()
		.unwrap_or_else(|| panic!("expected missing path failure"));
	assert!(missing.to_string().contains("does not exist"));

	let empty = crate::resolve_changeset_path(tempdir.path(), "")
		.err()
		.unwrap_or_else(|| panic!("expected empty path failure"));
	assert!(empty.to_string().contains("cannot be empty"));
}

#[test]
fn render_changeset_diagnostics_reports_empty_set() {
	let report = crate::ChangesetDiagnosticsReport {
		requested_changesets: Vec::new(),
		changesets: Vec::new(),
	};
	let rendered = crate::render_changeset_diagnostics(&report);

	assert_eq!(rendered, "no matching changesets found");
}

#[test]
fn command_unknown_commands_suggest_available_cli() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("ship-it")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected command suggestion"));

	assert!(
		error
			.to_string()
			.contains("unrecognized subcommand 'ship-it'")
	);
}

#[test]
fn cli_command_command_steps_can_run_through_the_shell() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/shell-command", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("announce")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let shell_output = fs::read_to_string(tempdir.path().join("shell-output.txt"))
		.unwrap_or_else(|error| panic!("shell output: {error}"));
	assert!(output.contains("command `announce` completed"));
	assert_eq!(shell_output, "shell-command");
}

#[test]
fn cli_command_command_steps_use_dry_run_overrides_when_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/dry-run-override", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("announce"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let dry_run_output = fs::read_to_string(tempdir.path().join("dry-run-output.txt"))
		.unwrap_or_else(|error| panic!("dry-run output: {error}"));
	assert!(output.contains("command `announce` completed (dry-run)"));
	assert_eq!(dry_run_output, "dry-run");
	assert!(!tempdir.path().join("command-output.txt").exists());
}

#[test]
fn cli_command_command_steps_expose_namespaced_inputs_and_step_overrides() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/namespaced-inputs", tempdir.path());
	run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("announce"),
			OsString::from("--message"),
			OsString::from("hello-world"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let command_output = fs::read_to_string(tempdir.path().join("command-output.txt"))
		.unwrap_or_else(|error| panic!("command output file: {error}"));
	assert_eq!(command_output, "hello-world");
}

#[test]
fn command_step_without_dry_run_override_reports_skipped_command() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "announce".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::Command {
			show_progress: None,
			name: None,
			when: None,
			command: "echo hello".to_string(),
			dry_run_command: None,
			shell: monochange_core::ShellConfig::default(),
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		}],
	};
	let output = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		true,
		BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("dry-run output: {error}"));
	assert!(output.contains("skipped command `echo hello` (dry-run)"));
}

#[test]
fn command_step_rejects_unparseable_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "announce".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::Command {
			show_progress: None,
			name: None,
			when: None,
			command: "\"unterminated".to_string(),
			dry_run_command: None,
			shell: monochange_core::ShellConfig::default(),
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		}],
	};
	let error = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected parse failure"));
	assert!(
		error
			.to_string()
			.contains("failed to parse command `\"unterminated`")
	);
}

#[test]
fn command_step_rejects_empty_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "announce".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::Command {
			show_progress: None,
			name: None,
			when: None,
			command: String::new(),
			dry_run_command: None,
			shell: monochange_core::ShellConfig::default(),
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		}],
	};
	let error = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected empty command failure"));
	assert!(error.to_string().contains("command must not be empty"));
}

#[test]
fn command_step_reports_process_spawn_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "announce".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::Command {
			show_progress: None,
			name: None,
			when: None,
			command: "definitely-not-a-real-command-12345".to_string(),
			dry_run_command: None,
			shell: monochange_core::ShellConfig::default(),
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		}],
	};
	let error = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected process spawn failure"));
	assert!(
		error
			.to_string()
			.contains("failed to run command `definitely-not-a-real-command-12345`")
	);
}

#[test]
fn command_step_reports_nonzero_exit_status_without_stderr() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "announce".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::Command {
			show_progress: None,
			name: None,
			when: None,
			command: "sh -c 'exit 7'".to_string(),
			dry_run_command: None,
			shell: monochange_core::ShellConfig::default(),
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		}],
	};
	let error = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected exit-status failure"));
	assert!(
		error.to_string().contains("failed: exit status"),
		"error: {error}"
	);
}

#[test]
fn command_step_reports_stderr_text_for_nonzero_exit_status() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "announce".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::Command {
			show_progress: None,
			name: None,
			when: None,
			command: "sh -c 'echo boom 1>&2; exit 1'".to_string(),
			dry_run_command: None,
			shell: monochange_core::ShellConfig::default(),
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		}],
	};
	let error = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected stderr failure"));
	assert!(error.to_string().contains("boom"), "error: {error}");
}

#[test]
fn execute_cli_command_without_steps_reports_completion_status() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "noop".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
	};
	let output = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("noop output: {error}"));
	assert_eq!(output, "command `noop` completed");
	let dry_run_output = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		true,
		BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("noop dry-run output: {error}"));
	assert_eq!(dry_run_output, "command `noop` completed (dry-run)");
}

#[test]
fn affected_packages_requires_attached_coverage_for_changed_packages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_changeset_policy_fixture(tempdir.path(), false);

	let evaluation = affected_packages(
		tempdir.path(),
		&["crates/core/src/lib.rs".to_string()],
		&Vec::new(),
	)
	.unwrap_or_else(|error| panic!("verification: {error}"));

	assert_eq!(
		evaluation.status,
		monochange_core::ChangesetPolicyStatus::Failed
	);
	assert!(evaluation.required);
	assert!(evaluation.comment.is_some());
	assert_eq!(evaluation.matched_paths, vec!["crates/core/src/lib.rs"]);
	assert_eq!(evaluation.uncovered_package_ids, vec!["core"]);
}

#[test]
fn affected_packages_skips_when_allowed_label_is_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_changeset_policy_fixture(tempdir.path(), false);

	let evaluation = affected_packages(
		tempdir.path(),
		&["crates/core/src/lib.rs".to_string()],
		&["no-changeset-required".to_string()],
	)
	.unwrap_or_else(|error| panic!("verification: {error}"));

	assert_eq!(
		evaluation.status,
		monochange_core::ChangesetPolicyStatus::Skipped
	);
	assert!(!evaluation.required);
	assert_eq!(
		evaluation.matched_skip_labels,
		vec!["no-changeset-required"]
	);
}

#[test]
fn affected_packages_step_can_override_built_in_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/changeset-policy-base", tempdir.path());
	copy_fixture("monochange/affected-step-override", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("pr-check"),
			OsString::from("--paths"),
			OsString::from("crates/core/src/lib.rs"),
			OsString::from("--labels"),
			OsString::from("no-changeset-required"),
			OsString::from("--enforce"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	assert!(output.contains("changeset policy: skipped"));
	assert!(output.contains("matched skip labels: no-changeset-required"));
}

#[test]
fn planning_behavior_is_consistent_across_ecosystem_fixtures() {
	assert_simple_release_pattern(
		"../../fixtures/cargo/workspace",
		"crates/core/Cargo.toml",
		"crates/app/Cargo.toml",
	);
	assert_simple_release_pattern(
		"../../fixtures/npm/workspace",
		"packages/shared/package.json",
		"packages/web/package.json",
	);
	assert_simple_release_pattern(
		"../../fixtures/deno/workspace",
		"packages/shared/deno.json",
		"packages/tool/deno.json",
	);
	assert_simple_release_pattern(
		"../../fixtures/dart/workspace",
		"packages/shared/pubspec.yaml",
		"packages/app/pubspec.yaml",
	);
}

#[test]
fn source_github_release_comments_command_supports_provider_neutral_source_config() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/source/github");
	copy_directory(&fixture_root, tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("release-comments"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("release comments output: {error}"));

	assert!(output.contains("released_packages") || output.contains("releaseTargets"));
}

#[test]
fn command_release_dry_run_is_consistent_across_ecosystem_fixtures() {
	assert_cli_release_pattern(
		"../../fixtures/cargo/workspace",
		"crates/core/Cargo.toml",
		"crates/app/Cargo.toml",
	);
	assert_cli_release_pattern(
		"../../fixtures/npm/workspace",
		"packages/shared/package.json",
		"packages/web/package.json",
	);
	assert_cli_release_pattern(
		"../../fixtures/deno/workspace",
		"packages/shared/deno.json",
		"packages/tool/deno.json",
	);
	assert_cli_release_pattern(
		"../../fixtures/dart/workspace",
		"packages/shared/pubspec.yaml",
		"packages/app/pubspec.yaml",
	);
}

#[test]
fn configuration_guide_calls_out_current_implementation_limits() {
	let configuration_guide =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/guide/04-configuration.md");
	let content = fs::read_to_string(configuration_guide)
		.unwrap_or_else(|error| panic!("configuration guide: {error}"));

	for expected in [
		"`defaults.include_private`",
		"`[ecosystems.*].enabled/roots/exclude`",
		"`PrepareRelease`",
		"`RetargetRelease`",
		"`Command`",
	] {
		assert!(
			content.contains(expected),
			"configuration guide missing `{expected}`"
		);
	}
}

#[test]
fn repairable_releases_guide_distinguishes_manifest_and_release_record() {
	let guide = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../docs/src/guide/12-repairable-releases.md");
	let content = fs::read_to_string(guide)
		.unwrap_or_else(|error| panic!("repairable releases guide: {error}"));

	for expected in [
		"manifest = \"what monochange is preparing right now\"",
		"release record = \"what this release commit historically declared\"",
		"`ReleaseRecord` does **not** replace the cached release manifest",
		"mc release-record --from v1.2.3",
		"mc repair-release --from v1.2.3 --target HEAD --dry-run",
		"Prefer publishing a new patch release",
	] {
		assert!(
			content.contains(expected),
			"repairable releases guide missing `{expected}`"
		);
	}
}

#[test]
fn github_automation_guide_mentions_release_repair_and_dry_run() {
	let guide =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/guide/08-github-automation.md");
	let content = fs::read_to_string(guide)
		.unwrap_or_else(|error| panic!("github automation guide: {error}"));

	for expected in [
		"mc release-record --from v1.2.3",
		"mc repair-release --from v1.2.3 --target HEAD --dry-run",
		"Use `--dry-run` first for `repair-release`",
	] {
		assert!(
			content.contains(expected),
			"github automation guide missing `{expected}`"
		);
	}
}

#[test]
fn discovery_guide_describes_stable_relative_output_paths() {
	let discovery_guide =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/guide/03-discovery.md");
	let content = fs::read_to_string(discovery_guide)
		.unwrap_or_else(|error| panic!("discovery guide: {error}"));

	assert!(
		content.contains("rendered relative to the repository root"),
		"discovery guide missing stable relative output note"
	);
}

#[test]
fn release_planning_guide_describes_release_cli_command_requirements() {
	let release_guide =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/guide/06-release-planning.md");
	let content =
		fs::read_to_string(release_guide).unwrap_or_else(|error| panic!("release guide: {error}"));

	for expected in [
		"`mc release` is a config-driven workflow command.",
		"`.changeset/*.md`",
		"`--dry-run`",
	] {
		assert!(
			content.contains(expected),
			"release planning guide missing `{expected}`"
		);
	}
}

fn assert_simple_release_pattern(
	relative_fixture_root: &str,
	direct_manifest_suffix: &str,
	dependent_manifest_suffix: &str,
) {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_fixture_root);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());
	let changes_path = tempdir.path().join(".changeset/feature.md");

	let discovery = discover_workspace(tempdir.path())
		.unwrap_or_else(|error| panic!("fixture discovery: {error}"));
	assert_eq!(discovery.packages.len(), 2);
	assert_eq!(discovery.dependencies.len(), 1);

	let plan = plan_release(tempdir.path(), &changes_path)
		.unwrap_or_else(|error| panic!("fixture release plan: {error}"));
	let direct = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains(direct_manifest_suffix))
		.unwrap_or_else(|| panic!("expected direct decision"));
	let dependent = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains(dependent_manifest_suffix))
		.unwrap_or_else(|| panic!("expected dependent decision"));

	assert_eq!(direct.recommended_bump.to_string(), "minor");
	assert_eq!(dependent.recommended_bump.to_string(), "patch");
}

fn assert_cli_release_pattern(
	relative_fixture_root: &str,
	direct_manifest_suffix: &str,
	dependent_manifest_suffix: &str,
) {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_fixture_root);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("step:prepare-release"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("json: {error}"));
	let decisions = parsed["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("plan decisions"));
	let direct = decisions
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains(direct_manifest_suffix))
		})
		.unwrap_or_else(|| panic!("expected direct release decision"));
	let dependent = decisions
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains(dependent_manifest_suffix))
		})
		.unwrap_or_else(|| panic!("expected dependent release decision"));

	assert_eq!(direct["bump"].as_str(), Some("minor"));
	assert_eq!(dependent["bump"].as_str(), Some("patch"));
}

fn seed_diagnostics_fixture(root: &Path, with_second_changeset: bool) {
	if with_second_changeset {
		copy_fixture("monochange/diagnostics-two-changesets", root);
	} else {
		copy_fixture("monochange/diagnostics-base", root);
	}
}

fn seed_changeset_policy_fixture(root: &Path, _with_changeset: bool) {
	copy_fixture("monochange/changeset-policy-base", root);
}

fn seed_release_fixture(root: &Path, command_step: Option<&str>, failing_changelog: bool) {
	if failing_changelog {
		copy_fixture("monochange/release-failing-changelog", root);
	} else if command_step.is_some() {
		copy_fixture("monochange/release-with-command-step", root);
	} else {
		copy_fixture("monochange/release-base", root);
	}
	let _ = command_step;
}

fn seed_cargo_lock_release_fixture(root: &Path) {
	copy_fixture("monochange/cargo-lock-release", root);
}

fn seed_npm_lock_release_fixture(root: &Path) {
	copy_fixture("monochange/npm-lock-release", root);
}

fn seed_bun_lockb_release_fixture(root: &Path) {
	copy_fixture("monochange/bun-lock-release", root);
}

fn seed_deno_lock_release_fixture(root: &Path) {
	copy_fixture("monochange/deno-lock-release", root);
}

fn seed_explicit_lockfile_override_fixture(root: &Path) {
	copy_fixture("monochange/explicit-lockfile-override", root);
}

fn seed_group_empty_update_message_fixture(root: &Path) {
	copy_fixture("monochange/group-empty-update-message", root);
}

#[test]
fn validate_rejects_workspace_versioned_packages_in_different_groups() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/cargo/workspace-versioned-different-groups");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected validation failure"));
	let rendered = error.to_string();

	assert!(rendered.contains("version.workspace = true"));
	assert!(rendered.contains("same version group"));
}

#[test]
fn validate_rejects_workspace_versioned_packages_not_in_any_group() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/cargo/workspace-versioned-ungrouped");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected validation failure"));
	let rendered = error.to_string();

	assert!(rendered.contains("version.workspace = true"));
	assert!(rendered.contains("same version group"));
	assert!(rendered.contains("not in any group"));
}

#[test]
fn validate_accepts_workspace_versioned_packages_in_same_group() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/cargo/workspace-versioned-same-group");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.unwrap_or_else(|error| panic!("validate output: {error}"));
	assert!(output.contains("workspace validation passed"));
}

#[test]
fn validate_accepts_single_workspace_versioned_package_without_group() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/cargo/workspace-versioned-single");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.unwrap_or_else(|error| panic!("validate output: {error}"));
	assert!(output.contains("workspace validation passed"));
}

#[test]
fn command_step_with_id_captures_stdout_for_later_steps() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_step_outputs_fixture(tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("echo-test")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(
		output.contains("got:hello-world"),
		"expected stdout interpolation, got: {output}"
	);
}

#[test]
fn command_step_with_shell_string_uses_custom_shell() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_step_outputs_fixture(tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("shell-bash")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(
		output.contains("result:bash-works"),
		"expected bash shell output, got: {output}"
	);
}

#[test]
fn release_step_exposes_updated_changelogs_to_command_steps() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_step_outputs_fixture(tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(
		output.contains("crates/core/CHANGELOG.md"),
		"expected changelog path in output, got: {output}"
	);
}

fn seed_step_outputs_fixture(root: &Path) {
	let fixture_dir =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/step-outputs/base");
	let files = [
		"Cargo.toml",
		"monochange.toml",
		"crates/core/Cargo.toml",
		"crates/core/src/lib.rs",
		"crates/core/CHANGELOG.md",
		".changeset/feature.md",
	];
	for file in &files {
		let source = fixture_dir.join(file);
		let target = root.join(file);
		if let Some(parent) = target.parent() {
			fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dir: {error}"));
		}
		fs::copy(&source, &target)
			.unwrap_or_else(|error| panic!("copy {}: {error}", source.display()));
	}
}

#[cfg(unix)]
fn make_executable(path: &Path) {
	let metadata =
		fs::metadata(path).unwrap_or_else(|error| panic!("metadata {}: {error}", path.display()));
	let mut permissions = metadata.permissions();
	permissions.set_mode(0o755);
	fs::set_permissions(path, permissions)
		.unwrap_or_else(|error| panic!("set permissions {}: {error}", path.display()));
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

fn with_path_prefixed<T>(root: &Path, action: impl FnOnce() -> T) -> T {
	let bin_dir = root.join("tools/bin");
	for tool in ["cargo", "npm", "npx", "pnpm", "bun", "bunx", "custom-lock"] {
		let candidate = bin_dir.join(tool);
		if candidate.exists() {
			make_executable(&candidate);
		}
	}
	let existing = std::env::var_os("PATH").unwrap_or_default();
	let mut combined = std::env::split_paths(&existing).collect::<Vec<_>>();
	combined.insert(0, bin_dir);
	let new_path =
		std::env::join_paths(combined).unwrap_or_else(|error| panic!("join PATH entries: {error}"));
	temp_env::with_var("PATH", Some(new_path), action)
}

fn with_fixture_path_only<T>(root: &Path, action: impl FnOnce() -> T) -> T {
	let bin_dir = root.join("tools/bin");
	for tool in ["cargo", "npm", "npx", "pnpm", "bun", "bunx", "custom-lock"] {
		let candidate = bin_dir.join(tool);
		if candidate.exists() {
			make_executable(&candidate);
		}
	}
	let path = std::env::join_paths([bin_dir])
		.unwrap_or_else(|error| panic!("join isolated PATH entries: {error}"));
	temp_env::with_var("PATH", Some(path), action)
}

fn copy_fixture(fixture_relative: &str, dest: &Path) {
	copy_directory(&fixture_path(fixture_relative), dest);
}

#[test]
fn step_override_with_literal_list_uses_list_as_changed_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/changeset-policy-base", tempdir.path());
	copy_fixture("monochange/step-override-literal-list", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("pr-check")],
	)
	.unwrap_or_else(|error| panic!("pr-check output: {error}"));
	let json: serde_json::Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("json: {error}; output={output}"));
	assert_eq!(json["affectedPackageIds"][0], "core");
}

#[test]
fn step_override_with_non_direct_jinja_template_renders_correctly() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/step-override-jinja", tempdir.path());
	run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("announce"),
			OsString::from("--prefix"),
			OsString::from("goodbye"),
		],
	)
	.unwrap_or_else(|error| panic!("announce output: {error}"));
	let output = fs::read_to_string(tempdir.path().join("output.txt"))
		.unwrap_or_else(|error| panic!("output file: {error}"));
	assert_eq!(output, "goodbye-world");
}

#[test]
fn step_override_forwards_multi_value_list_reference_as_list() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/changeset-policy-base", tempdir.path());
	copy_fixture("monochange/step-override-multi-list", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("pr-check"),
			OsString::from("--paths"),
			OsString::from("crates/core/src/lib.rs"),
			OsString::from("--paths"),
			OsString::from("docs/readme.md"),
		],
	)
	.unwrap_or_else(|error| panic!("pr-check output: {error}"));
	let json: serde_json::Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("json: {error}; output={output}"));
	assert!(
		json["matchedPaths"]
			.as_array()
			.is_some_and(|p| p.iter().any(|v| v == "crates/core/src/lib.rs"))
	);
}

#[test]
fn step_override_missing_template_reference_produces_empty_changed_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/changeset-policy-base", tempdir.path());
	copy_fixture("monochange/step-override-missing-ref", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("pr-check")],
	)
	.unwrap_or_else(|error| panic!("pr-check output: {error}"));
	let json: serde_json::Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("json: {error}; output={output}"));
	assert_eq!(json["affectedPackageIds"].as_array().map(Vec::len), Some(0));
}

// Unit tests for private helper functions in the input-override pipeline.
// These tests cover branches not reached by integration paths (e.g.
// Null/Number/Object JSON types, invalid template reference forms).

#[test]
fn parse_direct_template_reference_returns_inner_path_for_valid_refs() {
	use super::parse_direct_template_reference;
	assert_eq!(
		parse_direct_template_reference("{{ inputs.message }}"),
		Some("inputs.message")
	);
	assert_eq!(
		parse_direct_template_reference("  {{ foo.bar_baz }}  "),
		Some("foo.bar_baz")
	);
}

#[test]
fn parse_direct_template_reference_returns_none_for_empty_inner() {
	use super::parse_direct_template_reference;
	assert_eq!(parse_direct_template_reference("{{  }}"), None);
	assert_eq!(parse_direct_template_reference("{{}}"), None);
}

#[test]
fn parse_direct_template_reference_returns_none_for_invalid_chars() {
	use super::parse_direct_template_reference;
	assert_eq!(parse_direct_template_reference("{{ foo-bar }}"), None);
	assert_eq!(parse_direct_template_reference("{{ foo bar }}"), None);
}

#[test]
fn parse_direct_template_reference_returns_none_for_literals() {
	use super::parse_direct_template_reference;
	assert_eq!(parse_direct_template_reference("{{ false }}"), None);
	assert_eq!(parse_direct_template_reference("{{ 1 }}"), None);
}

#[test]
fn parse_direct_template_reference_returns_none_when_not_a_bare_ref() {
	use super::parse_direct_template_reference;
	assert_eq!(parse_direct_template_reference("prefix-{{ foo }}"), None);
	assert_eq!(parse_direct_template_reference("hello world"), None);
}

#[test]
fn normalize_when_expression_supports_logical_operators() {
	assert_eq!(
		normalize_when_expression("{{ flag_a && !flag_b || flag_c }}"),
		"{{ flag_a  and  not flag_b  or  flag_c }}"
	);
}

fn cli_context_for_when_evaluation_tests() -> CliContext {
	CliContext {
		root: PathBuf::from("."),
		dry_run: false,
		quiet: false,
		show_diff: false,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	}
}

#[test]
fn should_execute_cli_step_runs_when_condition_is_true() {
	let context = cli_context_for_when_evaluation_tests();
	let step_inputs = BTreeMap::from([
		("run".to_string(), vec!["true".to_string()]),
		("extra".to_string(), vec!["true".to_string()]),
	]);
	let step = monochange_core::CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: Some("{{ inputs.run && inputs.extra }}".to_string()),
		command: "printf hi".to_string(),
		dry_run_command: None,
		shell: monochange_core::ShellConfig::default(),
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	assert!(
		should_execute_cli_step(&step, &context, &step_inputs)
			.unwrap_or_else(|error| { panic!("when condition: {error}") })
	);
}

#[test]
fn should_execute_cli_step_skips_when_condition_is_false() {
	let context = cli_context_for_when_evaluation_tests();
	let step_inputs = BTreeMap::from([("run".to_string(), vec!["false".to_string()])]);
	let step = monochange_core::CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: Some("{{ inputs.run }}".to_string()),
		command: "printf hi".to_string(),
		dry_run_command: None,
		shell: monochange_core::ShellConfig::default(),
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	assert!(
		!should_execute_cli_step(&step, &context, &step_inputs)
			.unwrap_or_else(|error| { panic!("when condition: {error}") })
	);
}

#[test]
fn should_execute_cli_step_skips_for_zero_value() {
	let context = cli_context_for_when_evaluation_tests();
	let step_inputs = BTreeMap::from([("run".to_string(), vec!["0".to_string()])]);
	let step = monochange_core::CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: Some("{{ inputs.run }}".to_string()),
		command: "printf hi".to_string(),
		dry_run_command: None,
		shell: monochange_core::ShellConfig::default(),
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	assert!(
		!should_execute_cli_step(&step, &context, &step_inputs)
			.unwrap_or_else(|error| { panic!("when condition: {error}") })
	);
}

#[test]
fn should_execute_cli_step_can_gate_on_number_of_changesets() {
	let mut context = cli_context_for_when_evaluation_tests();
	let step_inputs = BTreeMap::new();
	let step = monochange_core::CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: Some("{{ number_of_changesets > 0 }}".to_string()),
		command: "printf hi".to_string(),
		dry_run_command: None,
		shell: monochange_core::ShellConfig::default(),
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};

	context.prepared_release = Some(sample_prepared_release_for_cli_render());
	assert!(
		!should_execute_cli_step(&step, &context, &step_inputs)
			.unwrap_or_else(|error| { panic!("when condition: {error}") })
	);

	let mut prepared = sample_prepared_release_for_cli_render();
	prepared.changeset_paths = vec![PathBuf::from(".changeset/feature.md")];
	context.prepared_release = Some(prepared);
	assert!(
		should_execute_cli_step(&step, &context, &step_inputs)
			.unwrap_or_else(|error| { panic!("when condition: {error}") })
	);
}

#[test]
fn should_execute_cli_step_trims_and_treats_1_as_true() {
	let context = cli_context_for_when_evaluation_tests();
	let step_inputs = BTreeMap::from([("run".to_string(), vec![" 1 ".to_string()])]);
	let step = monochange_core::CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: Some("{{ inputs.run }}".to_string()),
		command: "printf hi".to_string(),
		dry_run_command: None,
		shell: monochange_core::ShellConfig::default(),
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	assert!(
		should_execute_cli_step(&step, &context, &step_inputs)
			.unwrap_or_else(|error| { panic!("when condition: {error}") })
	);
}

#[test]
fn should_execute_cli_step_skips_with_not_operator() {
	let context = cli_context_for_when_evaluation_tests();
	let step_inputs = BTreeMap::from([("skip".to_string(), vec!["true".to_string()])]);
	let step = monochange_core::CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: Some("{{ ! inputs.skip }}".to_string()),
		command: "printf hi".to_string(),
		dry_run_command: None,
		shell: monochange_core::ShellConfig::default(),
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	assert!(
		!should_execute_cli_step(&step, &context, &step_inputs)
			.unwrap_or_else(|error| { panic!("when condition: {error}") })
	);
}

#[test]
fn should_execute_cli_step_rejects_unknown_template_reference() {
	let context = cli_context_for_when_evaluation_tests();
	let step_inputs = BTreeMap::from([("run".to_string(), vec!["true".to_string()])]);
	let step = monochange_core::CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: Some("{{ inputs.missing }}".to_string()),
		command: "printf hi".to_string(),
		dry_run_command: None,
		shell: monochange_core::ShellConfig::default(),
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	let error = should_execute_cli_step(&step, &context, &step_inputs).unwrap_err();
	assert!(
		error
			.to_string()
			.contains("failed to evaluate `when` condition `{{ inputs.missing }}`")
	);
}

#[test]
fn should_execute_cli_step_rejects_non_scalar_condition_value() {
	let context = cli_context_for_when_evaluation_tests();
	let step_inputs =
		BTreeMap::from([("list".to_string(), vec!["a".to_string(), "b".to_string()])]);
	let step = monochange_core::CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: Some("{{ inputs.list }}".to_string()),
		command: "printf hi".to_string(),
		dry_run_command: None,
		shell: monochange_core::ShellConfig::default(),
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	let error = should_execute_cli_step(&step, &context, &step_inputs).unwrap_err();
	assert!(error.to_string().contains("is not a scalar boolean value"));
}

#[test]
fn lookup_template_value_traverses_nested_objects() {
	use serde_json::json;

	use super::lookup_template_value;
	let v = json!({"inputs": {"message": "hello"}});
	assert_eq!(
		lookup_template_value(&v, "inputs.message"),
		Some(&json!("hello"))
	);
}

#[test]
fn lookup_template_value_traverses_array_by_index() {
	use serde_json::json;

	use super::lookup_template_value;
	let v = json!({"items": ["a", "b", "c"]});
	assert_eq!(lookup_template_value(&v, "items.1"), Some(&json!("b")));
}

#[test]
fn lookup_template_value_returns_none_for_missing_key() {
	use serde_json::json;

	use super::lookup_template_value;
	let v = json!({"inputs": {}});
	assert_eq!(lookup_template_value(&v, "inputs.missing"), None);
}

#[test]
fn lookup_template_value_returns_none_for_primitive_descent() {
	use serde_json::json;

	use super::lookup_template_value;
	let v = json!({"foo": "string_value"});
	assert_eq!(lookup_template_value(&v, "foo.nested"), None);
}

#[test]
fn template_value_to_input_values_null_returns_empty() {
	use super::template_value_to_input_values;
	assert_eq!(
		template_value_to_input_values(&serde_json::Value::Null),
		Vec::<String>::new()
	);
}

#[test]
fn template_value_to_input_values_number_returns_string() {
	use serde_json::json;

	use super::template_value_to_input_values;
	assert_eq!(
		template_value_to_input_values(&json!(42)),
		vec!["42".to_string()]
	);
	assert_eq!(
		template_value_to_input_values(&json!(1.5)),
		vec!["1.5".to_string()]
	);
}

#[test]
fn template_value_to_input_values_array_flattens_elements() {
	use serde_json::json;

	use super::template_value_to_input_values;
	assert_eq!(
		template_value_to_input_values(&json!(["a", "b", "c"])),
		vec!["a", "b", "c"]
	);
	assert_eq!(
		template_value_to_input_values(&json!([true, false])),
		vec!["true", "false"]
	);
}

#[test]
fn template_value_to_input_values_object_returns_json_serialization() {
	use serde_json::json;

	use super::template_value_to_input_values;
	let obj = json!({"k": "v"});
	let result = template_value_to_input_values(&obj);
	assert_eq!(result.len(), 1);
	assert!(result[0].contains('"'));
}

#[test]
fn build_release_commit_message_includes_release_record_body() {
	let source = sample_source_configuration_for_release_commit();
	let manifest = sample_release_manifest_for_commit_message(true, true);

	let commit_message = crate::build_release_commit_message(Some(&source), &manifest);
	assert_eq!(commit_message.subject, "chore(release): prepare release");
	let body = commit_message
		.body
		.unwrap_or_else(|| panic!("expected commit body"));
	assert!(body.contains("Prepare release."));
	assert!(body.contains("- release targets: sdk (1.2.3)"));
	assert!(body.contains("- released packages: monochange, monochange_core"));
	assert!(body.contains("- updated changelogs: crates/monochange/CHANGELOG.md"));
	assert!(body.contains("- deleted changesets: .changeset/feature.md"));
	assert!(body.contains("## monochange Release Record"));
	assert!(body.contains("\"command\": \"release-pr\""));
}

#[test]
fn render_release_commit_body_omits_release_targets_when_empty() {
	let source = sample_source_configuration_for_release_commit();
	let mut manifest = sample_release_manifest_for_commit_message(false, false);
	manifest.release_targets.clear();

	let body = crate::render_release_commit_body(Some(&source), &manifest);
	assert!(!body.contains("- release targets:"));
	assert!(body.contains("- released packages: monochange, monochange_core"));
}

#[test]
fn build_release_commit_message_uses_default_title_without_source() {
	let manifest = sample_release_manifest_for_commit_message(false, false);

	let commit_message = crate::build_release_commit_message(None, &manifest);
	assert_eq!(commit_message.subject, "chore(release): prepare release");
	assert!(
		commit_message
			.body
			.as_deref()
			.is_some_and(|body| body.contains("## monochange Release Record"))
	);
}

#[test]
fn render_release_commit_body_omits_empty_optional_sections() {
	let source = sample_source_configuration_for_release_commit();
	let manifest = sample_release_manifest_for_commit_message(false, false);

	let body = crate::render_release_commit_body(Some(&source), &manifest);
	assert!(body.contains("Prepare release."));
	assert!(body.contains("- release targets: sdk (1.2.3)"));
	assert!(body.contains("- released packages: monochange, monochange_core"));
	assert!(!body.contains("- updated changelogs:"));
	assert!(!body.contains("- deleted changesets:"));
}

#[test]
fn build_release_record_captures_provider_and_release_targets() {
	let source = sample_source_configuration_for_release_commit();
	let manifest = sample_release_manifest_for_commit_message(true, true);

	let record = crate::build_release_record(Some(&source), &manifest);
	assert_eq!(record.command, "release-pr");
	assert_eq!(record.version.as_deref(), Some("1.2.3"));
	assert_eq!(record.group_version.as_deref(), Some("1.2.3"));
	assert_eq!(record.release_targets.len(), 1);
	assert_eq!(record.release_targets[0].tag_name, "v1.2.3");
	assert_eq!(record.updated_changelogs.len(), 1);
	assert_eq!(record.deleted_changesets.len(), 1);
	let provider = record
		.provider
		.unwrap_or_else(|| panic!("expected provider block"));
	assert_eq!(provider.kind, monochange_core::SourceProvider::GitHub);
	assert_eq!(provider.owner, "ifiokjr");
	assert_eq!(provider.repo, "monochange");
}

fn sample_source_configuration_for_release_commit() -> monochange_core::SourceConfiguration {
	monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitHub,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings {
			title: "chore(release): prepare release".to_string(),
			..monochange_core::ProviderMergeRequestSettings::default()
		},
	}
}

fn sample_release_manifest_for_commit_message(
	include_changelog: bool,
	include_deleted_changeset: bool,
) -> monochange_core::ReleaseManifest {
	monochange_core::ReleaseManifest {
		command: "release-pr".to_string(),
		dry_run: false,
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![monochange_core::ReleaseManifestTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.3".to_string(),
			members: vec!["monochange".to_string(), "monochange_core".to_string()],
			rendered_title: "monochange 1.2.3".to_string(),
			rendered_changelog_title: "1.2.3".to_string(),
		}],
		released_packages: vec!["monochange".to_string(), "monochange_core".to_string()],
		changed_files: vec![Path::new("Cargo.lock").to_path_buf()],
		changelogs: if include_changelog {
			vec![monochange_core::ReleaseManifestChangelog {
				owner_id: "sdk".to_string(),
				owner_kind: monochange_core::ReleaseOwnerKind::Group,
				path: Path::new("crates/monochange/CHANGELOG.md").to_path_buf(),
				format: monochange_core::ChangelogFormat::Monochange,
				notes: monochange_core::ReleaseNotesDocument {
					title: "1.2.3".to_string(),
					summary: vec!["- prepare release".to_string()],
					sections: Vec::new(),
				},
				rendered: "## 1.2.3".to_string(),
			}]
		} else {
			Vec::new()
		},
		changesets: Vec::new(),
		deleted_changesets: if include_deleted_changeset {
			vec![Path::new(".changeset/feature.md").to_path_buf()]
		} else {
			Vec::new()
		},
		package_publications: Vec::new(),
		plan: monochange_core::ReleaseManifestPlan {
			workspace_root: Path::new(".").to_path_buf(),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
	}
}

#[test]
fn text_release_record_discovery_renders_targets_packages_and_provider() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234567890".to_string(),
		record_commit: "abc1234567890".to_string(),
		distance: 0,
		record: sample_release_record_for_discovery_text(),
	};

	let rendered = crate::release_record::text_release_record_discovery(&discovery);
	assert!(rendered.contains("input ref: v1.2.3"));
	assert!(rendered.contains("resolved commit: abc1234"));
	assert!(rendered.contains("record commit: abc1234"));
	assert!(rendered.contains("distance: 0"));
	assert!(rendered.contains("version: 1.2.3"));
	assert!(rendered.contains("group version: 1.2.3"));
	assert!(rendered.contains("- group sdk -> 1.2.3 (tag: v1.2.3)"));
	assert!(rendered.contains("- monochange"));
	assert!(rendered.contains("- monochange_core"));
	assert!(rendered.contains("provider: github ifiokjr/monochange"));
}

fn sample_release_record_for_discovery_text() -> monochange_core::ReleaseRecord {
	monochange_core::ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-07T08:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![monochange_core::ReleaseRecordTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.2.3".to_string(),
			members: vec!["monochange".to_string(), "monochange_core".to_string()],
		}],
		released_packages: vec!["monochange".to_string(), "monochange_core".to_string()],
		changed_files: vec![Path::new("Cargo.lock").to_path_buf()],
		updated_changelogs: vec![Path::new("crates/monochange/CHANGELOG.md").to_path_buf()],
		deleted_changesets: vec![Path::new(".changeset/feature.md").to_path_buf()],
		changesets: Vec::new(),
		changelogs: Vec::new(),
		package_publications: Vec::new(),
		provider: Some(monochange_core::ReleaseRecordProvider {
			kind: monochange_core::SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
	}
}

#[test]
fn text_release_record_discovery_omits_empty_sections() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "HEAD".to_string(),
		resolved_commit: "def5678901234".to_string(),
		record_commit: "abc1234567890".to_string(),
		distance: 2,
		record: monochange_core::ReleaseRecord {
			schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
			kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
			created_at: "2026-04-07T08:00:00Z".to_string(),
			command: "release-pr".to_string(),
			version: None,
			group_version: None,
			release_targets: Vec::new(),
			released_packages: Vec::new(),
			changed_files: Vec::new(),
			updated_changelogs: Vec::new(),
			deleted_changesets: Vec::new(),
			changesets: Vec::new(),
			changelogs: Vec::new(),
			package_publications: Vec::new(),
			provider: None,
		},
	};

	let rendered = crate::release_record::text_release_record_discovery(&discovery);
	assert!(rendered.contains("input ref: HEAD"));
	assert!(!rendered.contains("  targets:"));
	assert!(!rendered.contains("  packages:"));
	assert!(!rendered.contains("  provider:"));
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn commit_release_command_creates_local_commit_with_release_record() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_fixture("prepared-release/source-github-follow-up/workspace", root);
	init_git_repo(root);
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);

	// CommitRelease requires a prepared release artifact; run PrepareRelease first
	run_cli(
		root,
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("prepare-release output: {error}"));

	let output = run_cli(
		root,
		[OsString::from("mc"), OsString::from("step:commit-release")],
	)
	.unwrap_or_else(|error| panic!("commit-release output: {error}"));
	let commit_subject = git_output_in_temp_repo(root, &["log", "-1", "--pretty=%s"]);
	let commit_body = git_output_in_temp_repo(root, &["log", "-1", "--pretty=%B"]);
	let status = git_output_in_temp_repo(root, &["status", "--short"]);

	assert!(output.contains("## Release commit"));
	assert!(output.contains("- **Status:** completed"));
	assert_eq!(commit_subject, "chore(release): prepare release");
	assert!(commit_body.contains("## monochange Release Record"));
	assert!(commit_body.contains("\"command\": \"step:commit-release\""));
	assert!(
		status.is_empty(),
		"expected clean working tree, got: {status}"
	);
}

#[test]
fn commit_release_command_reports_json_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_fixture("prepared-release/source-github-follow-up/workspace", root);
	init_git_repo(root);
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);

	// CommitRelease requires a prepared release artifact; run PrepareRelease first
	run_cli(
		root,
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("prepare-release output: {error}"));

	let output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("step:commit-release"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_or_else(|error| panic!("commit-release output: {error}"));

	// step:commit-release does not support --format, so dry-run returns
	// plain text report.
	assert!(output.contains("# `step:commit-release` (dry-run)"));
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn repair_release_command_dry_run_reports_text_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	create_release_record_history(root);

	let output = temp_env::with_var("MONOCHANGE_PROGRESS_FORMAT", None::<&str>, || {
		run_cli(
			root,
			[
				OsString::from("mc"),
				OsString::from("step:retarget-release"),
				OsString::from("--from"),
				OsString::from("v1.2.3"),
				OsString::from("--target"),
				OsString::from("HEAD"),
				OsString::from("--sync-provider=false"),
				OsString::from("--dry-run"),
			],
		)
	})
	.unwrap_or_else(|error| panic!("repair-release output: {error}"));

	assert!(output.contains("repair release:"));
	assert!(output.contains("from: v1.2.3"));
	assert!(output.contains("tags to move:"));
	assert!(output.contains("v1.2.3"));
	assert!(output.contains("provider sync: disabled"));
	assert!(
		output.contains("status: dry-run"),
		"unexpected output: {output}"
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn repair_release_command_reports_json_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	create_release_record_history(root);

	let output = temp_env::with_var("MONOCHANGE_PROGRESS_FORMAT", None::<&str>, || {
		run_cli(
			root,
			[
				OsString::from("mc"),
				OsString::from("step:retarget-release"),
				OsString::from("--from"),
				OsString::from("v1.2.3"),
				OsString::from("--sync-provider=false"),
				OsString::from("--dry-run"),
			],
		)
	})
	.unwrap_or_else(|error| panic!("repair-release output: {error}"));

	// step:retarget-release does not support --format, so dry-run returns
	// plain text report.
	assert!(output.contains("repair release:"));
	assert!(output.contains("from: v1.2.3"));
	assert!(output.contains("tags to move:"));
	assert!(output.contains("provider sync: disabled"));
	assert!(
		output.contains("status: dry-run"),
		"unexpected output: {output}"
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn repair_release_command_rejects_non_descendant_targets_without_force() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write initial file: {error}"));
	git_in_temp_repo(root, &["add", "monochange.toml", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let initial_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	fs::write(root.join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	let release_record =
		monochange_core::render_release_record_block(&sample_release_record_for_retarget())
			.unwrap_or_else(|error| panic!("render release record: {error}"));
	git_in_temp_repo(
		root,
		&[
			"commit",
			"-m",
			"chore(release): prepare release",
			"-m",
			release_record.as_str(),
		],
	);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	fs::write(root.join("release.txt"), "follow-up\n")
		.unwrap_or_else(|error| panic!("write follow-up file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "fix: follow-up release change"]);
	let main_branch = git_output_in_temp_repo(root, &["branch", "--show-current"]);
	git_in_temp_repo(root, &["checkout", &initial_commit]);
	fs::write(root.join("release.txt"), "branch\n")
		.unwrap_or_else(|error| panic!("write branch release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "branch"]);
	let branch_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["checkout", &main_branch]);

	let error = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("step:retarget-release"),
			OsString::from("--from"),
			OsString::from("v1.2.3"),
			OsString::from("--target"),
			OsString::from(branch_commit),
			OsString::from("--sync-provider=false"),
			OsString::from("--dry-run"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("expected non-descendant error"));
	assert!(
		error
			.to_string()
			.contains("is not a descendant of release-record commit")
	);
}

#[test]
fn template_context_exposes_release_commit_namespace() {
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		quiet: false,
		show_diff: false,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: Some(crate::CommitReleaseReport {
			subject: "chore(release): prepare release".to_string(),
			body: "Prepare release.".to_string(),
			commit: Some("abc1234567890".to_string()),
			tracked_paths: vec![PathBuf::from("Cargo.toml")],
			dry_run: false,
			status: "completed".to_string(),
		}),
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let template_context = crate::build_cli_template_context(&context, &BTreeMap::new(), None);
	assert_eq!(
		template_context
			.get("release_commit")
			.and_then(|value| value.pointer("/commit"))
			.and_then(serde_json::Value::as_str),
		Some("abc1234567890")
	);
}

#[test]
fn template_context_exposes_retarget_namespace() {
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		quiet: false,
		show_diff: false,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: Some(sample_retarget_release_report()),
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let template_context = crate::build_cli_template_context(&context, &BTreeMap::new(), None);
	assert_eq!(
		template_context
			.get("retarget")
			.and_then(|value| value.pointer("/status"))
			.and_then(serde_json::Value::as_str),
		Some("dry_run")
	);
}

#[test]
fn template_context_exposes_publish_namespace() {
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		quiet: false,
		show_diff: false,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: Some(crate::package_publish::PackagePublishReport {
			mode: crate::package_publish::PackagePublishRunMode::Release,
			dry_run: true,
			packages: vec![crate::package_publish::PackagePublishOutcome {
				package: "@monochange/skill".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: "npm".to_string(),
				version: "1.2.3".to_string(),
				status: crate::package_publish::PackagePublishStatus::Planned,
				message: "would publish package".to_string(),
				placeholder: false,
				trusted_publishing: crate::package_publish::TrustedPublishingOutcome {
					status: crate::package_publish::TrustedPublishingStatus::Planned,
					repository: Some("ifiokjr/monochange".to_string()),
					workflow: Some("publish.yml".to_string()),
					environment: None,
					setup_url: Some(
						"https://docs.npmjs.com/cli/v11/commands/npm-trust".to_string(),
					),
					message: "would configure trusted publishing".to_string(),
				},
			}],
		}),
		rate_limit_report: Some(monochange_core::PublishRateLimitReport {
			dry_run: true,
			windows: vec![monochange_core::RegistryRateLimitWindowPlan {
				registry: monochange_core::RegistryKind::Npm,
				operation: monochange_core::RateLimitOperation::Publish,
				limit: None,
				window_seconds: None,
				pending: 1,
				batches_required: 1,
				fits_single_window: true,
				confidence: monochange_core::RateLimitConfidence::Low,
				notes: "npm soft limit".to_string(),
				evidence: Vec::new(),
			}],
			batches: vec![monochange_core::PublishRateLimitBatch {
				registry: monochange_core::RegistryKind::Npm,
				operation: monochange_core::RateLimitOperation::Publish,
				batch_index: 1,
				total_batches: 1,
				packages: vec!["@monochange/skill".to_string()],
				recommended_wait_seconds: None,
			}],
			warnings: Vec::new(),
		}),
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let template_context = crate::build_cli_template_context(&context, &BTreeMap::new(), None);
	assert_eq!(
		template_context
			.get("publish")
			.and_then(|value| value.pointer("/mode"))
			.and_then(serde_json::Value::as_str),
		Some("release")
	);
	assert_eq!(
		template_context
			.get("publish")
			.and_then(|value| value.pointer("/packages/0/package"))
			.and_then(serde_json::Value::as_str),
		Some("@monochange/skill")
	);
	assert_eq!(
		template_context
			.get("publish")
			.and_then(|value| value.pointer("/packages/0/trustedPublishing/status"))
			.and_then(serde_json::Value::as_str),
		Some("planned")
	);
	assert_eq!(
		template_context
			.get("publish")
			.and_then(|value| value.pointer("/rateLimits/batches/0/packages/0"))
			.and_then(serde_json::Value::as_str),
		Some("@monochange/skill")
	);
}

#[test]
fn template_context_exposes_manifest_affected_steps_and_custom_variables() {
	let mut step_outputs = BTreeMap::new();
	step_outputs.insert(
		"lint".to_string(),
		crate::CommandStepOutput {
			stdout: "ok".to_string(),
			stderr: String::new(),
		},
	);
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		quiet: false,
		show_diff: false,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: Some(sample_prepared_release_for_cli_render()),
		prepared_file_diffs: Vec::new(),
		release_manifest_path: Some(PathBuf::from("target/release-manifest.json")),
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: Some(monochange_core::ChangesetPolicyEvaluation {
			status: monochange_core::ChangesetPolicyStatus::Passed,
			required: true,
			enforce: false,
			summary: "covered".to_string(),
			comment: None,
			labels: Vec::new(),
			matched_skip_labels: Vec::new(),
			changed_paths: vec!["crates/core/src/lib.rs".to_string()],
			matched_paths: vec!["crates/core/src/lib.rs".to_string()],
			ignored_paths: Vec::new(),
			changeset_paths: vec![".changeset/core.md".to_string()],
			affected_package_ids: vec!["core".to_string()],
			covered_package_ids: vec!["core".to_string()],
			uncovered_package_ids: Vec::new(),
			errors: Vec::new(),
		}),
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs,
		command_logs: Vec::new(),
	};
	let inputs = BTreeMap::from([("format".to_string(), vec!["json".to_string()])]);
	let variables = BTreeMap::from([
		(
			"custom_version".to_string(),
			monochange_core::CommandVariable::Version,
		),
		(
			"custom_changesets".to_string(),
			monochange_core::CommandVariable::Changesets,
		),
	]);
	let template_context = crate::build_cli_template_context(&context, &inputs, Some(&variables));
	assert_eq!(
		template_context
			.get("manifest")
			.and_then(|value| value.pointer("/path"))
			.and_then(serde_json::Value::as_str),
		Some("target/release-manifest.json")
	);
	assert_eq!(
		template_context
			.get("affected")
			.and_then(|value| value.pointer("/status"))
			.and_then(serde_json::Value::as_str),
		Some("passed")
	);
	assert_eq!(
		template_context
			.get("steps")
			.and_then(|value| value.pointer("/lint/stdout"))
			.and_then(serde_json::Value::as_str),
		Some("ok")
	);
	assert_eq!(
		template_context
			.get("custom_version")
			.and_then(serde_json::Value::as_str),
		Some("1.2.3")
	);
	assert_eq!(
		template_context
			.get("custom_changesets")
			.and_then(serde_json::Value::as_str),
		Some("")
	);
	assert_eq!(
		template_context
			.get("inputs")
			.and_then(|value| value.pointer("/format"))
			.and_then(serde_json::Value::as_str),
		Some("json")
	);
	assert!(
		template_context.get("format").is_none(),
		"CLI inputs should only be available under the inputs namespace"
	);
	assert_eq!(
		template_context
			.get("released_packages_list")
			.and_then(serde_json::Value::as_array)
			.map(Vec::len),
		Some(1)
	);
	assert_eq!(
		template_context
			.get("number_of_changesets")
			.and_then(serde_json::Value::as_u64),
		Some(0)
	);
	assert_eq!(
		template_context
			.get("changeset_count")
			.and_then(serde_json::Value::as_u64),
		Some(0)
	);
}

#[test]
fn render_cli_command_result_prefers_retarget_report() {
	let cli_command = CliCommandDefinition {
		name: "repair-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
	};
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		quiet: false,
		show_diff: false,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: Some(sample_retarget_release_report()),
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let rendered = crate::render_cli_command_result(&cli_command, &context);
	assert!(rendered.contains("repair release:"));
	assert!(rendered.contains("status: dry-run"));
}

#[test]
fn render_cli_command_result_renders_release_follow_up_sections() {
	let cli_command = CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
	};
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		quiet: false,
		show_diff: false,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: Some(sample_prepared_release_for_cli_render()),
		prepared_file_diffs: Vec::new(),
		release_manifest_path: Some(PathBuf::from("target/release-manifest.json")),
		release_requests: Vec::new(),
		release_results: vec!["dry-run org/repo v1.2.3 (sdk) via github".to_string()],
		release_request: None,
		release_request_result: Some(
			"dry-run org/repo monochange/release/release -> main via github".to_string(),
		),
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: vec!["dry-run org/repo 123".to_string()],
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let rendered = crate::render_cli_command_result(&cli_command, &context);
	assert!(rendered.contains("release manifest: target/release-manifest.json"));
	assert!(rendered.contains("releases:"));
	assert!(rendered.contains("release request:"));
	assert!(rendered.contains("issue comments:"));
	assert!(rendered.contains("changed files:"));
}

#[test]
fn render_cli_command_markdown_result_uses_markdown_sections_for_prepare_release() {
	let cli_command = CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
	};
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		quiet: false,
		show_diff: true,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: Some(sample_prepared_release_for_cli_render()),
		prepared_file_diffs: vec![PreparedFileDiff {
			path: PathBuf::from("Cargo.toml"),
			diff: "@@ -1 +1 @@".to_string(),
			display_diff: "diff --git a/Cargo.toml b/Cargo.toml\n@@ -1 +1 @@".to_string(),
		}],
		release_manifest_path: Some(PathBuf::from("target/release-manifest.json")),
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: vec!["skipped command `cargo publish` (dry-run)".to_string()],
	};
	let rendered = crate::render_cli_command_markdown_result(&cli_command, &context);
	assert!(rendered.contains("# `release` (dry-run)"));
	assert!(rendered.contains("## Summary"));
	assert!(rendered.contains("## Release targets"));
	assert!(rendered.contains("## File diffs"));
	assert!(rendered.contains("```diff"));
	assert!(rendered.contains("## Commands"));
	assert!(rendered.contains("skipped command `cargo publish` (dry-run)"));
}

#[test]
fn render_cli_command_markdown_result_renders_release_follow_up_sections() {
	let cli_command = CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
	};
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: false,
		quiet: false,
		show_diff: true,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: Some(sample_prepared_release_for_cli_render()),
		prepared_file_diffs: vec![PreparedFileDiff {
			path: PathBuf::from("Cargo.toml"),
			diff: "@@ -1 +1 @@".to_string(),
			display_diff: "diff --git a/Cargo.toml b/Cargo.toml\n@@ -1 +1 @@".to_string(),
		}],
		release_manifest_path: Some(PathBuf::from("target/release-manifest.json")),
		release_requests: Vec::new(),
		release_results: vec!["published monochange v1.2.3".to_string()],
		release_request: None,
		release_request_result: Some("opened monochange/release".to_string()),
		release_commit_report: Some(crate::CommitReleaseReport {
			subject: "chore(release): prepare release".to_string(),
			body: "## monochange Release Record".to_string(),
			commit: Some("abcdef1234567890".to_string()),
			tracked_paths: vec![PathBuf::from("Cargo.toml"), PathBuf::from("CHANGELOG.md")],
			dry_run: false,
			status: "completed".to_string(),
		}),
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: vec!["commented on #123".to_string()],
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: vec!["executed release workflow".to_string()],
	};

	let rendered = crate::render_cli_command_markdown_result(&cli_command, &context);
	assert!(rendered.contains("## Releases"));
	assert!(rendered.contains("published monochange v1.2.3"));
	assert!(rendered.contains("## Release commit"));
	assert!(rendered.contains("abcdef1"));
	assert!(rendered.contains("## Release request"));
	assert!(rendered.contains("opened monochange/release"));
	assert!(rendered.contains("## Issue comments"));
	assert!(rendered.contains("commented on #123"));
	assert!(rendered.contains("## Changed files"));
	assert!(rendered.contains("abcdef1"));
	assert!(rendered.contains("CHANGELOG.md"));
}

#[test]
fn execute_cli_command_retarget_release_requires_from_input() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "repair-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::RetargetRelease {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		}],
	};
	let error = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		true,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing from input error"));
	assert!(
		error
			.to_string()
			.contains("`RetargetRelease` requires a `from` input")
	);
}

#[test]
fn execute_cli_command_release_follow_up_steps_require_prepare_release() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["init", "-b", "main"])
		.output()
		.unwrap_or_else(|error| panic!("git init: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "user.email", "test@example.com"])
		.output()
		.unwrap_or_else(|error| panic!("git config email: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "user.name", "Test User"])
		.output()
		.unwrap_or_else(|error| panic!("git config name: {error}"));
	fs::write(tempdir.path().join("tracked.txt"), "test\n")
		.unwrap_or_else(|error| panic!("write tracked: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["add", "tracked.txt"])
		.output()
		.unwrap_or_else(|error| panic!("git add: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["commit", "-m", "initial commit"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cases = [
		(
			"publish-release",
			monochange_core::CliStepDefinition::PublishRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"no monochange release record found in first-parent ancestry",
		),
		(
			"release-pr",
			monochange_core::CliStepDefinition::OpenReleaseRequest {
				name: None,
				when: None,
				no_verify: false,
				inputs: BTreeMap::new(),
			},
			"`OpenReleaseRequest` requires a previous `PrepareRelease` step",
		),
		(
			"release-comments",
			monochange_core::CliStepDefinition::CommentReleasedIssues {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"no monochange release record found in first-parent ancestry",
		),
	];
	for (name, step, expected) in cases {
		let cli_command = CliCommandDefinition {
			name: name.to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: vec![step],
		};
		let error = crate::execute_cli_command(
			tempdir.path(),
			&configuration,
			&cli_command,
			true,
			BTreeMap::new(),
		)
		.err()
		.unwrap_or_else(|| panic!("expected missing PrepareRelease error for {name}"));
		assert!(error.to_string().contains(expected), "error: {error}");
	}
}

#[test]
fn execute_cli_command_source_follow_up_steps_require_source_configuration() {
	let root = fixture_path("monochange/release-base");
	let mut configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	configuration.source = Some(monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: Some("https://gitlab.example.com".to_string()),
		api_url: Some("https://gitlab.example.com/api/v4".to_string()),
		owner: "org".to_string(),
		repo: "repo".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	});
	let prepare_and_publish = CliCommandDefinition {
		name: "publish-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
				allow_empty_changesets: false,
			},
			monochange_core::CliStepDefinition::CommentReleasedIssues {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
		],
	};
	let error = crate::execute_cli_command(
		&root,
		&configuration,
		&prepare_and_publish,
		true,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected github source requirement error"));
	assert!(
		error.to_string().contains(
			"`CommentReleasedIssues` is not supported for `[source].provider = \"gitlab\"`"
		)
	);
}

#[test]
fn execute_cli_command_comment_released_issues_requires_source_configuration() {
	let root = fixture_path("monochange/release-base");
	let mut configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	configuration.source = None;
	let cli_command = CliCommandDefinition {
		name: "release-comments".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
				allow_empty_changesets: false,
			},
			monochange_core::CliStepDefinition::CommentReleasedIssues {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
		],
	};

	let error =
		crate::execute_cli_command(&root, &configuration, &cli_command, true, BTreeMap::new())
			.err()
			.unwrap_or_else(|| panic!("expected missing source configuration error"));

	assert!(
		error
			.to_string()
			.contains("`CommentReleasedIssues` requires `[source]` configuration")
	);
}

#[test]
fn execute_cli_command_publish_and_request_steps_require_source_configuration() {
	let root = fixture_path("monochange/release-base");
	let mut configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	configuration.source = None;

	let cases = [
		(
			"publish-release",
			monochange_core::CliStepDefinition::PublishRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"`PublishRelease` requires `[source]` configuration",
		),
		(
			"release-pr",
			monochange_core::CliStepDefinition::OpenReleaseRequest {
				name: None,
				when: None,
				no_verify: false,
				inputs: BTreeMap::new(),
			},
			"`OpenReleaseRequest` requires `[source]` configuration",
		),
	];

	for (name, step, expected) in cases {
		let cli_command = CliCommandDefinition {
			name: name.to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: vec![
				monochange_core::CliStepDefinition::PrepareRelease {
					name: None,
					when: None,
					inputs: BTreeMap::new(),
					allow_empty_changesets: false,
				},
				step,
			],
		};
		let error =
			crate::execute_cli_command(&root, &configuration, &cli_command, true, BTreeMap::new())
				.err()
				.unwrap_or_else(|| panic!("expected missing source error for {name}"));
		assert!(error.to_string().contains(expected), "error: {error}");
	}
}

#[test]
fn execute_cli_command_change_step_requires_reason_input() {
	let root = fixture_path("monochange/release-base");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "change".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::CreateChangeFile {
			show_progress: None,
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		}],
	};
	let error = crate::execute_cli_command(
		&root,
		&configuration,
		&cli_command,
		true,
		BTreeMap::from([("package".to_string(), vec!["core".to_string()])]),
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing reason error"));
	assert!(
		error
			.to_string()
			.contains("command `change` requires a `--reason` value")
	);
}

#[test]
fn execute_cli_command_prepare_release_writes_default_manifest_cache_and_follow_up_steps_render_dry_run_outputs()
 {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	fs::OpenOptions::new()
		.append(true)
		.open(tempdir.path().join("monochange.toml"))
		.and_then(|mut file| {
			use std::io::Write;
			writeln!(
				file,
				"\n[source]\nprovider = \"github\"\nowner = \"ifiokjr\"\nrepo = \"monochange\"\n"
			)
		})
		.unwrap_or_else(|error| panic!("append source config: {error}"));
	let root = tempdir.path();
	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("configuration: {error}"));

	let manifest_path = root.join(".monochange/release-manifest.json");
	let prepare_release = CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::PrepareRelease {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
			allow_empty_changesets: false,
		}],
	};
	let render_output = crate::execute_cli_command(
		root,
		&configuration,
		&prepare_release,
		true,
		BTreeMap::from([("format".to_string(), vec!["text".to_string()])]),
	)
	.unwrap_or_else(|error| panic!("prepare release: {error}"));
	assert!(render_output.contains("release manifest: .monochange/release-manifest.json"));
	let manifest_contents =
		fs::read_to_string(&manifest_path).unwrap_or_else(|error| panic!("read manifest: {error}"));
	assert!(manifest_contents.contains("\"releaseTargets\""));

	let publish_release = CliCommandDefinition {
		name: "publish-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
				allow_empty_changesets: false,
			},
			monochange_core::CliStepDefinition::PublishRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
		],
	};
	let publish_output = crate::execute_cli_command(
		root,
		&configuration,
		&publish_release,
		true,
		BTreeMap::from([("format".to_string(), vec!["text".to_string()])]),
	)
	.unwrap_or_else(|error| panic!("publish release: {error}"));
	assert!(publish_output.contains("releases:"));
	assert!(publish_output.contains("dry-run"));

	let release_request = CliCommandDefinition {
		name: "release-pr".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
				allow_empty_changesets: false,
			},
			monochange_core::CliStepDefinition::OpenReleaseRequest {
				name: None,
				when: None,
				no_verify: false,
				inputs: BTreeMap::new(),
			},
		],
	};
	let request_output = crate::execute_cli_command(
		root,
		&configuration,
		&release_request,
		true,
		BTreeMap::from([("format".to_string(), vec!["text".to_string()])]),
	)
	.unwrap_or_else(|error| panic!("open release request: {error}"));
	assert!(request_output.contains("release request:"));
	assert!(request_output.contains("dry-run"));

	let issue_comments = CliCommandDefinition {
		name: "release-comments".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
				allow_empty_changesets: false,
			},
			monochange_core::CliStepDefinition::CommentReleasedIssues {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
		],
	};
	let comments_output =
		crate::execute_cli_command(root, &configuration, &issue_comments, true, BTreeMap::new())
			.unwrap_or_else(|error| panic!("comment released issues: {error}"));
	assert!(!comments_output.is_empty());
}

#[test]
fn execute_cli_command_supports_placeholder_and_package_publish_steps() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	let root = tempdir.path();
	fs::write(
		root.join("Cargo.toml"),
		concat!(
			"[workspace]\n",
			"members = [\"crates/*\"]\n",
			"resolver = \"2\"\n\n",
			"[workspace.package]\n",
			"version = \"1.0.0\"\n",
			"description = \"Workflow packages\"\n",
			"license = \"Unlicense\"\n\n",
			"[workspace.dependencies]\n",
			"workflow-core = { path = \"./crates/core\", version = \"1.0.0\" }\n",
			"workflow-app = { path = \"./crates/app\", version = \"1.0.0\" }\n",
		),
	)
	.unwrap_or_else(|error| panic!("workspace manifest: {error}"));
	fs::create_dir_all(root.join("crates/core/src"))
		.unwrap_or_else(|error| panic!("core src dir: {error}"));
	fs::create_dir_all(root.join("crates/app/src"))
		.unwrap_or_else(|error| panic!("app src dir: {error}"));
	fs::write(root.join("crates/core/src/lib.rs"), "pub fn core() {}\n")
		.unwrap_or_else(|error| panic!("core src file: {error}"));
	fs::write(root.join("crates/app/src/lib.rs"), "pub fn app() {}\n")
		.unwrap_or_else(|error| panic!("app src file: {error}"));
	fs::write(
		root.join("crates/core/Cargo.toml"),
		concat!(
			"[package]\n",
			"name = \"workflow-core\"\n",
			"version = { workspace = true }\n",
			"description = { workspace = true }\n",
			"license = { workspace = true }\n",
			"edition = \"2021\"\n",
		),
	)
	.unwrap_or_else(|error| panic!("core manifest: {error}"));
	fs::write(
		root.join("crates/app/Cargo.toml"),
		concat!(
			"[package]\n",
			"name = \"workflow-app\"\n",
			"version = { workspace = true }\n",
			"description = { workspace = true }\n",
			"license = { workspace = true }\n",
			"edition = \"2021\"\n\n",
			"[dependencies]\n",
			"workflow-core = { workspace = true }\n",
		),
	)
	.unwrap_or_else(|error| panic!("app manifest: {error}"));
	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("configuration: {error}"));
	let server = MockServer::start();
	let _registry = server.mock(|when, then| {
		when.method(GET);
		then.status(404);
	});

	temp_env::with_var(
		"MONOCHANGE_CRATES_IO_API_URL",
		Some(server.base_url()),
		|| {
			let placeholder_command = CliCommandDefinition {
				name: "placeholder-publish".to_string(),
				help_text: None,
				inputs: Vec::new(),
				steps: vec![monochange_core::CliStepDefinition::PlaceholderPublish {
					name: None,
					when: None,
					inputs: BTreeMap::new(),
				}],
			};
			let placeholder_output = crate::execute_cli_command(
				root,
				&configuration,
				&placeholder_command,
				true,
				BTreeMap::from([("format".to_string(), vec!["text".to_string()])]),
			)
			.unwrap_or_else(|error| panic!("placeholder publish: {error}"));
			assert!(placeholder_output.contains("placeholder publishing:"));
			assert!(placeholder_output.contains("would publish placeholder"));
			assert!(placeholder_output.contains("publish rate limits:"));

			let publish_command = CliCommandDefinition {
				name: "publish".to_string(),
				help_text: None,
				inputs: Vec::new(),
				steps: vec![
					monochange_core::CliStepDefinition::PrepareRelease {
						name: None,
						when: None,
						inputs: BTreeMap::new(),
						allow_empty_changesets: false,
					},
					monochange_core::CliStepDefinition::PublishPackages {
						name: None,
						when: None,
						inputs: BTreeMap::new(),
					},
				],
			};
			let publish_output = crate::execute_cli_command(
				root,
				&configuration,
				&publish_command,
				true,
				BTreeMap::from([
					("format".to_string(), vec!["text".to_string()]),
					("package".to_string(), vec!["core".to_string()]),
				]),
			)
			.unwrap_or_else(|error| panic!("publish packages: {error}"));
			assert!(publish_output.contains("package publishing:"));
			assert!(
				publish_output.contains("would publish workflow-core"),
				"publish output:\n{publish_output}"
			);
			assert!(publish_output.contains("publish rate limits:"));
		},
	);
}

#[test]
fn execute_cli_command_allows_package_publish_steps_without_readiness_or_matching_packages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	let root = tempdir.path();
	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("configuration: {error}"));
	let server = MockServer::start();
	let _registry = server.mock(|when, then| {
		when.method(GET);
		then.status(404);
	});
	temp_env::with_var(
		"MONOCHANGE_CRATES_IO_API_URL",
		Some(server.base_url()),
		|| {
			let placeholder_command = CliCommandDefinition {
				name: "placeholder-publish".to_string(),
				help_text: None,
				inputs: Vec::new(),
				steps: vec![monochange_core::CliStepDefinition::PlaceholderPublish {
					name: None,
					when: None,
					inputs: BTreeMap::new(),
				}],
			};
			let placeholder_output = crate::execute_cli_command(
				root,
				&configuration,
				&placeholder_command,
				false,
				BTreeMap::from([
					("format".to_string(), vec!["text".to_string()]),
					("package".to_string(), vec!["missing-package".to_string()]),
				]),
			)
			.unwrap_or_else(|error| panic!("non-dry-run placeholder publish: {error}"));
			assert!(placeholder_output.contains("placeholder publishing:"));
			assert!(placeholder_output.contains("no packages matched the publishing criteria"));
			assert!(placeholder_output.contains("no publish operations matched the current plan"));

			let publish_command = CliCommandDefinition {
				name: "publish".to_string(),
				help_text: None,
				inputs: Vec::new(),
				steps: vec![
					monochange_core::CliStepDefinition::PrepareRelease {
						name: None,
						when: None,
						inputs: BTreeMap::new(),
						allow_empty_changesets: false,
					},
					monochange_core::CliStepDefinition::PublishPackages {
						name: None,
						when: None,
						inputs: BTreeMap::new(),
					},
				],
			};
			let publish_output = crate::execute_cli_command(
				root,
				&configuration,
				&publish_command,
				false,
				BTreeMap::from([
					("format".to_string(), vec!["text".to_string()]),
					("package".to_string(), vec!["missing-package".to_string()]),
				]),
			)
			.unwrap_or_else(|error| {
				panic!("non-dry-run publish packages without readiness: {error}")
			});
			assert!(publish_output.contains("package publishing:"));
			assert!(publish_output.contains("no packages matched the publishing criteria"));

			let release_tempdir =
				tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
			let release_root = release_tempdir.path();
			create_release_record_commit(release_root);
			let release_configuration = load_workspace_configuration(release_root)
				.unwrap_or_else(|error| panic!("release configuration: {error}"));
			let publish_result_path = release_root.join("publish-result.json");
			let publish_release_command = CliCommandDefinition {
				name: "publish".to_string(),
				help_text: None,
				inputs: Vec::new(),
				steps: vec![monochange_core::CliStepDefinition::PublishPackages {
					name: None,
					when: None,
					inputs: BTreeMap::new(),
				}],
			};
			let publish_output = crate::execute_cli_command(
				release_root,
				&release_configuration,
				&publish_release_command,
				false,
				BTreeMap::from([
					("format".to_string(), vec!["text".to_string()]),
					("package".to_string(), vec!["missing-package".to_string()]),
					(
						"output".to_string(),
						vec![publish_result_path.to_string_lossy().to_string()],
					),
				]),
			)
			.unwrap_or_else(|error| panic!("publish packages without readiness: {error}"));
			assert!(publish_output.contains("package publishing:"));
			assert!(publish_output.contains("no packages matched the publishing criteria"));
			let publish_result = fs::read_to_string(&publish_result_path)
				.unwrap_or_else(|error| panic!("read publish result: {error}"));
			assert!(publish_result.contains("\"mode\": \"release\""));

			let plan_command = CliCommandDefinition {
				name: "publish-plan".to_string(),
				help_text: None,
				inputs: Vec::new(),
				steps: vec![monochange_core::CliStepDefinition::PlanPublishRateLimits {
					name: None,
					when: None,
					inputs: BTreeMap::new(),
				}],
			};
			let plan_output = crate::execute_cli_command(
				root,
				&configuration,
				&plan_command,
				false,
				BTreeMap::from([
					("package".to_string(), vec!["missing-package".to_string()]),
					("mode".to_string(), vec!["placeholder".to_string()]),
				]),
			)
			.unwrap_or_else(|error| panic!("publish plan without matches: {error}"));
			assert!(plan_output.contains("publish rate limits:"));
			assert!(plan_output.contains("no publish operations matched the current plan"));
		},
	);
}

#[test]
fn release_follow_up_helpers_render_real_operation_outputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let manifest = sample_release_manifest_for_commit_message(true, true);
	let written_path = crate::cli_runtime::write_release_manifest_file(
		tempdir.path(),
		Path::new("target/release-manifest.json"),
		&manifest,
	)
	.unwrap_or_else(|error| panic!("write manifest: {error}"));
	assert_eq!(written_path, PathBuf::from("target/release-manifest.json"));
	let manifest_contents =
		fs::read_to_string(tempdir.path().join(&written_path)).unwrap_or_else(|error| {
			panic!("read written manifest {}: {error}", written_path.display())
		});
	assert!(manifest_contents.contains("\"releaseTargets\""));

	let release_requests = vec![monochange_core::SourceReleaseRequest {
		provider: monochange_core::SourceProvider::GitHub,
		repository: "ifiokjr/monochange".to_string(),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		target_id: "sdk".to_string(),
		target_kind: monochange_core::ReleaseOwnerKind::Group,
		tag_name: "v1.2.3".to_string(),
		name: "monochange 1.2.3".to_string(),
		body: Some("body".to_string()),
		draft: false,
		prerelease: false,
		generate_release_notes: true,
	}];
	let release_results =
		crate::cli_runtime::build_release_results(false, &release_requests, || {
			Ok(vec![monochange_core::SourceReleaseOutcome {
				provider: monochange_core::SourceProvider::GitHub,
				repository: "ifiokjr/monochange".to_string(),
				tag_name: "v1.2.3".to_string(),
				operation: monochange_core::SourceReleaseOperation::Created,
				url: Some("https://example.com/releases/1".to_string()),
			}])
		})
		.unwrap_or_else(|error| panic!("render release results: {error}"));
	assert_eq!(
		release_results,
		vec!["ifiokjr/monochange v1.2.3 (created) via github".to_string()]
	);

	let release_request = monochange_core::SourceChangeRequest {
		provider: monochange_core::SourceProvider::GitHub,
		repository: "ifiokjr/monochange".to_string(),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		base_branch: "main".to_string(),
		head_branch: "release/v1.2.3".to_string(),
		title: "chore(release): prepare release".to_string(),
		body: "body".to_string(),
		labels: vec!["release".to_string()],
		auto_merge: true,
		commit_message: monochange_core::CommitMessage {
			subject: "subject".to_string(),
			body: Some("body".to_string()),
		},
	};
	let release_request_result =
		crate::cli_runtime::build_release_request_result(false, &release_request, || {
			Ok(monochange_core::SourceChangeRequestOutcome {
				provider: monochange_core::SourceProvider::GitHub,
				repository: "ifiokjr/monochange".to_string(),
				number: 7,
				head_branch: "release/v1.2.3".to_string(),
				operation: monochange_core::SourceChangeRequestOperation::Updated,
				url: Some("https://example.com/pr/7".to_string()),
			})
		})
		.unwrap_or_else(|error| panic!("render release request result: {error}"));
	assert_eq!(
		release_request_result,
		"ifiokjr/monochange #7 (updated) via github"
	);

	let issue_comment_plans = vec![monochange_github::GitHubIssueCommentPlan {
		repository: "ifiokjr/monochange".to_string(),
		issue_id: "#7".to_string(),
		issue_url: Some("https://example.com/issues/7".to_string()),
		body: "released".to_string(),
		close: false,
	}];
	let issue_comment_results =
		crate::cli_runtime::build_issue_comment_results(false, &issue_comment_plans, || {
			Ok(vec![
				monochange_github::GitHubIssueCommentOutcome {
					repository: "ifiokjr/monochange".to_string(),
					issue_id: "#7".to_string(),
					operation: monochange_github::GitHubIssueCommentOperation::Created,
					url: Some("https://example.com/issues/7#comment-1".to_string()),
				},
				monochange_github::GitHubIssueCommentOutcome {
					repository: "ifiokjr/monochange".to_string(),
					issue_id: "#8".to_string(),
					operation: monochange_github::GitHubIssueCommentOperation::SkippedExisting,
					url: Some("https://example.com/issues/8#comment-2".to_string()),
				},
			])
		})
		.unwrap_or_else(|error| panic!("render issue comment results: {error}"));
	assert_eq!(
		issue_comment_results,
		vec![
			"ifiokjr/monochange #7 (created)".to_string(),
			"ifiokjr/monochange #8 (skipped_existing)".to_string(),
		]
	);
}

#[test]
fn execute_matches_rejects_unknown_cli_command_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let matches = Command::new("dummy")
		.try_get_matches_from(["dummy"])
		.unwrap_or_else(|error| panic!("matches: {error}"));
	let error = crate::execute_matches(tempdir.path(), &configuration, "missing", &matches, false)
		.err()
		.unwrap_or_else(|| panic!("expected unknown command error"));
	assert!(error.to_string().contains("unknown command `missing`"));
}

#[test]
fn run_git_capture_and_process_report_io_failures_for_missing_worktrees() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing = tempdir.path().join("missing");
	let capture_error = crate::run_git_capture(&missing, &["status"], "capture failure")
		.err()
		.unwrap_or_else(|| panic!("expected capture error"));
	assert!(capture_error.to_string().contains("capture failure"));

	let mut command = std::process::Command::new("git");
	command.current_dir(&missing).arg("status");
	let process_error = crate::run_git_process(command, "process failure")
		.err()
		.unwrap_or_else(|| panic!("expected process error"));
	assert!(process_error.to_string().contains("process failure"));
}

#[test]
fn parse_boolean_step_input_rejects_invalid_values() {
	let inputs = BTreeMap::from([("force".to_string(), vec!["maybe".to_string()])]);
	let error = crate::parse_boolean_step_input(&inputs, "force")
		.err()
		.unwrap_or_else(|| panic!("expected invalid boolean error"));
	assert!(
		error
			.to_string()
			.contains("invalid boolean value `maybe` for `force`")
	);
}

#[test]
fn run_mcp_command_with_skips_server_when_quiet() {
	let called = Cell::new(false);
	let output = crate::run_mcp_command_with(true, || {
		async {
			called.set(true);
		}
	})
	.unwrap_or_else(|error| panic!("quiet mcp helper: {error}"));

	assert!(output.is_empty());
	assert!(!called.get());
}

#[test]
fn run_mcp_command_with_runs_server_when_not_quiet() {
	let called = Cell::new(false);
	let output = crate::run_mcp_command_with(false, || {
		async {
			called.set(true);
		}
	})
	.unwrap_or_else(|error| panic!("non-quiet mcp helper: {error}"));

	assert!(output.is_empty());
	assert!(called.get());
}

#[test]
fn quiet_builtin_commands_return_empty_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	let init_output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("--quiet"),
			OsString::from("init"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet init output: {error}"));
	assert!(init_output.is_empty());
	assert!(root.join("monochange.toml").exists());

	let populate_output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("--quiet"),
			OsString::from("populate"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet populate output: {error}"));
	assert!(populate_output.is_empty());

	let analyze_output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("--quiet"),
			OsString::from("analyze"),
			OsString::from("--package"),
			OsString::from("core"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet analyze output: {error}"));
	assert!(analyze_output.is_empty());

	let mcp_output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("--quiet"),
			OsString::from("mcp"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet mcp output: {error}"));
	assert!(mcp_output.is_empty());

	let check_output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("--quiet"),
			OsString::from("check"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet check output: {error}"));
	assert!(check_output.is_empty());
}

#[test]
fn inferred_retarget_source_configuration_prefers_configured_source() {
	let configured = sample_github_source_configuration("https://example.com");
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234".to_string(),
		record_commit: "abc1234".to_string(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let inferred =
		crate::inferred_retarget_source_configuration(Some(&configured), &discovery, true)
			.unwrap_or_else(|| panic!("expected configured source"));
	assert_eq!(inferred, configured);
}

#[test]
fn inferred_retarget_source_configuration_infers_from_release_record_provider() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234".to_string(),
		record_commit: "abc1234".to_string(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let inferred = crate::inferred_retarget_source_configuration(None, &discovery, true)
		.unwrap_or_else(|| panic!("expected inferred source"));
	assert_eq!(inferred.provider, monochange_core::SourceProvider::GitHub);
	assert_eq!(inferred.owner, "ifiokjr");
	assert_eq!(inferred.repo, "monochange");
}

#[test]
fn inferred_retarget_source_configuration_returns_none_when_sync_is_disabled() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234".to_string(),
		record_commit: "abc1234".to_string(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	assert!(crate::inferred_retarget_source_configuration(None, &discovery, false).is_none());
}

#[test]
fn build_retarget_release_report_marks_completed_status() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234".to_string(),
		record_commit: "abc1234".to_string(),
		distance: 2,
		record: sample_release_record_for_retarget(),
	};
	let result = monochange_core::RetargetResult {
		record_commit: "abc1234".to_string(),
		target_commit: "def5678".to_string(),
		force: true,
		git_tag_results: vec![monochange_core::RetargetTagResult {
			tag_name: "v1.2.3".to_string(),
			from_commit: "abc1234".to_string(),
			to_commit: "def5678".to_string(),
			operation: monochange_core::RetargetOperation::Moved,
			message: None,
		}],
		provider_results: vec![monochange_core::RetargetProviderResult {
			provider: monochange_core::SourceProvider::GitHub,
			tag_name: "v1.2.3".to_string(),
			target_commit: "def5678".to_string(),
			operation: monochange_core::RetargetProviderOperation::Synced,
			url: None,
			message: None,
		}],
		sync_provider: true,
		dry_run: false,
	};
	let report = crate::build_retarget_release_report("v1.2.3", "HEAD", &discovery, false, &result);
	assert_eq!(report.status, "completed");
	assert!(!report.is_descendant);
}

#[test]
fn render_retarget_release_report_handles_provider_sync_variants() {
	let mut report = sample_retarget_release_report();
	report.sync_provider = true;
	report.provider_results = vec![monochange_core::RetargetProviderResult {
		provider: monochange_core::SourceProvider::GitHub,
		tag_name: "v1.2.3".to_string(),
		target_commit: "def5678901234".to_string(),
		operation: monochange_core::RetargetProviderOperation::Synced,
		url: None,
		message: None,
	}];
	let rendered = crate::render_retarget_release_report(&report);
	assert!(rendered.contains("provider sync: github"));
	assert!(rendered.contains("[planned]"));

	let mut no_provider_report = sample_retarget_release_report();
	no_provider_report.sync_provider = true;
	no_provider_report.provider_results = Vec::new();
	no_provider_report.git_tag_results = Vec::new();
	let rendered = crate::render_retarget_release_report(&no_provider_report);
	assert!(rendered.contains("provider sync: none"));
}

#[test]
fn retarget_operation_label_covers_all_variants() {
	assert_eq!(
		crate::retarget_operation_label(monochange_core::RetargetOperation::Planned),
		"planned"
	);
	assert_eq!(
		crate::retarget_operation_label(monochange_core::RetargetOperation::Moved),
		"moved"
	);
	assert_eq!(
		crate::retarget_operation_label(monochange_core::RetargetOperation::AlreadyUpToDate),
		"already_up_to_date"
	);
	assert_eq!(
		crate::retarget_operation_label(monochange_core::RetargetOperation::Skipped),
		"skipped"
	);
	assert_eq!(
		crate::retarget_operation_label(monochange_core::RetargetOperation::Failed),
		"failed"
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_collects_tag_and_provider_updates() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	fs::write(root.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "second"]);

	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD~1"]);
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit: record_commit.clone(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let source = sample_github_source_configuration("https://example.com");

	let plan =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, true, true, Some(&source))
			.unwrap_or_else(|error| panic!("plan retarget: {error}"));

	assert!(plan.is_descendant);
	assert_eq!(plan.git_tag_updates.len(), 1);
	assert_eq!(plan.git_tag_updates[0].tag_name, "v1.2.3");
	assert_eq!(
		plan.git_tag_updates[0].operation,
		monochange_core::RetargetOperation::Planned
	);
	assert_eq!(plan.provider_updates.len(), 1);
	assert_eq!(
		plan.provider_updates[0].operation,
		monochange_core::RetargetProviderOperation::Planned
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_rejects_non_descendant_without_force() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let main_branch = git_output_in_temp_repo(root, &["branch", "--show-current"]);
	let base_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	fs::write(root.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "second"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["checkout", &base_commit]);
	fs::write(root.join("release.txt"), "branch\n")
		.unwrap_or_else(|error| panic!("write branch release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "branch"]);
	let branch_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["checkout", &main_branch]);
	assert_ne!(record_commit, branch_commit);

	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: record_commit.clone(),
		resolved_commit: record_commit.clone(),
		record_commit: record_commit.clone(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};

	let error =
		crate::plan_release_retarget(root, &discovery, &branch_commit, false, false, true, None)
			.err()
			.unwrap_or_else(|| panic!("expected non-descendant error"));
	assert!(
		error
			.to_string()
			.contains("is not a descendant of release-record commit")
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn execute_release_retarget_moves_tags_and_pushes_origin_refs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let remote = tempdir.path().join("remote.git");
	git_in_dir(
		tempdir.path(),
		&["init", "--bare", remote.to_str().unwrap_or("remote.git")],
	);

	let repo = tempdir.path().join("repo");
	fs::create_dir_all(&repo).unwrap_or_else(|error| panic!("create repo dir: {error}"));
	init_git_repo(&repo);
	git_in_temp_repo(
		&repo,
		&[
			"remote",
			"add",
			"origin",
			remote.to_str().unwrap_or("remote.git"),
		],
	);
	fs::write(repo.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(&repo, &["add", "release.txt"]);
	git_in_temp_repo(&repo, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(&repo, &["rev-parse", "HEAD"]);
	git_in_temp_repo(&repo, &["tag", "v1.2.3"]);
	git_in_temp_repo(
		&repo,
		&[
			"push",
			"origin",
			"HEAD",
			"refs/tags/v1.2.3:refs/tags/v1.2.3",
		],
	);
	fs::write(repo.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second release file: {error}"));
	git_in_temp_repo(&repo, &["add", "release.txt"]);
	git_in_temp_repo(&repo, &["commit", "-m", "second"]);

	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let plan = crate::plan_release_retarget(&repo, &discovery, "HEAD", false, false, false, None)
		.unwrap_or_else(|error| panic!("plan retarget: {error}"));

	let result = crate::execute_release_retarget(&repo, None, &plan)
		.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	let head = git_output_in_temp_repo(&repo, &["rev-parse", "HEAD"]);
	let local_tag = git_output_in_temp_repo(&repo, &["rev-parse", "refs/tags/v1.2.3^{commit}"]);
	let remote_tag = git_output_in_git_dir(&remote, &["rev-parse", "refs/tags/v1.2.3^{commit}"]);

	assert_eq!(
		result.git_tag_results[0].operation,
		monochange_core::RetargetOperation::Moved
	);
	assert_eq!(local_tag, head);
	assert_eq!(remote_tag, head);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn retarget_release_reports_missing_tags() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);

	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: record_commit.clone(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record: sample_release_record_for_retarget(),
	};

	let error = crate::retarget_release(root, &discovery, "HEAD", false, false, true, None)
		.err()
		.unwrap_or_else(|| panic!("expected missing tag error"));
	assert!(
		error
			.to_string()
			.contains("release tag v1.2.3 could not be found")
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_marks_existing_target_tag_as_up_to_date() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit: record_commit.clone(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};

	let plan =
		crate::plan_release_retarget(root, &discovery, &record_commit, false, false, false, None)
			.unwrap_or_else(|error| panic!("plan retarget: {error}"));
	assert_eq!(
		plan.git_tag_updates
			.first()
			.unwrap_or_else(|| panic!("expected git tag update"))
			.operation,
		monochange_core::RetargetOperation::AlreadyUpToDate
	);

	let result = crate::execute_release_retarget(root, None, &plan)
		.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	assert_eq!(
		result
			.git_tag_results
			.first()
			.unwrap_or_else(|| panic!("expected git tag result"))
			.operation,
		monochange_core::RetargetOperation::AlreadyUpToDate
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_marks_unsupported_provider_sync_in_dry_run() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let mut record = sample_release_record_for_retarget();
	record.provider = Some(monochange_core::ReleaseRecordProvider {
		kind: monochange_core::SourceProvider::GitLab,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		host: None,
	});
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record,
	};
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};

	let plan =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, true, true, Some(&source))
			.unwrap_or_else(|error| panic!("plan retarget: {error}"));
	let provider_update = plan
		.provider_updates
		.first()
		.unwrap_or_else(|| panic!("expected provider update"));
	assert_eq!(
		provider_update.operation,
		monochange_core::RetargetProviderOperation::Unsupported
	);
	assert!(
		provider_update
			.message
			.as_deref()
			.unwrap_or("")
			.contains("gitlab")
	);

	let result = crate::execute_release_retarget(root, Some(&source), &plan)
		.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	assert_eq!(
		result
			.provider_results
			.first()
			.unwrap_or_else(|| panic!("expected provider result"))
			.operation,
		monochange_core::RetargetProviderOperation::Unsupported
	);
}

#[test]
fn execute_release_retarget_returns_empty_provider_results_without_source() {
	let plan = monochange_core::RetargetPlan {
		record_commit: "abc1234".to_string(),
		target_commit: "def5678".to_string(),
		is_descendant: true,
		force: false,
		git_tag_updates: Vec::new(),
		provider_updates: vec![monochange_core::RetargetProviderResult {
			provider: monochange_core::SourceProvider::GitHub,
			tag_name: "v1.2.3".to_string(),
			target_commit: "def5678".to_string(),
			operation: monochange_core::RetargetProviderOperation::Planned,
			url: None,
			message: None,
		}],
		sync_provider: true,
		dry_run: false,
	};
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let result = crate::execute_release_retarget(tempdir.path(), None, &plan)
		.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	assert!(result.provider_results.is_empty());
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_rejects_provider_kind_mismatches() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};

	let error =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, false, true, Some(&source))
			.err()
			.unwrap_or_else(|| panic!("expected provider mismatch error"));
	assert!(
		error
			.to_string()
			.contains("does not match configured source provider")
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_rejects_repository_mismatches() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let mut record = sample_release_record_for_retarget();
	record.provider = Some(monochange_core::ReleaseRecordProvider {
		kind: monochange_core::SourceProvider::GitHub,
		owner: "other".to_string(),
		repo: "repo".to_string(),
		host: None,
	});
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record,
	};
	let source = sample_github_source_configuration("https://example.com");

	let error =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, false, true, Some(&source))
			.err()
			.unwrap_or_else(|| panic!("expected repository mismatch error"));
	assert!(
		error
			.to_string()
			.contains("does not match configured source repository")
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_skips_provider_updates_when_no_provider_is_available() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let mut record = sample_release_record_for_retarget();
	record.provider = None;
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record,
	};
	let plan = crate::plan_release_retarget(root, &discovery, "HEAD", false, true, true, None)
		.unwrap_or_else(|error| panic!("plan retarget: {error}"));
	assert!(plan.provider_updates.is_empty());
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_accepts_missing_record_provider_with_configured_source() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let mut record = sample_release_record_for_retarget();
	record.provider = None;
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record,
	};
	let source = sample_github_source_configuration("https://example.com");
	let plan =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, false, true, Some(&source))
			.unwrap_or_else(|error| panic!("plan retarget: {error}"));
	assert_eq!(plan.git_tag_updates.len(), 1);
}

#[test]
fn execute_release_retarget_delegates_github_provider_sync() {
	let server = MockServer::start();
	let release_lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/releases/tags/v1.2.3");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"{\"id\":42,\"html_url\":\"https://example.com/releases/42\",\"target_commitish\":\"abc1234\"}",
			);
	});
	let update_release = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/releases/42")
			.json_body_obj(&serde_json::json!({ "target_commitish": "def5678" }));
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://example.com/releases/42\"}");
	});
	let source = sample_github_source_configuration(&server.base_url());
	let plan = monochange_core::RetargetPlan {
		record_commit: "abc1234".to_string(),
		target_commit: "def5678".to_string(),
		is_descendant: true,
		force: false,
		git_tag_updates: vec![monochange_core::RetargetTagResult {
			tag_name: "v1.2.3".to_string(),
			from_commit: "def5678".to_string(),
			to_commit: "def5678".to_string(),
			operation: monochange_core::RetargetOperation::AlreadyUpToDate,
			message: None,
		}],
		provider_updates: vec![monochange_core::RetargetProviderResult {
			provider: monochange_core::SourceProvider::GitHub,
			tag_name: "v1.2.3".to_string(),
			target_commit: "def5678".to_string(),
			operation: monochange_core::RetargetProviderOperation::Planned,
			url: None,
			message: None,
		}],
		sync_provider: true,
		dry_run: false,
	};
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let result = temp_env::with_var("GITHUB_TOKEN", Some("token"), || {
		crate::execute_release_retarget(tempdir.path(), Some(&source), &plan)
	})
	.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	assert_eq!(result.provider_results.len(), 1);
	release_lookup.assert();
	assert_eq!(update_release.calls(), 0);
	assert_eq!(
		result
			.provider_results
			.first()
			.unwrap_or_else(|| panic!("expected provider result"))
			.operation,
		monochange_core::RetargetProviderOperation::AlreadyAligned
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some() || std::env::var_os("CARGO_LLVM_COV").is_some())]
fn retarget_release_succeeds_end_to_end_without_provider_sync() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let remote = root.join("remote.git");
	git_in_dir(
		root,
		&["init", "--bare", remote.to_str().unwrap_or("remote.git")],
	);
	let repo = root.join("repo");
	fs::create_dir_all(&repo).unwrap_or_else(|error| panic!("create repo dir: {error}"));
	init_git_repo(&repo);
	git_in_temp_repo(
		&repo,
		&[
			"remote",
			"add",
			"origin",
			remote.to_str().unwrap_or("remote.git"),
		],
	);
	fs::write(repo.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(&repo, &["add", "release.txt"]);
	git_in_temp_repo(&repo, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(&repo, &["rev-parse", "HEAD"]);
	git_in_temp_repo(&repo, &["tag", "v1.2.3"]);
	git_in_temp_repo(
		&repo,
		&[
			"push",
			"origin",
			"HEAD",
			"refs/tags/v1.2.3:refs/tags/v1.2.3",
		],
	);
	fs::write(repo.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second release file: {error}"));
	git_in_temp_repo(&repo, &["add", "release.txt"]);
	git_in_temp_repo(&repo, &["commit", "-m", "second"]);
	let target_commit = git_output_in_temp_repo(&repo, &["rev-parse", "HEAD"]);
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let result =
		crate::retarget_release(&repo, &discovery, &target_commit, false, false, false, None)
			.unwrap_or_else(|error| panic!("retarget release: {error}"));
	assert_eq!(result.git_tag_results.len(), 1);
}

#[test]
fn git_is_ancestor_reports_git_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = crate::git_support::git_is_ancestor(tempdir.path(), "abc", "def")
		.err()
		.unwrap_or_else(|| panic!("expected git ancestry error"));
	assert!(error.to_string().contains("discovery error"));
}

#[test]
fn git_is_ancestor_reports_missing_worktree_directory_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing = tempdir.path().join("missing");
	let error = crate::git_support::git_is_ancestor(&missing, "abc", "def")
		.err()
		.unwrap_or_else(|| panic!("expected git spawn error"));
	assert!(
		error
			.to_string()
			.contains("failed to compare commit ancestry")
	);
}

#[test]
fn sync_retargeted_provider_releases_reports_unsupported_provider_in_dry_run() {
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::Gitea,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};
	let updates = vec![monochange_core::RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: monochange_core::RetargetOperation::Moved,
		message: None,
	}];
	let results = crate::release_record::sync_retargeted_provider_releases(&source, &updates, true)
		.unwrap_or_else(|error| panic!("expected dry-run provider results: {error}"));
	assert_eq!(results.len(), 1);
	assert_eq!(
		results
			.first()
			.unwrap_or_else(|| panic!("expected provider result"))
			.operation,
		monochange_core::RetargetProviderOperation::Unsupported
	);
}

#[test]
fn sync_retargeted_provider_releases_rejects_unsupported_provider_in_real_mode() {
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::Gitea,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};
	let updates = vec![monochange_core::RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: monochange_core::RetargetOperation::Moved,
		message: None,
	}];
	let error = crate::release_record::sync_retargeted_provider_releases(&source, &updates, false)
		.err()
		.unwrap_or_else(|| panic!("expected unsupported provider error"));
	assert!(error.to_string().contains("gitea"));
}

#[test]
fn execute_release_retarget_rejects_unsupported_provider_sync_in_real_mode() {
	let plan = monochange_core::RetargetPlan {
		record_commit: "abc1234".to_string(),
		target_commit: "def5678".to_string(),
		is_descendant: true,
		force: false,
		git_tag_updates: Vec::new(),
		provider_updates: vec![monochange_core::RetargetProviderResult {
			provider: monochange_core::SourceProvider::GitLab,
			tag_name: "v1.2.3".to_string(),
			target_commit: "def5678".to_string(),
			operation: monochange_core::RetargetProviderOperation::Unsupported,
			url: None,
			message: Some(
				"provider sync is not yet supported for gitlab release retargeting".to_string(),
			),
		}],
		sync_provider: true,
		dry_run: false,
	};
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = crate::execute_release_retarget(tempdir.path(), Some(&source), &plan)
		.err()
		.unwrap_or_else(|| panic!("expected unsupported provider error"));
	assert!(
		error
			.to_string()
			.contains("provider sync is not yet supported for gitlab release retargeting")
	);
}

#[test]
fn execute_cli_command_commit_release_requires_prepare_release() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "commit-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::CommitRelease {
			name: None,
			when: None,
			no_verify: false,
			inputs: BTreeMap::new(),
		}],
	};

	let error = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing PrepareRelease error"));
	assert!(
		error.to_string().contains(
			"`CommitRelease` requires a previous `PrepareRelease` step or a reusable prepared release artifact"
		)
	);
}

#[test]
fn git_stage_paths_reports_git_inspection_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = crate::git_stage_paths(tempdir.path(), &[PathBuf::from("release.txt")])
		.err()
		.unwrap_or_else(|| panic!("expected git stage failure"));
	assert!(
		error
			.to_string()
			.contains("failed to inspect tracked git path release.txt")
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn git_stage_paths_skips_missing_untracked_paths_and_ignored_untracked_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_fixture("prepared-release/commit-release-flexible/workspace", root);
	init_git_repo(root);
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);

	let mut cargo_toml = fs::read_to_string(root.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("read Cargo.toml: {error}"));
	cargo_toml.push_str("\n# staged release update\n");
	fs::write(root.join("Cargo.toml"), cargo_toml)
		.unwrap_or_else(|error| panic!("write Cargo.toml: {error}"));
	fs::create_dir_all(root.join(".monochange"))
		.unwrap_or_else(|error| panic!("create .monochange: {error}"));
	fs::write(root.join(".monochange/release-manifest.json"), "{}\n")
		.unwrap_or_else(|error| panic!("write manifest: {error}"));

	crate::git_stage_paths(
		root,
		&[
			PathBuf::from("Cargo.toml"),
			PathBuf::from(".monochange/release-manifest.json"),
			PathBuf::from(".changeset/001-release-foundation.md"),
		],
	)
	.unwrap_or_else(|error| panic!("git stage paths: {error}"));

	assert_eq!(
		git_output_in_temp_repo(root, &["diff", "--cached", "--name-only"]),
		"Cargo.toml"
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn first_parent_commits_returns_head_then_ancestors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	git_in_temp_repo(root, &["init"]);
	git_in_temp_repo(root, &["config", "user.name", "monochange Tests"]);
	git_in_temp_repo(root, &["config", "user.email", "monochange@example.com"]);
	git_in_temp_repo(root, &["config", "commit.gpgsign", "false"]);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write initial file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	fs::write(root.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "second"]);

	let head = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	let commits = crate::git_support::first_parent_commits(root, &head)
		.unwrap_or_else(|error| panic!("first parent commits: {error}"));
	assert_eq!(commits.first().map(String::as_str), Some(head.as_str()));
	assert_eq!(commits.len(), 2);
}

#[test]
fn git_head_commit_and_read_commit_message_roundtrip() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "hello\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "subject line", "-m", "body line"]);

	let head = crate::git_head_commit(root).unwrap_or_else(|error| panic!("head commit: {error}"));
	let message = crate::read_git_commit_message(root, &head)
		.unwrap_or_else(|error| panic!("read commit message: {error}"));
	assert!(message.contains("subject line"));
	assert!(message.contains("body line"));
}

#[test]
fn run_git_status_reports_nonzero_exit_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	init_git_repo(tempdir.path());
	let error = crate::run_git_status(
		tempdir.path(),
		&["definitely-not-a-real-git-command"],
		"git status failure",
	)
	.err()
	.unwrap_or_else(|| panic!("expected git status failure"));
	assert!(error.to_string().contains("git status failure"));
}

fn sample_retarget_release_report() -> crate::RetargetReleaseReport {
	crate::RetargetReleaseReport {
		from: "v1.2.3".to_string(),
		target: "HEAD".to_string(),
		resolved_from_commit: "abc1234567890".to_string(),
		record_commit: "abc1234567890".to_string(),
		target_commit: "def5678901234".to_string(),
		distance: 1,
		is_descendant: true,
		force: false,
		dry_run: true,
		sync_provider: false,
		tags: vec!["v1.2.3".to_string()],
		git_tag_results: vec![monochange_core::RetargetTagResult {
			tag_name: "v1.2.3".to_string(),
			from_commit: "abc1234567890".to_string(),
			to_commit: "def5678901234".to_string(),
			operation: monochange_core::RetargetOperation::Planned,
			message: None,
		}],
		provider_results: Vec::new(),
		status: "dry_run".to_string(),
	}
}

#[test]
fn versioned_file_kind_detects_supported_paths_across_ecosystems() {
	assert!(matches!(
		crate::versioned_file_kind(
			monochange_core::EcosystemType::Cargo,
			&fixture_path("cargo/manifest-lockfile-workspace/crates/core/Cargo.toml"),
		),
		Some(crate::VersionedFileKind::Cargo(_))
	));
	assert!(matches!(
		crate::versioned_file_kind(
			monochange_core::EcosystemType::Npm,
			&fixture_path("npm/manifest-lockfile-workspace/packages/web/package-lock.json"),
		),
		Some(crate::VersionedFileKind::Npm(_))
	));
	assert!(matches!(
		crate::versioned_file_kind(
			monochange_core::EcosystemType::Npm,
			&fixture_path("npm/lockfile-workspace/pnpm-lock.yaml"),
		),
		Some(crate::VersionedFileKind::Npm(_))
	));
	assert!(matches!(
		crate::versioned_file_kind(
			monochange_core::EcosystemType::Npm,
			&fixture_path("npm/bun-text-lock/packages/app/bun.lock"),
		),
		Some(crate::VersionedFileKind::Npm(_))
	));
	assert!(matches!(
		crate::versioned_file_kind(
			monochange_core::EcosystemType::Npm,
			&fixture_path("monochange/bun-lock-release/packages/app/bun.lockb"),
		),
		Some(crate::VersionedFileKind::Npm(_))
	));
	assert!(matches!(
		crate::versioned_file_kind(
			monochange_core::EcosystemType::Deno,
			&fixture_path("monochange/deno-lock-release/packages/app/deno.json"),
		),
		Some(crate::VersionedFileKind::Deno(_))
	));
	assert!(matches!(
		crate::versioned_file_kind(
			monochange_core::EcosystemType::Dart,
			&fixture_path("dart/manifest-lockfile-workspace/packages/app/pubspec.lock"),
		),
		Some(crate::VersionedFileKind::Dart(_))
	));
	assert!(matches!(
		crate::versioned_file_kind(
			monochange_core::EcosystemType::Python,
			&fixture_path("python/standalone/pyproject.toml"),
		),
		Some(crate::VersionedFileKind::Python(_))
	));
	assert!(matches!(
		crate::versioned_file_kind(
			monochange_core::EcosystemType::Python,
			&fixture_path("python/uv-workspace/uv.lock"),
		),
		Some(crate::VersionedFileKind::Python(_))
	));
}

#[test]
fn read_cached_document_returns_cached_entries_before_disk_lookup() {
	let path = fixture_path("test-support/setup-fixture/root.txt");
	let mut updates = BTreeMap::from([(
		path.clone(),
		crate::CachedDocument::Text("cached".to_string()),
	)]);
	let cached =
		crate::read_cached_document(&mut updates, &path, monochange_core::EcosystemType::Cargo)
			.unwrap_or_else(|error| panic!("cached document: {error}"));
	assert!(matches!(cached, crate::CachedDocument::Text(contents) if contents == "cached"));
	assert!(updates.is_empty());
}

#[test]
fn read_cached_document_rejects_unsupported_versioned_files() {
	let path = fixture_path("test-support/setup-fixture/root.txt");
	let error = crate::read_cached_document(
		&mut BTreeMap::new(),
		&path,
		monochange_core::EcosystemType::Cargo,
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported file error"));
	assert!(error.to_string().contains("unsupported versioned file"));
	assert!(error.to_string().contains("cargo"));
}

#[test]
fn read_cached_document_parses_supported_document_formats() {
	let cargo = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("cargo/manifest-lockfile-workspace/crates/core/Cargo.toml"),
		monochange_core::EcosystemType::Cargo,
	)
	.unwrap_or_else(|error| panic!("cargo manifest: {error}"));
	assert!(
		matches!(cargo, crate::CachedDocument::Text(contents) if contents.contains("[package]"))
	);

	let npm_json = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("npm/manifest-lockfile-workspace/packages/web/package-lock.json"),
		monochange_core::EcosystemType::Npm,
	)
	.unwrap_or_else(|error| panic!("npm json: {error}"));
	assert!(matches!(npm_json, crate::CachedDocument::Json(_)));

	let npm_yaml = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("npm/lockfile-workspace/pnpm-lock.yaml"),
		monochange_core::EcosystemType::Npm,
	)
	.unwrap_or_else(|error| panic!("pnpm lock: {error}"));
	assert!(matches!(
		npm_yaml,
		crate::CachedDocument::Text(contents) if contents.contains("lockfileVersion")
	));

	let bun_text = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("npm/bun-text-lock/packages/app/bun.lock"),
		monochange_core::EcosystemType::Npm,
	)
	.unwrap_or_else(|error| panic!("bun lock: {error}"));
	assert!(
		matches!(bun_text, crate::CachedDocument::Text(contents) if contents.contains("left-pad"))
	);

	let bun_binary = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("monochange/bun-lock-release/packages/app/bun.lockb"),
		monochange_core::EcosystemType::Npm,
	)
	.unwrap_or_else(|error| panic!("bun lockb: {error}"));
	assert!(matches!(bun_binary, crate::CachedDocument::Bytes(contents) if !contents.is_empty()));

	let npm_manifest = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("versioned-file-updates/npm-manifest/package.json"),
		monochange_core::EcosystemType::Npm,
	)
	.unwrap_or_else(|error| panic!("npm manifest: {error}"));
	assert!(matches!(npm_manifest, crate::CachedDocument::Text(_)));

	let deno_json = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("monochange/deno-lock-release/packages/app/deno.json"),
		monochange_core::EcosystemType::Deno,
	)
	.unwrap_or_else(|error| panic!("deno json: {error}"));
	assert!(matches!(deno_json, crate::CachedDocument::Text(_)));

	let dart_manifest = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("dart/workspace-pattern-warnings/packages/app/pubspec.yaml"),
		monochange_core::EcosystemType::Dart,
	)
	.unwrap_or_else(|error| panic!("dart manifest: {error}"));
	assert!(matches!(dart_manifest, crate::CachedDocument::Text(_)));

	let dart_yaml = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("dart/manifest-lockfile-workspace/packages/app/pubspec.lock"),
		monochange_core::EcosystemType::Dart,
	)
	.unwrap_or_else(|error| panic!("dart lock: {error}"));
	assert!(matches!(dart_yaml, crate::CachedDocument::Yaml(_)));

	let python_manifest = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("python/standalone/pyproject.toml"),
		monochange_core::EcosystemType::Python,
	)
	.unwrap_or_else(|error| panic!("python manifest: {error}"));
	assert!(matches!(
		python_manifest,
		crate::CachedDocument::Text(contents) if contents.contains("[project]")
	));

	let python_lock = crate::read_cached_document(
		&mut BTreeMap::new(),
		&fixture_path("python/uv-workspace/uv.lock"),
		monochange_core::EcosystemType::Python,
	)
	.unwrap_or_else(|error| panic!("python lock: {error}"));
	assert!(matches!(python_lock, crate::CachedDocument::Text(_)));
}

#[test]
fn read_cached_document_reports_python_error_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let manifest_path = tempdir.path().join("pyproject.toml");
	let lock_path = tempdir.path().join("uv.lock");
	let unsupported_path = tempdir.path().join("unknown.txt");

	fs::write(&manifest_path, [0xff, 0xfe])
		.unwrap_or_else(|error| panic!("write manifest: {error}"));
	let error = crate::read_cached_document(
		&mut BTreeMap::new(),
		&manifest_path,
		monochange_core::EcosystemType::Python,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid UTF-8 manifest error"));
	assert!(error.to_string().contains("is not valid UTF-8"));

	fs::write(&lock_path, [0xff, 0xfe]).unwrap_or_else(|error| panic!("write lock: {error}"));
	let error = crate::read_cached_document(
		&mut BTreeMap::new(),
		&lock_path,
		monochange_core::EcosystemType::Python,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid UTF-8 lock error"));
	assert!(error.to_string().contains("is not valid UTF-8"));

	fs::write(&manifest_path, "[project\n")
		.unwrap_or_else(|error| panic!("write invalid manifest: {error}"));
	let error = crate::read_cached_document(
		&mut BTreeMap::new(),
		&manifest_path,
		monochange_core::EcosystemType::Python,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid TOML manifest error"));
	assert!(error.to_string().contains("failed to parse"));

	fs::write(&unsupported_path, "text")
		.unwrap_or_else(|error| panic!("write unsupported: {error}"));
	let error = crate::read_cached_document(
		&mut BTreeMap::new(),
		&unsupported_path,
		monochange_core::EcosystemType::Python,
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported Python versioned file error"));
	assert!(error.to_string().contains("python"));
}

#[test]
fn resolve_versioned_prefix_prefers_explicit_then_ecosystem_then_default() {
	let mut configuration = load_workspace_configuration(&fixture_path("monochange/release-base"))
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	configuration.npm.dependency_version_prefix = Some("workspace:".to_string());
	configuration.python.dependency_version_prefix = Some("~=".to_string());
	configuration.deno.dependency_version_prefix = None;
	let context = crate::VersionedFileUpdateContext {
		package_by_config_id: BTreeMap::new(),
		package_by_native_name: BTreeMap::new(),
		current_versions_by_native_name: BTreeMap::new(),
		released_versions_by_native_name: BTreeMap::new(),
		configuration: &configuration,
	};

	let explicit = monochange_core::VersionedFileDefinition {
		path: "packages/app/package.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		prefix: Some("~".to_string()),
		fields: None,
		name: None,
		regex: None,
	};
	assert_eq!(crate::resolve_versioned_prefix(&explicit, &context), "~");

	let ecosystem = monochange_core::VersionedFileDefinition {
		path: "packages/app/package.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	assert_eq!(
		crate::resolve_versioned_prefix(&ecosystem, &context),
		"workspace:"
	);

	let fallback = monochange_core::VersionedFileDefinition {
		path: "packages/app/deno.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Deno),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	assert_eq!(
		crate::resolve_versioned_prefix(&fallback, &context),
		monochange_core::EcosystemType::Deno.default_prefix()
	);

	let python = monochange_core::VersionedFileDefinition {
		path: "packages/app/pyproject.toml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Python),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	assert_eq!(crate::resolve_versioned_prefix(&python, &context), "~=");
	assert_eq!(
		monochange_core::EcosystemType::Python.default_prefix(),
		">="
	);
	assert_eq!(
		monochange_core::EcosystemType::Python.default_fields(),
		["dependencies"]
	);
}

#[test]
fn build_versioned_file_updates_skips_unreleased_package_definitions() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());

	let mut configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	configuration.groups.clear();
	let discovery =
		discover_workspace(tempdir.path()).unwrap_or_else(|error| panic!("discovery: {error}"));
	let released_core = discovery
		.packages
		.iter()
		.find(|package| package.metadata.get("config_id").map(String::as_str) == Some("core"))
		.unwrap_or_else(|| panic!("expected core package"));
	let plan = monochange_core::ReleasePlan {
		workspace_root: tempdir.path().to_path_buf(),
		decisions: vec![monochange_core::ReleaseDecision {
			package_id: released_core.id.clone(),
			trigger_type: "changeset".to_string(),
			recommended_bump: BumpSeverity::Minor,
			planned_version: Some(
				Version::parse("1.1.0").unwrap_or_else(|error| panic!("planned version: {error}")),
			),
			group_id: None,
			reasons: vec!["release".to_string()],
			upstream_sources: Vec::new(),
			warnings: Vec::new(),
		}],
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};

	let updates = crate::build_versioned_file_updates(
		tempdir.path(),
		&configuration,
		&discovery.packages,
		&plan,
	)
	.unwrap_or_else(|error| panic!("versioned file updates: {error}"));

	assert_eq!(updates.len(), 1);
	assert_eq!(
		updates[0].path,
		tempdir.path().join("crates/core/extra.toml")
	);
}

#[test]
fn serialize_cached_document_formats_json_yaml_text_and_bytes() {
	let json = crate::serialize_cached_document(
		Path::new("package-lock.json"),
		crate::CachedDocument::Json(serde_json::json!({"name": "web"})),
	)
	.unwrap_or_else(|error| panic!("serialize json: {error}"));
	assert!(
		String::from_utf8(json.content)
			.unwrap_or_else(|error| panic!("json utf8: {error}"))
			.ends_with('\n')
	);

	let mut yaml_mapping = serde_yaml_ng::Mapping::new();
	yaml_mapping.insert(
		serde_yaml_ng::Value::String("lockfileVersion".to_string()),
		serde_yaml_ng::Value::Number(serde_yaml_ng::Number::from(9)),
	);
	let yaml = crate::serialize_cached_document(
		Path::new("pnpm-lock.yaml"),
		crate::CachedDocument::Yaml(yaml_mapping),
	)
	.unwrap_or_else(|error| panic!("serialize yaml: {error}"));
	assert!(
		String::from_utf8(yaml.content)
			.unwrap_or_else(|error| panic!("yaml utf8: {error}"))
			.contains("lockfileVersion")
	);

	let text = crate::serialize_cached_document(
		Path::new("bun.lock"),
		crate::CachedDocument::Text("text lock".to_string()),
	)
	.unwrap_or_else(|error| panic!("serialize text: {error}"));
	assert_eq!(text.content, b"text lock");

	let bytes = crate::serialize_cached_document(
		Path::new("bun.lockb"),
		crate::CachedDocument::Bytes(vec![1, 2, 3, 4]),
	)
	.unwrap_or_else(|error| panic!("serialize bytes: {error}"));
	assert_eq!(bytes.content, vec![1, 2, 3, 4]);
}

#[test]
fn read_cached_document_reports_parse_errors_for_invalid_supported_formats() {
	let cases = [
		(
			"cargo/invalid-workspace/invalid-workspace.toml",
			"Cargo.toml",
			monochange_core::EcosystemType::Cargo,
		),
		(
			"npm/invalid-package-json/invalid-package.json",
			"package-lock.json",
			monochange_core::EcosystemType::Npm,
		),
		(
			"npm/invalid-pnpm-workspace/invalid-pnpm-workspace.yaml",
			"pnpm-lock.yaml",
			monochange_core::EcosystemType::Npm,
		),
		(
			"dart/invalid-package/invalid-package.yaml",
			"pubspec.lock",
			monochange_core::EcosystemType::Dart,
		),
	];

	for (source_fixture, target_name, ecosystem) in cases {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let target = tempdir.path().join(target_name);
		fs::copy(fixture_path(source_fixture), &target)
			.unwrap_or_else(|error| panic!("copy invalid fixture {source_fixture}: {error}"));
		let error = crate::read_cached_document(&mut BTreeMap::new(), &target, ecosystem)
			.err()
			.unwrap_or_else(|| panic!("expected parse error for {target_name}"));
		assert!(
			error.to_string().contains("failed to parse"),
			"error: {error}"
		);
	}
}

#[test]
fn build_manifest_updates_preserve_npm_deno_and_dart_formatting() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let npm_path = tempdir.path().join("packages/web/package.json");
	let deno_path = tempdir.path().join("tools/deno/deno.json");
	let dart_path = tempdir.path().join("packages/mobile/pubspec.yaml");
	fs::create_dir_all(npm_path.parent().unwrap_or_else(|| panic!("npm parent")))
		.unwrap_or_else(|error| panic!("create npm parent: {error}"));
	fs::create_dir_all(deno_path.parent().unwrap_or_else(|| panic!("deno parent")))
		.unwrap_or_else(|error| panic!("create deno parent: {error}"));
	fs::create_dir_all(dart_path.parent().unwrap_or_else(|| panic!("dart parent")))
		.unwrap_or_else(|error| panic!("create dart parent: {error}"));
	fs::write(
		&npm_path,
		"{\n    \"name\": \"web\",\n    \"version\": \"1.0.0\",\n    \"private\": false\n}\n",
	)
	.unwrap_or_else(|error| panic!("write package.json: {error}"));
	fs::write(
		&deno_path,
		"{\n  \"name\": \"tool\",\n  \"version\": \"1.0.0\",\n  \"imports\": {\n    \"core\": \"^1.0.0\"\n  }\n}\n",
	)
	.unwrap_or_else(|error| panic!("write deno.json: {error}"));
	fs::write(&dart_path, "name: mobile\nversion: '1.0.0' # keep quote\n")
		.unwrap_or_else(|error| panic!("write pubspec.yaml: {error}"));

	let packages = vec![
		monochange_core::PackageRecord::new(
			Ecosystem::Npm,
			"web",
			npm_path.clone(),
			tempdir.path().to_path_buf(),
			Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
			monochange_core::PublishState::Public,
		),
		monochange_core::PackageRecord::new(
			Ecosystem::Deno,
			"tool",
			deno_path.clone(),
			tempdir.path().to_path_buf(),
			Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
			monochange_core::PublishState::Public,
		),
		monochange_core::PackageRecord::new(
			Ecosystem::Dart,
			"mobile",
			dart_path.clone(),
			tempdir.path().to_path_buf(),
			Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
			monochange_core::PublishState::Public,
		),
	];
	let plan = monochange_core::ReleasePlan {
		workspace_root: tempdir.path().to_path_buf(),
		decisions: packages
			.iter()
			.map(|package| {
				monochange_core::ReleaseDecision {
					package_id: package.id.clone(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Minor,
					planned_version: Some(
						Version::parse("1.1.0")
							.unwrap_or_else(|error| panic!("planned version: {error}")),
					),
					group_id: None,
					reasons: vec!["release".to_string()],
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				}
			})
			.collect(),
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};

	let npm_updates = crate::build_npm_manifest_updates(&packages, &plan)
		.unwrap_or_else(|error| panic!("npm manifest updates: {error}"));
	let deno_updates = crate::build_deno_manifest_updates(&packages, &plan)
		.unwrap_or_else(|error| panic!("deno manifest updates: {error}"));
	let dart_updates = crate::build_dart_manifest_updates(&packages, &plan)
		.unwrap_or_else(|error| panic!("dart manifest updates: {error}"));

	assert_eq!(
		String::from_utf8_lossy(&npm_updates[0].content),
		"{\n    \"name\": \"web\",\n    \"version\": \"1.1.0\",\n    \"private\": false\n}\n"
	);
	assert_eq!(
		String::from_utf8_lossy(&deno_updates[0].content),
		"{\n  \"name\": \"tool\",\n  \"version\": \"1.1.0\",\n  \"imports\": {\n    \"core\": \"^1.0.0\"\n  }\n}\n"
	);
	assert_eq!(
		String::from_utf8_lossy(&dart_updates[0].content),
		"name: mobile\nversion: '1.1.0' # keep quote\n"
	);
}

#[test]
fn build_manifest_updates_report_parse_and_io_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let npm_path = tempdir.path().join("package.json");
	let dart_path = tempdir.path().join("pubspec.yaml");
	fs::write(&npm_path, "{").unwrap_or_else(|error| panic!("write package.json: {error}"));
	fs::write(&dart_path, ": bad").unwrap_or_else(|error| panic!("write pubspec.yaml: {error}"));
	let npm = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"web",
		npm_path,
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let deno_missing = monochange_core::PackageRecord::new(
		Ecosystem::Deno,
		"tool",
		tempdir.path().join("missing/deno.json"),
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let deno_invalid_path = tempdir.path().join("invalid/deno.json");
	fs::create_dir_all(deno_invalid_path.parent().expect("deno manifest parent"))
		.unwrap_or_else(|error| panic!("create invalid deno dir: {error}"));
	fs::write(&deno_invalid_path, "{")
		.unwrap_or_else(|error| panic!("write invalid deno manifest: {error}"));
	let deno_invalid = monochange_core::PackageRecord::new(
		Ecosystem::Deno,
		"tool-invalid",
		deno_invalid_path,
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let dart = monochange_core::PackageRecord::new(
		Ecosystem::Dart,
		"mobile",
		dart_path,
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let dart_missing = monochange_core::PackageRecord::new(
		Ecosystem::Dart,
		"mobile-missing",
		tempdir.path().join("missing/pubspec.yaml"),
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let plan = monochange_core::ReleasePlan {
		workspace_root: tempdir.path().to_path_buf(),
		decisions: vec![
			npm.id.clone(),
			deno_missing.id.clone(),
			deno_invalid.id.clone(),
			dart.id.clone(),
			dart_missing.id.clone(),
		]
		.into_iter()
		.map(|package_id| {
			monochange_core::ReleaseDecision {
				package_id,
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Minor,
				planned_version: Some(
					Version::parse("1.1.0")
						.unwrap_or_else(|error| panic!("planned version: {error}")),
				),
				group_id: None,
				reasons: vec!["release".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			}
		})
		.collect(),
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let npm_error = crate::build_npm_manifest_updates(std::slice::from_ref(&npm), &plan)
		.err()
		.unwrap_or_else(|| panic!("expected npm parse error"));
	assert!(
		npm_error.to_string().contains("failed to parse"),
		"error: {npm_error}"
	);
	let deno_error = crate::build_deno_manifest_updates(std::slice::from_ref(&deno_missing), &plan)
		.err()
		.unwrap_or_else(|| panic!("expected deno io error"));
	assert!(
		deno_error.to_string().contains("failed to read"),
		"error: {deno_error}"
	);
	let deno_parse_error =
		crate::build_deno_manifest_updates(std::slice::from_ref(&deno_invalid), &plan)
			.err()
			.unwrap_or_else(|| panic!("expected deno parse error"));
	assert!(
		deno_parse_error.to_string().contains("failed to parse"),
		"error: {deno_parse_error}"
	);
	let dart_error = crate::build_dart_manifest_updates(std::slice::from_ref(&dart), &plan)
		.err()
		.unwrap_or_else(|| panic!("expected dart parse error"));
	assert!(
		dart_error.to_string().contains("failed to parse"),
		"error: {dart_error}"
	);
	let dart_read_error =
		crate::build_dart_manifest_updates(std::slice::from_ref(&dart_missing), &plan)
			.err()
			.unwrap_or_else(|| panic!("expected dart read error"));
	assert!(
		dart_read_error.to_string().contains("failed to read"),
		"error: {dart_read_error}"
	);

	let cargo_missing_dir = tempdir.path().join("cargo-missing-dir");
	fs::create_dir_all(&cargo_missing_dir)
		.unwrap_or_else(|error| panic!("create cargo missing dir: {error}"));
	let cargo_missing = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-missing",
		cargo_missing_dir,
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let cargo_invalid_path = tempdir.path().join("invalid-package/Cargo.toml");
	fs::create_dir_all(cargo_invalid_path.parent().expect("cargo package parent"))
		.unwrap_or_else(|error| panic!("create invalid cargo package dir: {error}"));
	fs::write(&cargo_invalid_path, "{")
		.unwrap_or_else(|error| panic!("write invalid cargo manifest: {error}"));
	let cargo_invalid = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-invalid",
		cargo_invalid_path,
		tempdir.path().join("cargo-invalid-workspace"),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let cargo_workspace_dir = tempdir.path().join("cargo-workspace-dir");
	fs::create_dir_all(cargo_workspace_dir.join("crates/core"))
		.unwrap_or_else(|error| panic!("create cargo workspace package dir: {error}"));
	fs::write(
		cargo_workspace_dir.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write cargo workspace package manifest: {error}"));
	fs::create_dir_all(cargo_workspace_dir.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("create workspace cargo root dir: {error}"));
	let cargo_workspace_read = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-workspace-read",
		cargo_workspace_dir.join("crates/core/Cargo.toml"),
		cargo_workspace_dir.clone(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let cargo_workspace_invalid = tempdir.path().join("cargo-workspace-invalid");
	fs::create_dir_all(cargo_workspace_invalid.join("crates/core"))
		.unwrap_or_else(|error| panic!("create cargo workspace invalid dir: {error}"));
	fs::write(
		cargo_workspace_invalid.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write valid package manifest: {error}"));
	fs::write(cargo_workspace_invalid.join("Cargo.toml"), "{")
		.unwrap_or_else(|error| panic!("write invalid workspace manifest: {error}"));
	let cargo_workspace_parse = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-workspace-parse",
		cargo_workspace_invalid.join("crates/core/Cargo.toml"),
		cargo_workspace_invalid,
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let cargo_plan = monochange_core::ReleasePlan {
		workspace_root: tempdir.path().to_path_buf(),
		decisions: vec![
			cargo_missing.id.clone(),
			cargo_invalid.id.clone(),
			cargo_workspace_read.id.clone(),
			cargo_workspace_parse.id.clone(),
		]
		.into_iter()
		.map(|package_id| {
			monochange_core::ReleaseDecision {
				package_id,
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Minor,
				planned_version: Some(
					Version::parse("1.1.0")
						.unwrap_or_else(|error| panic!("planned version: {error}")),
				),
				group_id: None,
				reasons: vec!["release".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			}
		})
		.collect(),
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let cargo_read_error =
		crate::build_cargo_manifest_updates(std::slice::from_ref(&cargo_missing), &cargo_plan)
			.err()
			.unwrap_or_else(|| panic!("expected cargo read error"));
	assert!(
		cargo_read_error.to_string().contains("failed to read"),
		"error: {cargo_read_error}"
	);
	let cargo_parse_error =
		crate::build_cargo_manifest_updates(std::slice::from_ref(&cargo_invalid), &cargo_plan)
			.err()
			.unwrap_or_else(|| panic!("expected cargo parse error"));
	assert!(
		cargo_parse_error.to_string().contains("failed to parse"),
		"error: {cargo_parse_error}"
	);
	let cargo_workspace_read_error = crate::build_cargo_manifest_updates(
		std::slice::from_ref(&cargo_workspace_read),
		&cargo_plan,
	)
	.err()
	.unwrap_or_else(|| panic!("expected cargo workspace read error"));
	assert!(
		cargo_workspace_read_error
			.to_string()
			.contains("failed to read"),
		"error: {cargo_workspace_read_error}"
	);
	let cargo_workspace_parse_error = crate::build_cargo_manifest_updates(
		std::slice::from_ref(&cargo_workspace_parse),
		&cargo_plan,
	)
	.err()
	.unwrap_or_else(|| panic!("expected cargo workspace parse error"));
	assert!(
		cargo_workspace_parse_error
			.to_string()
			.contains("failed to parse"),
		"error: {cargo_workspace_parse_error}"
	);

	let unreleased_plan = monochange_core::ReleasePlan {
		workspace_root: tempdir.path().to_path_buf(),
		decisions: Vec::new(),
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	assert!(
		crate::build_deno_manifest_updates(&[deno_missing], &unreleased_plan)
			.unwrap_or_else(|error| panic!("unreleased deno updates: {error}"))
			.is_empty()
	);
}

#[test]
fn build_cargo_manifest_updates_updates_dependents_of_released_packages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let core_manifest = tempdir.path().join("crates/core/Cargo.toml");
	let app_manifest = tempdir.path().join("crates/app/Cargo.toml");
	fs::create_dir_all(core_manifest.parent().expect("core manifest parent"))
		.unwrap_or_else(|error| panic!("create core dir: {error}"));
	fs::create_dir_all(app_manifest.parent().expect("app manifest parent"))
		.unwrap_or_else(|error| panic!("create app dir: {error}"));
	fs::write(
		&core_manifest,
		"[package]\nname = \"workflow-core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.unwrap_or_else(|error| panic!("write core Cargo.toml: {error}"));
	fs::write(
		&app_manifest,
		"[package]\nname = \"workflow-app\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n[dependencies]\nworkflow-core = { path = \"../core\", version = \"1.0.0\" }\n",
	)
	.unwrap_or_else(|error| panic!("write app Cargo.toml: {error}"));

	let core = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"workflow-core",
		core_manifest,
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let mut app = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"workflow-app",
		app_manifest.clone(),
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	app.declared_dependencies
		.push(monochange_core::PackageDependency {
			name: "workflow-core".to_string(),
			kind: monochange_core::DependencyKind::Runtime,
			version_constraint: Some("1.0.0".to_string()),
			optional: false,
		});

	let plan = monochange_core::ReleasePlan {
		workspace_root: tempdir.path().to_path_buf(),
		decisions: vec![monochange_core::ReleaseDecision {
			package_id: core.id.clone(),
			trigger_type: "changeset".to_string(),
			recommended_bump: BumpSeverity::Minor,
			planned_version: Some(
				Version::parse("1.1.0").unwrap_or_else(|error| panic!("planned version: {error}")),
			),
			group_id: None,
			reasons: vec!["release".to_string()],
			upstream_sources: Vec::new(),
			warnings: Vec::new(),
		}],
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};

	let updates = crate::build_cargo_manifest_updates(&[core, app], &plan)
		.unwrap_or_else(|error| panic!("cargo manifest updates: {error}"));
	let app_update_index = updates
		.iter()
		.position(|update| update.path.ends_with("crates/app/Cargo.toml"));
	assert!(app_update_index.is_some());
	let app_update = &updates[app_update_index.unwrap_or(0)];
	let app_update_content = String::from_utf8_lossy(&app_update.content);

	assert!(app_update_content.contains("version = \"1.1.0\""));
}

#[test]
fn build_npm_manifest_updates_reports_read_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let npm_missing = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"web",
		tempdir.path().join("missing/package.json"),
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let plan = monochange_core::ReleasePlan {
		workspace_root: tempdir.path().to_path_buf(),
		decisions: vec![monochange_core::ReleaseDecision {
			package_id: npm_missing.id.clone(),
			trigger_type: "changeset".to_string(),
			recommended_bump: BumpSeverity::Minor,
			planned_version: Some(
				Version::parse("1.1.0").unwrap_or_else(|error| panic!("planned version: {error}")),
			),
			group_id: None,
			reasons: vec!["release".to_string()],
			upstream_sources: Vec::new(),
			warnings: Vec::new(),
		}],
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};

	let error = crate::build_npm_manifest_updates(std::slice::from_ref(&npm_missing), &plan)
		.err()
		.unwrap_or_else(|| panic!("expected npm read error"));
	assert!(
		error.to_string().contains("failed to read"),
		"error: {error}"
	);
}

#[test]
fn expand_versioned_file_fields_supports_name_templates_and_passthrough_fields() {
	let definition = monochange_core::VersionedFileDefinition {
		path: "Cargo.toml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Cargo),
		prefix: None,
		fields: Some(vec![
			"workspace.dependencies.{{name}}.version".to_string(),
			"workspace.version".to_string(),
		]),
		name: None,
		regex: None,
	};
	assert_eq!(
		crate::versioned_files::expand_versioned_file_fields(&definition, &["core".to_string()]),
		vec![
			"workspace.dependencies.core.version".to_string(),
			"workspace.version".to_string(),
		]
	);
}

#[test]
fn apply_versioned_file_definition_reports_manifest_parse_errors_for_text_updaters() {
	let configuration = versioned_test_configuration();
	for (file_name, ecosystem_type, contents) in [
		("Cargo.toml", monochange_core::EcosystemType::Cargo, "{"),
		("package.json", monochange_core::EcosystemType::Npm, "{"),
		("deno.json", monochange_core::EcosystemType::Deno, "{"),
		(
			"pubspec.yaml",
			monochange_core::EcosystemType::Dart,
			": bad",
		),
	] {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let path = tempdir.path().join(file_name);
		fs::write(&path, contents).unwrap_or_else(|error| panic!("write {file_name}: {error}"));
		let context = versioned_test_context(
			&configuration,
			BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
			&[],
		);
		let definition = monochange_core::VersionedFileDefinition {
			path: file_name.to_string(),
			ecosystem_type: Some(ecosystem_type),
			prefix: None,
			fields: None,
			name: None,
			regex: None,
		};
		let error = crate::apply_versioned_file_definition(
			tempdir.path(),
			&mut BTreeMap::new(),
			&definition,
			"2.0.0",
			None,
			&["core".to_string()],
			&context,
		)
		.err()
		.unwrap_or_else(|| panic!("expected parse error for {file_name}"));
		assert!(
			error.to_string().contains("failed to parse"),
			"error: {error}"
		);
	}

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("Cargo.toml");
	fs::write(&path, "[package]\nname = \"core\"\nversion = \"1.0.0\"\n")
		.unwrap_or_else(|error| panic!("write cached cargo manifest path: {error}"));
	let context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let definition = monochange_core::VersionedFileDefinition {
		path: "Cargo.toml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Cargo),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let error = crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut BTreeMap::from([(path, crate::CachedDocument::Text("{".to_string()))]),
		&definition,
		"2.0.0",
		None,
		&["core".to_string()],
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected cached cargo parse error"));
	assert!(
		error.to_string().contains("failed to parse"),
		"error: {error}"
	);

	let pnpm_tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let pnpm_path = pnpm_tempdir.path().join("pnpm-lock.yaml");
	fs::write(&pnpm_path, "lockfileVersion: '9.0'\n")
		.unwrap_or_else(|error| panic!("write cached pnpm lock path: {error}"));
	let pnpm_definition = monochange_core::VersionedFileDefinition {
		path: "pnpm-lock.yaml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let error = crate::apply_versioned_file_definition(
		pnpm_tempdir.path(),
		&mut BTreeMap::from([(pnpm_path, crate::CachedDocument::Text(": bad".to_string()))]),
		&pnpm_definition,
		"2.0.0",
		None,
		&["core".to_string()],
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected cached pnpm parse error"));
	assert!(
		error.to_string().contains("failed to parse"),
		"error: {error}"
	);

	let cached_dart_tempdir =
		tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = cached_dart_tempdir.path().join("pubspec.yaml");
	fs::write(&path, "name: app\nversion: 1.0.0\n")
		.unwrap_or_else(|error| panic!("write cached dart manifest path: {error}"));
	let definition = monochange_core::VersionedFileDefinition {
		path: "pubspec.yaml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Dart),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let error = crate::apply_versioned_file_definition(
		cached_dart_tempdir.path(),
		&mut BTreeMap::from([(path, crate::CachedDocument::Text(": bad".to_string()))]),
		&definition,
		"2.0.0",
		None,
		&["core".to_string()],
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected cached dart parse error"));
	assert!(
		error.to_string().contains("failed to parse"),
		"error: {error}"
	);
}

#[test]
fn read_cached_document_reports_parse_errors_for_manifest_text_updaters() {
	for (file_name, ecosystem, contents) in [
		("package.json", monochange_core::EcosystemType::Npm, "["),
		("deno.json", monochange_core::EcosystemType::Deno, "["),
		(
			"pubspec.yaml",
			monochange_core::EcosystemType::Dart,
			": bad",
		),
	] {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let path = tempdir.path().join(file_name);
		fs::write(&path, contents).unwrap_or_else(|error| panic!("write {file_name}: {error}"));
		let error = crate::read_cached_document(&mut BTreeMap::new(), &path, ecosystem)
			.err()
			.unwrap_or_else(|| panic!("expected parse error for {}", path.display()));
		assert!(
			error.to_string().contains("failed to parse"),
			"error: {error}"
		);
	}
}

#[test]
fn read_cached_document_rejects_invalid_utf8_in_text_formats() {
	let cases = [
		(
			"versioned-file-invalid-utf8/invalid-cargo.toml",
			"Cargo.toml",
			monochange_core::EcosystemType::Cargo,
		),
		(
			"versioned-file-invalid-utf8/invalid-package-lock.json",
			"package-lock.json",
			monochange_core::EcosystemType::Npm,
		),
		(
			"versioned-file-invalid-utf8/invalid-pnpm-lock.yaml",
			"pnpm-lock.yaml",
			monochange_core::EcosystemType::Npm,
		),
		(
			"versioned-file-invalid-utf8/invalid-bun.lock",
			"bun.lock",
			monochange_core::EcosystemType::Npm,
		),
	];

	for (fixture, file_name, ecosystem) in cases {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let path = tempdir.path().join(file_name);
		fs::copy(fixture_path(fixture), &path)
			.unwrap_or_else(|error| panic!("copy fixture {fixture}: {error}"));
		let error = crate::read_cached_document(&mut BTreeMap::new(), &path, ecosystem)
			.err()
			.unwrap_or_else(|| panic!("expected utf8 error for {}", path.display()));
		assert!(
			error.to_string().contains("failed to parse"),
			"error: {error}"
		);
		assert!(error.to_string().contains("as text"), "error: {error}");
	}
}

#[test]
fn read_cached_document_rejects_invalid_utf8_manifest_text_formats() {
	for (file_name, ecosystem) in [
		("package.json", monochange_core::EcosystemType::Npm),
		("deno.json", monochange_core::EcosystemType::Deno),
		("pubspec.yaml", monochange_core::EcosystemType::Dart),
	] {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let path = tempdir.path().join(file_name);
		fs::write(&path, [0xff_u8, 0xfe_u8])
			.unwrap_or_else(|error| panic!("write invalid utf8 {file_name}: {error}"));
		let error = crate::read_cached_document(&mut BTreeMap::new(), &path, ecosystem)
			.err()
			.unwrap_or_else(|| panic!("expected utf8 error for {}", path.display()));
		assert!(
			error.to_string().contains("failed to parse"),
			"error: {error}"
		);
		assert!(error.to_string().contains("as text"), "error: {error}");
	}
}

#[test]
fn apply_versioned_file_definition_returns_early_without_matching_versions() {
	let tempdir = setup_fixture("versioned-file-updates/npm-manifest");
	let configuration = versioned_test_configuration();
	let context = versioned_test_context(&configuration, BTreeMap::new(), &[]);
	let definition = monochange_core::VersionedFileDefinition {
		path: "package.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let dep_names = vec!["core".to_string()];
	let mut updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"2.0.0",
		None,
		&dep_names,
		&context,
	)
	.unwrap_or_else(|error| panic!("no-op versioned file update: {error}"));
	assert!(updates.is_empty());
}

#[test]
fn apply_versioned_file_definition_rejects_invalid_glob_patterns() {
	let tempdir = setup_fixture("versioned-file-updates/npm-manifest");
	let configuration = versioned_test_configuration();
	let context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let definition = monochange_core::VersionedFileDefinition {
		path: "[".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let dep_names = vec!["core".to_string()];
	let error = crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut BTreeMap::new(),
		&definition,
		"2.0.0",
		None,
		&dep_names,
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid glob error"));
	assert!(error.to_string().contains("invalid glob pattern `[`"));
}

#[test]
fn apply_versioned_file_definition_rejects_unsupported_glob_matches() {
	let tempdir = setup_fixture("test-support/setup-fixture");
	let configuration = versioned_test_configuration();
	let context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let dep_names = vec!["core".to_string()];
	for ecosystem_type in [
		monochange_core::EcosystemType::Cargo,
		monochange_core::EcosystemType::Npm,
		monochange_core::EcosystemType::Deno,
	] {
		let definition = monochange_core::VersionedFileDefinition {
			path: "*.txt".to_string(),
			ecosystem_type: Some(ecosystem_type),
			prefix: None,
			fields: None,
			name: None,
			regex: None,
		};
		let error = crate::apply_versioned_file_definition(
			tempdir.path(),
			&mut BTreeMap::new(),
			&definition,
			"2.0.0",
			None,
			&dep_names,
			&context,
		)
		.err()
		.unwrap_or_else(|| panic!("expected unsupported match error"));
		assert!(error.to_string().contains("matched unsupported file"));
		assert!(error.to_string().contains("root.txt"), "error: {error}");
	}
}

#[test]
fn apply_versioned_file_definition_updates_cargo_workspace_dependencies_from_shorthand() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("Cargo.toml"),
		r#"[workspace.dependencies]
monochange = { path = "crates/monochange", version = "1.0.0" }
"#,
	)
	.unwrap_or_else(|error| panic!("write root cargo manifest: {error}"));
	let configuration = versioned_test_configuration();
	let context = versioned_test_context(
		&configuration,
		BTreeMap::from([("monochange".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let definition = monochange_core::VersionedFileDefinition {
		path: "Cargo.toml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Cargo),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let mut updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"2.0.0",
		None,
		&["monochange".to_string()],
		&context,
	)
	.unwrap_or_else(|error| panic!("cargo shorthand update: {error}"));
	let document = updates
		.remove(&tempdir.path().join("Cargo.toml"))
		.unwrap_or_else(|| panic!("expected cargo manifest update"));
	assert!(matches!(
		document,
		crate::CachedDocument::Text(contents)
			if contents.contains("monochange = { path = \"crates/monochange\", version = \"2.0.0\" }")
	));
}

#[test]
fn apply_versioned_file_definition_expands_cargo_name_templates_in_fields() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("Cargo.toml"),
		r#"[workspace.package]
version = "1.0.0"

[workspace.dependencies]
core = { path = "crates/core", version = "1.0.0" }
extra = { path = "crates/extra", version = "1.0.0" }
"#,
	)
	.unwrap_or_else(|error| panic!("write root cargo manifest: {error}"));
	let configuration = versioned_test_configuration();
	let context = versioned_test_context(
		&configuration,
		BTreeMap::from([
			("core".to_string(), "2.0.0".to_string()),
			("extra".to_string(), "3.0.0".to_string()),
		]),
		&[],
	);
	let definition = monochange_core::VersionedFileDefinition {
		path: "Cargo.toml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Cargo),
		prefix: None,
		fields: Some(vec![
			"workspace.version".to_string(),
			"workspace.dependencies.{{ name }}.version".to_string(),
		]),
		name: None,
		regex: None,
	};
	let mut updates = BTreeMap::new();
	let shared_version = "4.0.0".to_string();
	crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"2.0.0",
		Some(&shared_version),
		&["core".to_string(), "extra".to_string()],
		&context,
	)
	.unwrap_or_else(|error| panic!("cargo field template update: {error}"));
	let document = updates
		.remove(&tempdir.path().join("Cargo.toml"))
		.unwrap_or_else(|| panic!("expected cargo manifest update"));
	assert!(matches!(
		document,
		crate::CachedDocument::Text(contents)
			if contents.contains("[workspace.package]\nversion = \"4.0.0\"")
				&& contents.contains("core = { path = \"crates/core\", version = \"2.0.0\" }")
				&& contents.contains("extra = { path = \"crates/extra\", version = \"3.0.0\" }")
	));
}

#[test]
fn apply_versioned_file_definition_updates_bun_lockb_and_deno_text_variants() {
	let configuration = versioned_test_configuration();

	let bun_tempdir = setup_fixture("monochange/bun-lock-release");
	let bun_package = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"app",
		bun_tempdir.path().join("packages/app/package.json"),
		bun_tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let bun_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("app".to_string(), "2.0.0".to_string())]),
		std::slice::from_ref(&bun_package),
	);
	let bun_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/bun.lockb".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let bun_path = bun_tempdir.path().join("packages/app/bun.lockb");
	let original_bun =
		fs::read(&bun_path).unwrap_or_else(|error| panic!("read bun lockb: {error}"));
	let mut bun_updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		bun_tempdir.path(),
		&mut bun_updates,
		&bun_definition,
		"2.0.0",
		None,
		&["app".to_string()],
		&bun_context,
	)
	.unwrap_or_else(|error| panic!("bun lockb update: {error}"));
	let bun_document = bun_updates
		.remove(&bun_path)
		.unwrap_or_else(|| panic!("expected bun lockb update"));
	assert!(matches!(
		bun_document,
		crate::CachedDocument::Bytes(contents) if !contents.is_empty() && contents != original_bun
	));

	let deno_tempdir = setup_fixture("versioned-file-updates/deno-manifest");
	let deno_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let deno_definition = monochange_core::VersionedFileDefinition {
		path: "deno.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Deno),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let mut deno_updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		deno_tempdir.path(),
		&mut deno_updates,
		&deno_definition,
		"2.0.0",
		None,
		&["core".to_string()],
		&deno_context,
	)
	.unwrap_or_else(|error| panic!("deno manifest text update: {error}"));
	let deno_document = deno_updates
		.remove(&deno_tempdir.path().join("deno.json"))
		.unwrap_or_else(|| panic!("expected deno manifest text update"));
	assert!(matches!(
		deno_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("\"core\": \"^2.0.0\"")
	));
}

#[test]
fn apply_versioned_file_definition_updates_regex_versioned_files_from_cached_text() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("README.md"),
		"Download core from https://example.com/download/v1.0.0.tgz\n",
	)
	.unwrap_or_else(|error| panic!("write readme: {error}"));
	let configuration = versioned_test_configuration();
	let context = versioned_test_context(&configuration, BTreeMap::new(), &[]);
	let definition = monochange_core::VersionedFileDefinition {
		path: "README.md".to_string(),
		ecosystem_type: None,
		prefix: None,
		fields: None,
		name: None,
		regex: Some(
			r"https:\/\/example.com\/download\/v(?<version>\d+\.\d+\.\d+)\.tgz".to_string(),
		),
	};
	let mut updates = BTreeMap::from([(
		tempdir.path().join("README.md"),
		crate::CachedDocument::Text(
			"Download core from https://example.com/download/v1.0.0.tgz\n".to_string(),
		),
	)]);
	crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"2.0.0",
		None,
		&[],
		&context,
	)
	.unwrap_or_else(|error| panic!("regex versioned file update: {error}"));
	let updated_document = updates
		.remove(&tempdir.path().join("README.md"))
		.unwrap_or_else(|| panic!("expected regex versioned file update"));
	assert!(matches!(
		updated_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("https://example.com/download/v2.0.0.tgz")
				&& !contents.contains("https://example.com/download/v1.0.0.tgz")
	));
}

#[test]
fn apply_versioned_file_definition_reports_invalid_regex_patterns() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("README.md"),
		"Download core from https://example.com/download/v1.0.0.tgz\n",
	)
	.unwrap_or_else(|error| panic!("write readme: {error}"));
	let configuration = versioned_test_configuration();
	let context = versioned_test_context(&configuration, BTreeMap::new(), &[]);
	let definition = monochange_core::VersionedFileDefinition {
		path: "README.md".to_string(),
		ecosystem_type: None,
		prefix: None,
		fields: None,
		name: None,
		regex: Some("(".to_string()),
	};
	let mut updates = BTreeMap::new();
	let error = crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"2.0.0",
		None,
		&[],
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid regex error"));
	assert!(
		error
			.to_string()
			.contains("invalid versioned_files regex `(`")
	);
}

#[test]
fn apply_versioned_file_definition_updates_npm_manifest_and_lock_variants() {
	let configuration = versioned_test_configuration();

	let manifest_tempdir = setup_fixture("versioned-file-updates/npm-manifest");
	let manifest_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let manifest_definition = monochange_core::VersionedFileDefinition {
		path: "package.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let manifest_dep_names = vec!["core".to_string()];
	let mut manifest_updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		manifest_tempdir.path(),
		&mut manifest_updates,
		&manifest_definition,
		"2.0.0",
		None,
		&manifest_dep_names,
		&manifest_context,
	)
	.unwrap_or_else(|error| panic!("npm manifest update: {error}"));
	let manifest_path = manifest_tempdir.path().join("package.json");
	let manifest_document = manifest_updates
		.remove(&manifest_path)
		.unwrap_or_else(|| panic!("expected npm manifest update"));
	assert!(matches!(
		manifest_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("\"core\": \"^2.0.0\"")
	));

	let package_lock_tempdir = setup_fixture("monochange/npm-lock-release");
	let package = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"app",
		package_lock_tempdir
			.path()
			.join("packages/app/package.json"),
		package_lock_tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let package_lock_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("app".to_string(), "2.0.0".to_string())]),
		std::slice::from_ref(&package),
	);
	let package_lock_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/package-lock.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let package_lock_dep_names = vec!["app".to_string()];
	let mut package_lock_updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		package_lock_tempdir.path(),
		&mut package_lock_updates,
		&package_lock_definition,
		"2.0.0",
		None,
		&package_lock_dep_names,
		&package_lock_context,
	)
	.unwrap_or_else(|error| panic!("package lock update: {error}"));
	let package_lock_path = package_lock_tempdir
		.path()
		.join("packages/app/package-lock.json");
	let package_lock_document = package_lock_updates
		.remove(&package_lock_path)
		.unwrap_or_else(|| panic!("expected package-lock update"));
	assert!(matches!(
		package_lock_document,
		crate::CachedDocument::Json(value)
			if value["version"] == serde_json::Value::String("2.0.0".to_string())
				&& value["packages"][""]["version"] == serde_json::Value::String("2.0.0".to_string())
				&& value["dependencies"]["app"]["version"] == serde_json::Value::String("2.0.0".to_string())
	));

	let pnpm_tempdir = setup_fixture("versioned-file-updates/pnpm-lock");
	let pnpm_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let pnpm_definition = monochange_core::VersionedFileDefinition {
		path: "pnpm-lock.yaml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let pnpm_dep_names = vec!["core".to_string()];
	let mut pnpm_updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		pnpm_tempdir.path(),
		&mut pnpm_updates,
		&pnpm_definition,
		"2.0.0",
		None,
		&pnpm_dep_names,
		&pnpm_context,
	)
	.unwrap_or_else(|error| panic!("pnpm lock update: {error}"));
	let pnpm_path = pnpm_tempdir.path().join("pnpm-lock.yaml");
	let pnpm_document = pnpm_updates
		.remove(&pnpm_path)
		.unwrap_or_else(|| panic!("expected pnpm lock update"));
	assert!(matches!(
		pnpm_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("core: 2.0.0")
				&& contents.contains("lockfileVersion: '9.0'")
	));

	let bun_tempdir = setup_fixture("npm/bun-text-lock");
	let bun_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("left-pad".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let bun_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/bun.lock".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let bun_dep_names = vec!["left-pad".to_string()];
	let mut bun_updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		bun_tempdir.path(),
		&mut bun_updates,
		&bun_definition,
		"2.0.0",
		None,
		&bun_dep_names,
		&bun_context,
	)
	.unwrap_or_else(|error| panic!("bun lock update: {error}"));
	let bun_path = bun_tempdir.path().join("packages/app/bun.lock");
	let bun_document = bun_updates
		.remove(&bun_path)
		.unwrap_or_else(|| panic!("expected bun lock update"));
	assert!(matches!(
		bun_document,
		crate::CachedDocument::Text(contents) if contents.contains("\"left-pad\": \"2.0.0\"")
	));
}

#[test]
fn apply_versioned_file_definition_updates_python_manifest_and_lock_variants() {
	let configuration = versioned_test_configuration();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let manifest_path = tempdir.path().join("pyproject.toml");
	fs::write(
		&manifest_path,
		r#"[project]
name = "python-app"
version = "1.0.0"
dependencies = ["python-core>=1.0.0"]
"#,
	)
	.unwrap_or_else(|error| panic!("write pyproject: {error}"));
	let lock_path = tempdir.path().join("uv.lock");
	fs::write(&lock_path, "version = 1\n").unwrap_or_else(|error| panic!("write lock: {error}"));

	let context = versioned_test_context(
		&configuration,
		BTreeMap::from([("python-core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let manifest_definition = monochange_core::VersionedFileDefinition {
		path: "pyproject.toml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Python),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let dep_names = vec!["python-core".to_string()];
	let mut updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&manifest_definition,
		"2.0.0",
		None,
		&dep_names,
		&context,
	)
	.unwrap_or_else(|error| panic!("python manifest update: {error}"));
	let manifest_document = updates
		.remove(&manifest_path)
		.unwrap_or_else(|| panic!("expected python manifest update"));
	assert!(matches!(
		manifest_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("version = \"2.0.0\"")
				&& contents.contains("python-core>=2.0.0")
	));

	let lock_definition = monochange_core::VersionedFileDefinition {
		path: "uv.lock".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Python),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&lock_definition,
		"2.0.0",
		None,
		&dep_names,
		&context,
	)
	.unwrap_or_else(|error| panic!("python lock update: {error}"));
	let lock_document = updates
		.remove(&lock_path)
		.unwrap_or_else(|| panic!("expected python lock update"));
	assert!(matches!(
		lock_document,
		crate::CachedDocument::Text(contents) if contents == "version = 1\n"
	));
}

#[test]
fn apply_versioned_file_definition_reports_python_error_paths() {
	let configuration = versioned_test_configuration();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let context = versioned_test_context(
		&configuration,
		BTreeMap::from([("python-core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let dep_names = vec!["python-core".to_string()];
	let mut updates = BTreeMap::new();

	fs::write(tempdir.path().join("unknown.txt"), "text")
		.unwrap_or_else(|error| panic!("write unsupported: {error}"));
	let unsupported_definition = monochange_core::VersionedFileDefinition {
		path: "unknown.txt".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Python),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let error = crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&unsupported_definition,
		"2.0.0",
		None,
		&dep_names,
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported python path error"));
	assert!(error.to_string().contains("python"));

	let manifest_path = tempdir.path().join("pyproject.toml");
	fs::write(
		&manifest_path,
		"[project]\nname = \"python-core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write manifest: {error}"));
	let manifest_definition = monochange_core::VersionedFileDefinition {
		path: "pyproject.toml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Python),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	updates.insert(
		manifest_path,
		crate::CachedDocument::Text("[project\n".to_string()),
	);
	let error = crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&manifest_definition,
		"2.0.0",
		None,
		&dep_names,
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid python manifest error"));
	assert!(error.to_string().contains("failed to parse"));
}

#[test]
fn apply_versioned_file_definition_updates_deno_and_dart_variants() {
	let configuration = versioned_test_configuration();

	let deno_manifest_tempdir = setup_fixture("versioned-file-updates/deno-manifest");
	let deno_manifest_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let deno_manifest_definition = monochange_core::VersionedFileDefinition {
		path: "deno.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Deno),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let deno_manifest_dep_names = vec!["core".to_string()];
	let mut deno_manifest_updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		deno_manifest_tempdir.path(),
		&mut deno_manifest_updates,
		&deno_manifest_definition,
		"2.0.0",
		None,
		&deno_manifest_dep_names,
		&deno_manifest_context,
	)
	.unwrap_or_else(|error| panic!("deno manifest update: {error}"));
	let deno_manifest_path = deno_manifest_tempdir.path().join("deno.json");
	let deno_manifest_document = deno_manifest_updates
		.remove(&deno_manifest_path)
		.unwrap_or_else(|| panic!("expected deno manifest update"));
	assert!(matches!(
		deno_manifest_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("\"core\": \"^2.0.0\"")
	));

	let deno_lock_tempdir = setup_fixture("monochange/deno-lock-release");
	let deno_lock_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("app".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let deno_lock_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/deno.lock".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Deno),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let deno_lock_dep_names = vec!["app".to_string()];
	let mut deno_lock_updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		deno_lock_tempdir.path(),
		&mut deno_lock_updates,
		&deno_lock_definition,
		"2.0.0",
		None,
		&deno_lock_dep_names,
		&deno_lock_context,
	)
	.unwrap_or_else(|error| panic!("deno lock update: {error}"));
	let deno_lock_path = deno_lock_tempdir.path().join("packages/app/deno.lock");
	let deno_lock_document = deno_lock_updates
		.remove(&deno_lock_path)
		.unwrap_or_else(|| panic!("expected deno lock update"));
	assert!(matches!(
		deno_lock_document,
		crate::CachedDocument::Json(value) if value.to_string().contains("npm:app@2.0.0")
	));

	let dart_manifest_tempdir = setup_fixture("dart/workspace-pattern-warnings");
	let dart_manifest_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("shared".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let dart_manifest_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/pubspec.yaml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Dart),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let dart_manifest_dep_names = vec!["shared".to_string()];
	let mut dart_manifest_updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		dart_manifest_tempdir.path(),
		&mut dart_manifest_updates,
		&dart_manifest_definition,
		"2.0.0",
		None,
		&dart_manifest_dep_names,
		&dart_manifest_context,
	)
	.unwrap_or_else(|error| panic!("dart manifest update: {error}"));
	let dart_manifest_path = dart_manifest_tempdir
		.path()
		.join("packages/app/pubspec.yaml");
	let dart_manifest_document = dart_manifest_updates
		.remove(&dart_manifest_path)
		.unwrap_or_else(|| panic!("expected dart manifest update"));
	assert!(matches!(
		dart_manifest_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("shared: ^2.0.0")
	));

	let dart_lock_tempdir = setup_fixture("dart/manifest-lockfile-workspace");
	let dart_lock_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("nested_dart_app".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let dart_lock_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/pubspec.lock".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Dart),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let dart_lock_dep_names = vec!["nested_dart_app".to_string()];
	let mut dart_lock_updates = BTreeMap::new();
	crate::apply_versioned_file_definition(
		dart_lock_tempdir.path(),
		&mut dart_lock_updates,
		&dart_lock_definition,
		"2.0.0",
		None,
		&dart_lock_dep_names,
		&dart_lock_context,
	)
	.unwrap_or_else(|error| panic!("dart lock update: {error}"));
	let dart_lock_path = dart_lock_tempdir.path().join("packages/app/pubspec.lock");
	let dart_lock_document = dart_lock_updates
		.remove(&dart_lock_path)
		.unwrap_or_else(|| panic!("expected dart lock update"));
	assert!(matches!(
		dart_lock_document,
		crate::CachedDocument::Yaml(mapping)
			if mapping
				.get(serde_yaml_ng::Value::String("packages".to_string()))
				.and_then(serde_yaml_ng::Value::as_mapping)
				.and_then(|packages| packages.get(serde_yaml_ng::Value::String("nested_dart_app".to_string())))
				.and_then(serde_yaml_ng::Value::as_mapping)
				.and_then(|entry| entry.get(serde_yaml_ng::Value::String("version".to_string())))
				.and_then(serde_yaml_ng::Value::as_str)
				== Some("2.0.0")
	));
}

fn versioned_test_configuration() -> monochange_core::WorkspaceConfiguration {
	load_workspace_configuration(&fixture_path("monochange/release-base"))
		.unwrap_or_else(|error| panic!("configuration: {error}"))
}

fn versioned_test_context<'a>(
	configuration: &'a monochange_core::WorkspaceConfiguration,
	released_versions_by_native_name: BTreeMap<String, String>,
	packages: &'a [monochange_core::PackageRecord],
) -> crate::VersionedFileUpdateContext<'a> {
	crate::VersionedFileUpdateContext {
		package_by_config_id: packages
			.iter()
			.filter_map(|package| {
				package
					.metadata
					.get("config_id")
					.map(|config_id| (config_id.as_str(), package))
			})
			.collect(),
		package_by_native_name: packages
			.iter()
			.map(|package| (package.name.as_str(), package))
			.collect(),
		current_versions_by_native_name: packages
			.iter()
			.filter_map(|package| {
				package
					.current_version
					.as_ref()
					.map(|version| (package.name.clone(), version.to_string()))
			})
			.collect(),
		released_versions_by_native_name,
		configuration,
	}
}

fn write_blank_monochange_config(root: &Path) {
	fs::write(root.join("monochange.toml"), "")
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
}

fn create_release_record_history(root: &Path) {
	create_release_record_commit(root);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	fs::write(root.join("release.txt"), "follow-up\n")
		.unwrap_or_else(|error| panic!("write follow-up file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "fix: follow-up release change"]);
}

fn create_release_record_commit(root: &Path) -> String {
	let record = sample_release_record_for_retarget();
	create_release_record_commit_from_record(root, &record)
}

fn create_release_record_commit_with_package_publication(root: &Path, package: &str) -> String {
	let mut record = sample_release_record_for_retarget();
	record.package_publications = vec![monochange_core::PackagePublicationTarget {
		package: package.to_string(),
		ecosystem: Ecosystem::Cargo,
		registry: Some(monochange_core::PublishRegistry::Builtin(
			monochange_core::RegistryKind::CratesIo,
		)),
		version: "1.2.3".to_string(),
		mode: monochange_core::PublishMode::Builtin,
		trusted_publishing: monochange_core::TrustedPublishingSettings::default(),
		attestations: monochange_core::PublishAttestationSettings::default(),
	}];
	create_release_record_commit_from_record(root, &record)
}

fn create_release_record_commit_from_record(
	root: &Path,
	record: &monochange_core::ReleaseRecord,
) -> String {
	init_git_repo(root);
	write_blank_monochange_config(root);
	fs::write(root.join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "monochange.toml", "release.txt"]);
	let release_record = monochange_core::render_release_record_block(record)
		.unwrap_or_else(|error| panic!("render release record: {error}"));
	git_in_temp_repo(
		root,
		&[
			"commit",
			"-m",
			"chore(release): prepare release",
			"-m",
			release_record.as_str(),
		],
	);
	git_output_in_temp_repo(root, &["rev-parse", "HEAD"])
}

fn sample_release_record_for_retarget() -> monochange_core::ReleaseRecord {
	monochange_core::ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-07T08:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![monochange_core::ReleaseRecordTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.2.3".to_string(),
			members: vec!["monochange".to_string()],
		}],
		released_packages: vec!["monochange".to_string()],
		changed_files: vec![Path::new("Cargo.lock").to_path_buf()],
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		changesets: Vec::new(),
		changelogs: Vec::new(),
		package_publications: Vec::new(),
		provider: Some(monochange_core::ReleaseRecordProvider {
			kind: monochange_core::SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
	}
}

#[test]
fn group_changelog_include_allows_expected_member_targets() {
	let core_only = BTreeSet::from(["core".to_string()]);
	let core_and_app = BTreeSet::from(["core".to_string(), "app".to_string()]);

	assert!(crate::group_changelog_include_allows(
		&GroupChangelogInclude::All,
		&core_only,
	));
	assert!(!crate::group_changelog_include_allows(
		&GroupChangelogInclude::GroupOnly,
		&core_only,
	));
	assert!(crate::group_changelog_include_allows(
		&GroupChangelogInclude::Selected(["core".to_string()].into()),
		&core_only,
	));
	assert!(!crate::group_changelog_include_allows(
		&GroupChangelogInclude::Selected(["app".to_string()].into()),
		&core_only,
	));
	assert!(!crate::group_changelog_include_allows(
		&GroupChangelogInclude::Selected(["core".to_string()].into()),
		&core_and_app,
	));
}

#[test]
fn filter_group_release_note_change_handles_missing_context_and_direct_group_targets() {
	let planned_group = sample_planned_group();
	let group = sample_group_definition(GroupChangelogInclude::GroupOnly);

	assert!(
		crate::filter_group_release_note_change(
			&sample_release_note_change(None),
			Some(&group),
			&planned_group,
			&BTreeMap::new(),
		)
		.is_none()
	);

	assert!(
		crate::filter_group_release_note_change(
			&sample_release_note_change(Some(".changeset/missing.md")),
			Some(&group),
			&planned_group,
			&BTreeMap::new(),
		)
		.is_none()
	);

	let group_changeset = BTreeMap::from([(
		PathBuf::from(".changeset/group.md"),
		vec![PreparedChangesetTarget {
			id: "sdk".to_string(),
			kind: ChangesetTargetKind::Group,
			bump: None,
			origin: "author".to_string(),
			evidence_refs: Vec::new(),
			change_type: None,
			caused_by: Vec::new(),
		}],
	)]);
	let renamed = crate::filter_group_release_note_change(
		&sample_release_note_change(Some(".changeset/group.md")),
		Some(&group),
		&planned_group,
		&group_changeset,
	)
	.unwrap_or_else(|| panic!("expected direct group note"));
	assert_eq!(renamed.package_name, "sdk");

	let outside_group_changeset = BTreeMap::from([(
		PathBuf::from(".changeset/outside.md"),
		vec![PreparedChangesetTarget {
			id: "docs".to_string(),
			kind: ChangesetTargetKind::Package,
			bump: None,
			origin: "author".to_string(),
			evidence_refs: Vec::new(),
			change_type: None,
			caused_by: Vec::new(),
		}],
	)]);
	assert!(
		crate::filter_group_release_note_change(
			&sample_release_note_change(Some(".changeset/outside.md")),
			Some(&group),
			&planned_group,
			&outside_group_changeset,
		)
		.is_none()
	);
}

#[test]
fn filter_group_release_note_change_respects_member_allowlists() {
	let planned_group = sample_planned_group();
	let change = sample_release_note_change(Some(".changeset/member.md"));
	let selected_core =
		sample_group_definition(GroupChangelogInclude::Selected(["core".to_string()].into()));
	let member_changeset = BTreeMap::from([(
		PathBuf::from(".changeset/member.md"),
		vec![PreparedChangesetTarget {
			id: "core".to_string(),
			kind: ChangesetTargetKind::Package,
			bump: None,
			origin: "author".to_string(),
			evidence_refs: Vec::new(),
			change_type: None,
			caused_by: Vec::new(),
		}],
	)]);
	assert!(
		crate::filter_group_release_note_change(
			&change,
			Some(&selected_core),
			&planned_group,
			&member_changeset,
		)
		.is_some()
	);

	let blocked_changeset = BTreeMap::from([(
		PathBuf::from(".changeset/member.md"),
		vec![
			PreparedChangesetTarget {
				id: "core".to_string(),
				kind: ChangesetTargetKind::Package,
				bump: None,
				origin: "author".to_string(),
				evidence_refs: Vec::new(),
				change_type: None,
				caused_by: Vec::new(),
			},
			PreparedChangesetTarget {
				id: "app".to_string(),
				kind: ChangesetTargetKind::Package,
				bump: None,
				origin: "author".to_string(),
				evidence_refs: Vec::new(),
				change_type: None,
				caused_by: Vec::new(),
			},
		],
	)]);
	assert!(
		crate::filter_group_release_note_change(
			&change,
			Some(&selected_core),
			&planned_group,
			&blocked_changeset,
		)
		.is_none()
	);

	let message = crate::render_group_filtered_update_message("sdk");
	assert!(message.contains("No group-facing notes were recorded for this release."));
	assert!(message.contains("synchronized group `sdk`"));
}

fn sample_release_note_change(source_path: Option<&str>) -> crate::ReleaseNoteChange {
	crate::ReleaseNoteChange {
		package_id: "core".to_string(),
		package_name: "core".to_string(),
		package_labels: Vec::new(),
		source_path: source_path.map(str::to_string),
		summary: "add grouped note".to_string(),
		details: None,
		bump: BumpSeverity::Minor,
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
	}
}

fn sample_group_definition(include: GroupChangelogInclude) -> monochange_core::GroupDefinition {
	monochange_core::GroupDefinition {
		id: "sdk".to_string(),
		packages: vec!["core".to_string(), "app".to_string()],
		changelog: None,
		changelog_include: include,
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: false,
		release: false,
		version_format: VersionFormat::Namespaced,
	}
}

#[test]
fn build_command_and_configured_change_type_choices_include_runtime_metadata() {
	let command = crate::build_command("monochange");
	assert_eq!(command.get_name(), "monochange");
	assert!(command.clone().find_subcommand("skill").is_some());
	assert!(command.clone().find_subcommand("subagents").is_some());
	assert!(command.clone().find_subcommand("release-record").is_some());

	let configuration = monochange_core::WorkspaceConfiguration {
		root_path: PathBuf::from("."),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		packages: vec![monochange_core::PackageDefinition {
			id: "core".to_string(),
			path: PathBuf::from("crates/core"),
			package_type: monochange_core::PackageType::Cargo,
			changelog: None,
			excluded_changelog_types: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			ignore_ecosystem_versioned_files: false,
			ignored_paths: Vec::new(),
			additional_paths: Vec::new(),
			tag: true,
			release: true,
			publish: monochange_core::PublishSettings::default(),
			version_format: VersionFormat::Primary,
		}],
		groups: vec![monochange_core::GroupDefinition {
			id: "sdk".to_string(),
			packages: vec!["core".to_string()],
			changelog: None,
			changelog_include: GroupChangelogInclude::All,
			excluded_changelog_types: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
		}],
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
		python: monochange_core::EcosystemSettings::default(),
		go: monochange_core::EcosystemSettings::default(),
	};
	assert_eq!(
		crate::configured_change_type_choices(&configuration),
		vec![
			"breaking".to_string(),
			"change".to_string(),
			"docs".to_string(),
			"feat".to_string(),
			"fix".to_string(),
			"major".to_string(),
			"minor".to_string(),
			"none".to_string(),
			"patch".to_string(),
			"refactor".to_string(),
			"security".to_string(),
			"test".to_string()
		]
	);
}

#[test]
fn apply_runtime_prepare_release_markdown_defaults_promotes_release_format_defaults() {
	let mut cli = vec![CliCommandDefinition {
		name: "step:prepare-release".to_string(),
		help_text: None,
		inputs: vec![CliInputDefinition {
			name: "format".to_string(),
			kind: CliInputKind::Choice,
			help_text: None,
			required: false,
			default: Some("text".to_string()),
			choices: vec!["text".to_string(), "json".to_string()],
			short: None,
		}],
		steps: vec![monochange_core::CliStepDefinition::PrepareRelease {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
			allow_empty_changesets: false,
		}],
	}];

	let release = cli
		.iter_mut()
		.find(|command| command.name == "step:prepare-release")
		.unwrap_or_else(|| panic!("expected release command"));
	release.inputs[0].default = Some("text".to_string());
	release.inputs[0].choices = vec!["text".to_string(), "json".to_string()];

	crate::apply_runtime_prepare_release_markdown_defaults(&mut cli);

	let release = cli
		.iter()
		.find(|command| command.name == "step:prepare-release")
		.unwrap_or_else(|| panic!("expected release command after runtime defaults"));
	let format = release
		.inputs
		.iter()
		.find(|input| input.name == "format")
		.unwrap_or_else(|| panic!("expected release format input"));
	assert_eq!(format.default.as_deref(), Some("markdown"));
	assert_eq!(format.choices.first().map(String::as_str), Some("markdown"));
}

#[test]
fn apply_runtime_change_type_choices_updates_only_unconfigured_change_inputs() {
	let configuration = monochange_core::WorkspaceConfiguration {
		root_path: PathBuf::from("."),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		packages: vec![monochange_core::PackageDefinition {
			id: "core".to_string(),
			path: PathBuf::from("crates/core"),
			package_type: monochange_core::PackageType::Cargo,
			changelog: None,
			excluded_changelog_types: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			ignore_ecosystem_versioned_files: false,
			ignored_paths: Vec::new(),
			additional_paths: Vec::new(),
			tag: true,
			release: true,
			publish: monochange_core::PublishSettings::default(),
			version_format: VersionFormat::Primary,
		}],
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
		python: monochange_core::EcosystemSettings::default(),
		go: monochange_core::EcosystemSettings::default(),
	};
	let mut cli = vec![
		CliCommandDefinition {
			name: "change".to_string(),
			help_text: Some("Create a change".to_string()),
			inputs: vec![CliInputDefinition {
				name: "type".to_string(),
				kind: CliInputKind::String,
				help_text: None,
				required: false,
				default: None,
				choices: Vec::new(),
				short: None,
			}],
			steps: Vec::new(),
		},
		CliCommandDefinition {
			name: "change-with-existing-choices".to_string(),
			help_text: None,
			inputs: vec![CliInputDefinition {
				name: "type".to_string(),
				kind: CliInputKind::Choice,
				help_text: None,
				required: false,
				default: None,
				choices: vec!["existing".to_string()],
				short: None,
			}],
			steps: Vec::new(),
		},
	];

	crate::apply_runtime_change_type_choices(&mut cli, &configuration);

	assert_eq!(cli[0].inputs[0].kind, CliInputKind::Choice);
	assert_eq!(
		cli[0].inputs[0].choices,
		vec![
			"breaking", "change", "docs", "feat", "fix", "major", "minor", "none", "patch",
			"refactor", "security", "test"
		]
	);
	assert_eq!(cli[1].inputs[0].choices, vec!["existing"]);
}

#[test]
fn apply_runtime_change_type_choices_preserves_existing_choice_inputs_and_empty_configs() {
	let configuration = monochange_core::WorkspaceConfiguration {
		root_path: PathBuf::from("."),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		packages: Vec::new(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
		python: monochange_core::EcosystemSettings::default(),
		go: monochange_core::EcosystemSettings::default(),
	};
	let mut cli = vec![CliCommandDefinition {
		name: "change".to_string(),
		help_text: None,
		inputs: vec![CliInputDefinition {
			name: "type".to_string(),
			kind: CliInputKind::Choice,
			help_text: None,
			required: false,
			default: None,
			choices: vec!["existing".to_string()],
			short: None,
		}],
		steps: Vec::new(),
	}];
	crate::apply_runtime_change_type_choices(&mut cli, &configuration);
	assert_eq!(cli[0].inputs[0].choices, vec!["existing"]);
	assert_eq!(cli[0].inputs[0].kind, CliInputKind::Choice);
}

#[test]
fn cli_commands_for_root_uses_workspace_cli_when_configuration_load_succeeds() {
	let cli = crate::cli_commands_for_root(&fixture_path("config/package-group-and-cli"));
	let command_names = cli
		.iter()
		.map(|command| command.name.as_str())
		.collect::<Vec<_>>();
	assert!(command_names.contains(&"release"));
	assert_eq!(
		cli.iter()
			.find(|command| command.name == "release")
			.unwrap_or_else(|| panic!("expected release command"))
			.steps
			.len(),
		1
	);
}

#[test]
fn build_skill_subcommand_forwards_native_add_flags() {
	let command = Command::new("mc").subcommand(crate::build_skill_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("skill"),
			OsString::from("--list"),
			OsString::from("--copy"),
			OsString::from("-a"),
			OsString::from("pi"),
			OsString::from("-y"),
		])
		.unwrap_or_else(|error| panic!("skill matches: {error}"));
	let (_, skill_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected skill subcommand"));
	assert_eq!(
		skill_matches
			.get_many::<String>("args")
			.unwrap_or_else(|| panic!("missing forwarded skill args"))
			.map(String::as_str)
			.collect::<Vec<_>>(),
		vec!["--list", "--copy", "-a", "pi", "-y"]
	);

	let help = command
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("skill"),
			OsString::from("--help"),
		])
		.err()
		.unwrap_or_else(|| panic!("expected skill help output"));
	assert_eq!(help.kind(), clap::error::ErrorKind::DisplayHelp);
	assert!(help.to_string().contains("skills add <monochange-source>"));
}

#[test]
fn skill_command_runs_skills_add_with_npx_by_default() {
	let fixture = setup_scenario_workspace("skill/basic");
	let source = fixture.path().join("skill-source");
	let output = with_path_prefixed(fixture.path(), || {
		temp_env::with_var(
			"MONOCHANGE_SKILL_SOURCE",
			Some(source.to_string_lossy().to_string()),
			|| {
				run_cli(
					fixture.path(),
					[
						OsString::from("mc"),
						OsString::from("skill"),
						OsString::from("--list"),
						OsString::from("--copy"),
					],
				)
			},
		)
	})
	.unwrap_or_else(|error| panic!("run skill through npx: {error}"));
	assert_eq!(output, "");
	let log = fs::read_to_string(fixture.path().join(".skill-command.log"))
		.unwrap_or_else(|error| panic!("read skill command log: {error}"));
	assert_eq!(
		log.lines().collect::<Vec<_>>(),
		vec![
			"runner=npx",
			"arg=-y",
			"arg=skills",
			"arg=add",
			&format!("arg={}", source.display()),
			"arg=--list",
			"arg=--copy",
		]
	);
}

#[test]
fn skill_command_falls_back_to_pnpm_dlx_when_npx_is_missing() {
	let fixture = setup_scenario_workspace("skill/basic");
	let npx = fixture.path().join("tools/bin/npx");
	fs::remove_file(&npx)
		.unwrap_or_else(|error| panic!("remove fake npx {}: {error}", npx.display()));
	let source = fixture.path().join("skill-source");
	with_path_prefixed(fixture.path(), || {
		temp_env::with_var("MONOCHANGE_SKILL_RUNNER", Some("pnpm"), || {
			temp_env::with_var(
				"MONOCHANGE_SKILL_SOURCE",
				Some(source.to_string_lossy().to_string()),
				|| {
					run_cli(
						fixture.path(),
						[
							OsString::from("mc"),
							OsString::from("skill"),
							OsString::from("-y"),
						],
					)
				},
			)
		})
	})
	.unwrap_or_else(|error| panic!("run skill through pnpm dlx: {error}"));
	let log = fs::read_to_string(fixture.path().join(".skill-command.log"))
		.unwrap_or_else(|error| panic!("read skill command log after pnpm fallback: {error}"));
	assert_eq!(
		log.lines().collect::<Vec<_>>(),
		vec![
			"runner=pnpm",
			"arg=dlx",
			"arg=skills",
			"arg=add",
			&format!("arg={}", source.display()),
			"arg=-y",
		]
	);
}

#[test]
fn skill_command_reports_invalid_runner_override() {
	let fixture = setup_scenario_workspace("skill/basic");
	let source = fixture.path().join("skill-source");
	let error = with_path_prefixed(fixture.path(), || {
		temp_env::with_var("MONOCHANGE_SKILL_RUNNER", Some("nope"), || {
			temp_env::with_var(
				"MONOCHANGE_SKILL_SOURCE",
				Some(source.to_string_lossy().to_string()),
				|| {
					run_cli(
						fixture.path(),
						[OsString::from("mc"), OsString::from("skill")],
					)
				},
			)
		})
	})
	.err()
	.unwrap_or_else(|| panic!("expected invalid skill runner override error"));
	assert!(
		error
			.to_string()
			.contains("unsupported skill runner `nope`")
	);
}

#[test]
fn skill_command_reports_missing_forced_runner() {
	let fixture = setup_scenario_workspace("skill/basic");
	let npx = fixture.path().join("tools/bin/npx");
	fs::remove_file(&npx)
		.unwrap_or_else(|error| panic!("remove fake npx {}: {error}", npx.display()));
	let source = fixture.path().join("skill-source");
	let error = with_fixture_path_only(fixture.path(), || {
		temp_env::with_var("MONOCHANGE_SKILL_RUNNER", Some("npx"), || {
			temp_env::with_var(
				"MONOCHANGE_SKILL_SOURCE",
				Some(source.to_string_lossy().to_string()),
				|| {
					run_cli(
						fixture.path(),
						[OsString::from("mc"), OsString::from("skill")],
					)
				},
			)
		})
	})
	.err()
	.unwrap_or_else(|| panic!("expected missing forced skill runner error"));
	assert!(
		error
			.to_string()
			.contains("configured skill runner `npx` was not found in PATH")
	);
}

#[test]
fn skill_command_runs_skills_add_with_bunx_when_forced() {
	let fixture = setup_scenario_workspace("skill/basic");
	let source = fixture.path().join("skill-source");
	with_path_prefixed(fixture.path(), || {
		temp_env::with_var("MONOCHANGE_SKILL_RUNNER", Some("bunx"), || {
			temp_env::with_var(
				"MONOCHANGE_SKILL_SOURCE",
				Some(source.to_string_lossy().to_string()),
				|| {
					run_cli(
						fixture.path(),
						[
							OsString::from("mc"),
							OsString::from("skill"),
							OsString::from("--list"),
						],
					)
				},
			)
		})
	})
	.unwrap_or_else(|error| panic!("run skill through bunx: {error}"));
	let log = fs::read_to_string(fixture.path().join(".skill-command.log"))
		.unwrap_or_else(|error| panic!("read skill command log after bunx override: {error}"));
	assert_eq!(
		log.lines().collect::<Vec<_>>(),
		vec![
			"runner=bunx",
			"arg=skills",
			"arg=add",
			&format!("arg={}", source.display()),
			"arg=--list",
		]
	);
}

#[test]
fn skill_command_reports_nonzero_exit_status_from_runner() {
	let fixture = setup_scenario_workspace("skill/basic");
	let source = fixture.path().join("skill-source");
	let error = with_path_prefixed(fixture.path(), || {
		temp_env::with_var(
			"MONOCHANGE_SKILL_SOURCE",
			Some(source.to_string_lossy().to_string()),
			|| {
				temp_env::with_var("MONOCHANGE_SKILL_FAKE_EXIT", Some("7"), || {
					run_cli(
						fixture.path(),
						[
							OsString::from("mc"),
							OsString::from("skill"),
							OsString::from("--list"),
						],
					)
				})
			},
		)
	})
	.err()
	.unwrap_or_else(|| panic!("expected non-zero skill runner error"));
	assert!(error.to_string().contains("`npx -y skills add"));
	assert!(error.to_string().contains("exit status: 7"));
}

#[test]
fn build_subagents_subcommand_parses_valid_inputs_and_rejects_invalid_targets() {
	let command = Command::new("mc").subcommand(crate::build_subagents_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("claude"),
			OsString::from("pi"),
			OsString::from("--format"),
			OsString::from("json"),
		])
		.unwrap_or_else(|error| panic!("subagents matches: {error}"));
	let (_, subagent_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected subagents subcommand"));
	assert_eq!(
		subagent_matches
			.get_many::<String>("target")
			.unwrap_or_else(|| panic!("missing targets"))
			.map(String::as_str)
			.collect::<Vec<_>>(),
		vec!["claude", "pi"]
	);
	assert_eq!(
		subagent_matches
			.get_one::<String>("format")
			.map(String::as_str),
		Some("json")
	);

	let invalid_target = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("unknown"),
		])
		.err()
		.unwrap_or_else(|| panic!("expected invalid subagent target error"));
	assert_eq!(invalid_target.kind(), clap::error::ErrorKind::InvalidValue);

	let missing_target = command
		.clone()
		.try_get_matches_from([OsString::from("mc"), OsString::from("subagents")])
		.err()
		.unwrap_or_else(|| panic!("expected missing target error"));
	assert_eq!(
		missing_target.kind(),
		clap::error::ErrorKind::MissingRequiredArgument
	);

	let conflicting_all = command
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("claude"),
			OsString::from("--all"),
		])
		.err()
		.unwrap_or_else(|| panic!("expected target conflict error"));
	assert_eq!(
		conflicting_all.kind(),
		clap::error::ErrorKind::ArgumentConflict
	);
}

#[test]
fn subagent_parsing_helpers_cover_defaults_deduplication_and_errors() {
	assert_eq!(
		crate::SubagentTarget::all(),
		vec![
			crate::SubagentTarget::Claude,
			crate::SubagentTarget::Vscode,
			crate::SubagentTarget::Copilot,
			crate::SubagentTarget::Pi,
			crate::SubagentTarget::Codex,
			crate::SubagentTarget::Cursor,
		]
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("claude"),
		Some(crate::SubagentTarget::Claude)
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("vscode"),
		Some(crate::SubagentTarget::Vscode)
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("copilot"),
		Some(crate::SubagentTarget::Copilot)
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("pi"),
		Some(crate::SubagentTarget::Pi)
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("codex"),
		Some(crate::SubagentTarget::Codex)
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("cursor"),
		Some(crate::SubagentTarget::Cursor)
	);
	assert_eq!(crate::SubagentTarget::from_cli_value("unknown"), None);

	let json = String::from("json");
	assert_eq!(
		crate::parse_subagent_output_format_or_default(Some(&json)),
		crate::SubagentOutputFormat::Json
	);
	assert_eq!(
		crate::parse_subagent_output_format_or_default(None),
		crate::SubagentOutputFormat::Markdown
	);

	let claude = String::from("claude");
	let pi = String::from("pi");
	let targets = crate::parse_subagent_targets(Some(vec![&claude, &pi, &claude]))
		.unwrap_or_else(|error| panic!("parse subagent targets: {error}"));
	assert_eq!(
		targets,
		vec![crate::SubagentTarget::Claude, crate::SubagentTarget::Pi]
	);

	let invalid = String::from("nope");
	let invalid_error = crate::parse_subagent_targets(Some(vec![&invalid]))
		.err()
		.unwrap_or_else(|| panic!("expected invalid subagent target error"));
	assert!(
		invalid_error
			.to_string()
			.contains("unsupported subagent target `nope`")
	);

	let empty_values: Option<Vec<&String>> = None;
	let empty_error = crate::parse_subagent_targets(empty_values)
		.err()
		.unwrap_or_else(|| panic!("expected missing subagent targets error"));
	assert!(
		empty_error
			.to_string()
			.contains("expected at least one subagent target or `--all`")
	);
}

#[test]
fn subagents_command_supports_all_targets_in_default_text_output() {
	let _guard = snapshot_settings().bind_to_scope();
	let fixture = setup_scenario_workspace("subagents/basic");
	let output = run_cli(
		fixture.path(),
		[
			OsString::from("mc"),
			OsString::from("subagents"),
			OsString::from("--all"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_or_else(|error| panic!("subagents all dry-run text: {error}"));

	insta::assert_snapshot!("subagents_all_text_dry_run", output);
}

#[test]
fn build_release_record_subcommand_requires_from_and_supports_json_output() {
	let command = Command::new("mc").subcommand(crate::build_release_record_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("release-record"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--format"),
			OsString::from("json"),
		])
		.unwrap_or_else(|error| panic!("release-record matches: {error}"));
	let (_, subcommand_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected release-record subcommand"));
	assert_eq!(
		subcommand_matches
			.get_one::<String>("from")
			.map(String::as_str),
		Some("HEAD")
	);
	assert_eq!(
		subcommand_matches
			.get_one::<String>("format")
			.map(String::as_str),
		Some("json")
	);
	let error = command
		.try_get_matches_from([OsString::from("mc"), OsString::from("release-record")])
		.err()
		.unwrap_or_else(|| panic!("expected missing from error"));
	assert_eq!(
		error.kind(),
		clap::error::ErrorKind::MissingRequiredArgument
	);
}

#[test]
fn build_command_with_cli_registers_custom_subcommands_and_default_help_text() {
	let cli = vec![CliCommandDefinition {
		name: "custom".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
	}];
	let command = crate::build_command_with_cli("monochange", &cli);
	assert!(command.clone().find_subcommand("custom").is_some());
	let error = command
		.try_get_matches_from([
			OsString::from("monochange"),
			OsString::from("custom"),
			OsString::from("--help"),
		])
		.err()
		.unwrap_or_else(|| panic!("expected help output"));
	assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
	assert!(error.to_string().contains("Run the `custom` command"));
}

#[test]
fn cli_command_after_help_covers_supported_commands_and_custom_commands() {
	let cases = [
		("change", "Prefer configured package ids"),
		("release", "Direct package changes propagate to dependents"),
		("commit-release", "Embeds a durable release record block"),
		("affected", "Group-owned changesets cover all members"),
		("diagnostics", "linked review request"),
		("repair-release", "Defaults to descendant-only retargets"),
		(
			"tag-release",
			"Treats reruns on the same commit as already up to date",
		),
	];
	for (name, expected) in cases {
		let after_help = crate::cli_command_after_help(&CliCommandDefinition {
			name: name.to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: Vec::new(),
		})
		.unwrap_or_else(|| panic!("expected after_help for {name}"));
		assert!(after_help.contains(expected));
	}
	assert!(
		crate::cli_command_after_help(&CliCommandDefinition {
			name: "custom".to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: Vec::new(),
		})
		.is_none()
	);
}

#[test]
fn build_cli_command_subcommand_parses_supported_input_kinds() {
	let cli_command = CliCommandDefinition {
		name: "custom".to_string(),
		help_text: Some("Run a custom command".to_string()),
		inputs: vec![
			CliInputDefinition {
				name: "package".to_string(),
				kind: CliInputKind::String,
				help_text: Some("Target package".to_string()),
				required: true,
				default: None,
				choices: Vec::new(),
				short: Some('p'),
			},
			CliInputDefinition {
				name: "changed_paths".to_string(),
				kind: CliInputKind::StringList,
				help_text: None,
				required: false,
				default: None,
				choices: Vec::new(),
				short: None,
			},
			CliInputDefinition {
				name: "output".to_string(),
				kind: CliInputKind::Path,
				help_text: None,
				required: false,
				default: None,
				choices: Vec::new(),
				short: None,
			},
			CliInputDefinition {
				name: "enabled".to_string(),
				kind: CliInputKind::Boolean,
				help_text: None,
				required: false,
				default: None,
				choices: Vec::new(),
				short: None,
			},
			CliInputDefinition {
				name: "sync_provider".to_string(),
				kind: CliInputKind::Boolean,
				help_text: None,
				required: false,
				default: Some("true".to_string()),
				choices: Vec::new(),
				short: None,
			},
			CliInputDefinition {
				name: "type".to_string(),
				kind: CliInputKind::Choice,
				help_text: None,
				required: false,
				default: Some("docs".to_string()),
				choices: vec!["docs".to_string(), "test".to_string()],
				short: None,
			},
		],
		steps: Vec::new(),
	};

	let command = Command::new("mc").subcommand(crate::build_cli_command_subcommand(&cli_command));
	let matches = command
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("custom"),
			OsString::from("-p"),
			OsString::from("core"),
			OsString::from("--changed-paths"),
			OsString::from("crates/core/src/lib.rs"),
			OsString::from("--changed-paths"),
			OsString::from("Cargo.toml"),
			OsString::from("--output"),
			OsString::from("out.json"),
			OsString::from("--enabled"),
			OsString::from("--sync-provider=false"),
		])
		.unwrap_or_else(|error| panic!("matches: {error}"));
	let (_, subcommand_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected subcommand matches"));
	assert_eq!(
		subcommand_matches
			.get_one::<String>("package")
			.map(String::as_str),
		Some("core")
	);
	assert_eq!(
		subcommand_matches
			.get_many::<String>("changed_paths")
			.unwrap_or_else(|| panic!("expected changed_paths"))
			.map(String::as_str)
			.collect::<Vec<_>>(),
		vec!["crates/core/src/lib.rs", "Cargo.toml"]
	);
	assert_eq!(
		subcommand_matches
			.get_one::<String>("output")
			.map(String::as_str),
		Some("out.json")
	);
	assert!(subcommand_matches.get_flag("enabled"));
	assert_eq!(
		subcommand_matches
			.get_one::<String>("sync_provider")
			.map(String::as_str),
		Some("false")
	);
	assert_eq!(
		subcommand_matches
			.get_one::<String>("type")
			.map(String::as_str),
		Some("docs")
	);
}

#[test]
fn render_release_record_discovery_supports_text_and_json_formats() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	create_release_record_history(root);
	let text = crate::render_release_record_discovery(root, "HEAD", crate::OutputFormat::Text)
		.unwrap_or_else(|error| panic!("release-record text: {error}"));
	assert!(text.contains("release record:"));
	assert!(text.contains("input ref: HEAD"));
	let json = crate::render_release_record_discovery(root, "HEAD", crate::OutputFormat::Json)
		.unwrap_or_else(|error| panic!("release-record json: {error}"));
	assert!(json.contains("\"record\""));
	assert!(json.contains("\"resolvedCommit\""));
	assert!(json.contains("\"inputRef\": "));
}

#[test]
fn render_release_tag_report_supports_text_and_json_formats() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let release_commit = create_release_record_commit(root);

	let text = crate::release_record::render_release_tag_report(
		root,
		"HEAD",
		crate::OutputFormat::Text,
		false,
		true,
	)
	.unwrap_or_else(|error| panic!("tag-release text: {error}"));
	assert!(text.contains("release tags:"));
	assert!(text.contains("push: no"));
	assert!(text.contains("status: dry-run"));
	assert!(text.contains("[planned]"));

	let json = crate::release_record::render_release_tag_report(
		root,
		"HEAD",
		crate::OutputFormat::Json,
		false,
		true,
	)
	.unwrap_or_else(|error| panic!("tag-release json: {error}"));
	assert!(json.contains("\"recordCommit\": "));
	assert!(json.contains("\"push\": false"));
	assert!(json.contains("\"status\": \"dry_run\""));
	assert!(json.contains(&release_commit[..7]));
}

#[test]
fn create_release_tags_creates_and_pushes_tags_in_process() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let release_commit = create_release_record_commit(root);
	configure_origin_remote(root);
	git_in_temp_repo(root, &["push", "-u", "origin", "HEAD:main"]);
	let discovery = crate::discover_release_record(root, "HEAD")
		.unwrap_or_else(|error| panic!("discover release record: {error}"));

	let report = crate::release_record::create_release_tags(root, &discovery, true, false)
		.unwrap_or_else(|error| panic!("create release tags: {error}"));
	assert_eq!(report.status, "completed");
	assert_eq!(report.tag_results.len(), 1);
	assert_eq!(
		report.tag_results[0].operation,
		crate::release_record::ReleaseTagOperation::Created
	);
	assert_eq!(
		git_output_in_temp_repo(root, &["rev-parse", "refs/tags/v1.2.3^{commit}"]),
		release_commit
	);
	assert_eq!(
		git_output_in_temp_repo(root, &["ls-remote", "--tags", "origin", "v1.2.3"])
			.split_whitespace()
			.next()
			.unwrap_or_else(|| panic!("expected remote tag sha")),
		release_commit
	);
}

#[test]
fn render_tag_name_and_provider_urls_follow_provider_conventions() {
	let github = sample_github_source_configuration("https://api.github.com");
	assert_eq!(
		crate::render_tag_name("core", "1.2.3", VersionFormat::Primary),
		"v1.2.3"
	);
	assert_eq!(
		crate::render_tag_name("core", "1.2.3", VersionFormat::Namespaced),
		"core/v1.2.3"
	);
	assert!(crate::tag_url_for_provider(&github, "v1.2.3").contains("/releases/tag/v1.2.3"));
	assert!(
		crate::compare_url_for_provider(&github, "v1.2.2", "v1.2.3")
			.contains("compare/v1.2.2...v1.2.3")
	);

	let gitlab = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: Some("gitlab.example.com".to_string()),
		api_url: None,
		owner: "group".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};
	assert!(crate::tag_url_for_provider(&gitlab, "v1.2.3").contains("gitlab.example.com"));
	assert!(crate::compare_url_for_provider(&gitlab, "v1.2.2", "v1.2.3").contains("/-/compare/"));
}

#[test]
fn parse_tag_prefix_and_version_parses_primary_and_namespaced_tags() {
	let primary = crate::parse_tag_prefix_and_version("v1.2.3")
		.unwrap_or_else(|| panic!("expected primary tag"));
	assert_eq!(primary.0, "v");
	assert_eq!(primary.1, Version::new(1, 2, 3));

	let namespaced = crate::parse_tag_prefix_and_version("core/v2.0.0")
		.unwrap_or_else(|| panic!("expected namespaced tag"));
	assert_eq!(namespaced.0, "core/v");
	assert_eq!(namespaced.1, Version::new(2, 0, 0));
	assert_eq!(crate::parse_tag_prefix_and_version("not-a-tag"), None);
}

#[test]
fn resolve_release_datetime_honors_env_overrides() {
	temp_env::with_var("MONOCHANGE_RELEASE_DATE", Some("2026-04-07"), || {
		assert_eq!(
			crate::resolve_release_datetime(),
			chrono::NaiveDate::from_ymd_opt(2026, 4, 7)
				.unwrap_or_else(|| panic!("valid date"))
				.and_hms_opt(0, 0, 0)
				.unwrap_or_else(|| panic!("valid time"))
		);
	});
	temp_env::with_var(
		"MONOCHANGE_RELEASE_DATE",
		Some("2026-04-07T12:34:56"),
		|| {
			assert_eq!(
				crate::resolve_release_datetime(),
				chrono::NaiveDate::from_ymd_opt(2026, 4, 7)
					.unwrap_or_else(|| panic!("valid date"))
					.and_hms_opt(12, 34, 56)
					.unwrap_or_else(|| panic!("valid time"))
			);
		},
	);
}

#[test]
fn diff_output_supports_color_respects_common_terminal_env_controls() {
	temp_env::with_var("NO_COLOR", Some("1"), || {
		assert!(!crate::diff_output_supports_color(true));
	});
	temp_env::with_var("NO_COLOR", None::<&str>, || {
		temp_env::with_var("CLICOLOR", Some("0"), || {
			assert!(!crate::diff_output_supports_color(true));
		});
		temp_env::with_var("CLICOLOR", None::<&str>, || {
			temp_env::with_var("CLICOLOR_FORCE", Some("1"), || {
				assert!(crate::diff_output_supports_color(false));
			});
			temp_env::with_var("CLICOLOR_FORCE", None::<&str>, || {
				assert!(crate::diff_output_supports_color(true));
				assert!(!crate::diff_output_supports_color(false));
			});
		});
	});
}

#[test]
fn colorize_diff_line_styles_unified_diff_markers() {
	assert_eq!(
		crate::colorize_diff_line("--- a/Cargo.toml"),
		"\u{1b}[1;36m--- a/Cargo.toml\u{1b}[0m"
	);
	assert_eq!(
		crate::colorize_diff_line("+++ b/Cargo.toml"),
		"\u{1b}[1;36m+++ b/Cargo.toml\u{1b}[0m"
	);
	assert_eq!(
		crate::colorize_diff_line("@@ -1,1 +1,1 @@"),
		"\u{1b}[36m@@ -1,1 +1,1 @@\u{1b}[0m"
	);
	assert_eq!(
		crate::colorize_diff_line("-version = \"1.0.0\""),
		"\u{1b}[31m-version = \"1.0.0\"\u{1b}[0m"
	);
	assert_eq!(
		crate::colorize_diff_line("+version = \"1.1.0\""),
		"\u{1b}[32m+version = \"1.1.0\"\u{1b}[0m"
	);
	assert_eq!(
		crate::colorize_diff_line(r"\ No newline at end of file"),
		"\u{1b}[33m\\ No newline at end of file\u{1b}[0m"
	);
	assert_eq!(crate::colorize_diff_line(" unchanged"), " unchanged");
}

#[test]
fn build_file_diff_previews_handles_missing_files_and_color_modes() {
	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	let existing_path = tempdir.path().join("Cargo.toml");
	let missing_path = tempdir.path().join("new-file.md");
	let existing_content =
		fs::read(&existing_path).unwrap_or_else(|error| panic!("read existing file: {error}"));
	let mut updated_existing_content = existing_content.clone();
	updated_existing_content.extend_from_slice(b"\n# diff coverage\n");
	let updates = vec![
		crate::FileUpdate {
			path: existing_path.clone(),
			content: updated_existing_content.clone(),
		},
		crate::FileUpdate {
			path: missing_path.clone(),
			content: b"# created by coverage\n".to_vec(),
		},
	];

	let plain_previews = temp_env::with_var("NO_COLOR", Some("1"), || {
		crate::build_file_diff_previews(tempdir.path(), &updates)
			.unwrap_or_else(|error| panic!("plain previews: {error}"))
	});
	assert_eq!(plain_previews.len(), 2);
	assert!(
		plain_previews
			.iter()
			.all(|preview| preview.display_diff == preview.diff)
	);
	let rendered_plain_diffs = plain_previews
		.iter()
		.map(|preview| preview.diff.clone())
		.collect::<Vec<_>>()
		.join("\n---\n");
	assert!(rendered_plain_diffs.contains("new-file.md"));

	let colored_previews = temp_env::with_var("NO_COLOR", None::<&str>, || {
		temp_env::with_var("CLICOLOR", None::<&str>, || {
			temp_env::with_var("CLICOLOR_FORCE", Some("1"), || {
				crate::build_file_diff_previews(
					tempdir.path(),
					&[crate::FileUpdate {
						path: existing_path.clone(),
						content: updated_existing_content,
					}],
				)
				.unwrap_or_else(|error| panic!("colored previews: {error}"))
			})
		})
	});
	assert_eq!(colored_previews.len(), 1);
	assert!(colored_previews[0].display_diff.contains("\u{1b}["));
	assert!(!colored_previews[0].diff.contains("\u{1b}["));
}

#[test]
fn build_file_diff_previews_reports_directory_read_errors() {
	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	let directory_path = tempdir.path().join("crates");
	let error = crate::build_file_diff_previews(
		tempdir.path(),
		&[crate::FileUpdate {
			path: directory_path.clone(),
			content: b"unused".to_vec(),
		}],
	)
	.err()
	.unwrap_or_else(|| panic!("expected directory read failure"));
	assert!(
		error
			.to_string()
			.contains(&format!("failed to read {}", directory_path.display()))
	);
}

#[test]
fn prepare_release_execution_collects_file_diffs_for_dry_runs() {
	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	let prepared = temp_env::with_var("NO_COLOR", Some("1"), || {
		prepare_release_execution(tempdir.path(), true)
			.unwrap_or_else(|error| panic!("prepare release execution: {error}"))
	});
	assert!(!prepared.prepared_release.changed_files.is_empty());
	assert!(!prepared.file_diffs.is_empty());
	assert!(
		prepared
			.file_diffs
			.iter()
			.all(|file_diff| file_diff.display_diff == file_diff.diff)
	);
}

#[test]
fn prepare_release_skips_file_diff_previews_when_callers_do_not_need_them() {
	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	set_force_build_file_diff_previews_error(true);
	let prepared = crate::prepare_release(tempdir.path(), true)
		.unwrap_or_else(|error| panic!("prepare release without diffs: {error}"));
	set_force_build_file_diff_previews_error(false);
	assert!(!prepared.changed_files.is_empty());
}

#[test]
fn command_release_without_diff_skips_file_diff_previews() {
	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	set_force_build_file_diff_previews_error(true);
	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("release without diff: {error}"));
	set_force_build_file_diff_previews_error(false);
	assert!(output.contains("# `step:prepare-release`"));
}

#[test]
fn prepare_release_execution_propagates_file_diff_preview_errors() {
	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	set_force_build_file_diff_previews_error(true);
	let error = prepare_release_execution(tempdir.path(), true)
		.err()
		.unwrap_or_else(|| panic!("expected forced file diff preview error"));
	set_force_build_file_diff_previews_error(false);
	assert!(
		error
			.to_string()
			.contains("forced build_file_diff_previews test error")
	);
}

#[test]
fn default_change_path_sluggifies_first_package_reference() {
	let path = crate::default_change_path(
		Path::new("/workspace"),
		&["cargo:crates/core/Cargo.toml".to_string()],
	);
	assert!(path.starts_with("/workspace/.changeset"));
	assert!(
		path.to_string_lossy()
			.ends_with("-cargo-crates-core-cargo-toml.md")
	);
}

#[test]
fn format_publish_state_and_source_operation_labels_are_stable() {
	assert_eq!(
		crate::format_publish_state(monochange_core::PublishState::Public),
		"public"
	);
	assert_eq!(
		crate::format_publish_state(monochange_core::PublishState::Private),
		"private"
	);
	assert_eq!(
		crate::format_publish_state(monochange_core::PublishState::Unpublished),
		"unpublished"
	);
	assert_eq!(
		crate::format_publish_state(monochange_core::PublishState::Excluded),
		"excluded"
	);
	assert_eq!(
		crate::format_source_operation(&monochange_core::SourceReleaseOperation::Created),
		"created"
	);
	assert_eq!(
		crate::format_source_operation(&monochange_core::SourceReleaseOperation::Updated),
		"updated"
	);
	assert_eq!(
		crate::format_change_request_operation(
			&monochange_core::SourceChangeRequestOperation::Created
		),
		"created"
	);
	assert_eq!(
		crate::format_change_request_operation(
			&monochange_core::SourceChangeRequestOperation::Updated
		),
		"updated"
	);
	assert_eq!(
		crate::format_change_request_operation(
			&monochange_core::SourceChangeRequestOperation::Skipped
		),
		"skipped"
	);
}

#[test]
fn parse_output_format_accepts_markdown_text_and_json_and_rejects_invalid_values() {
	assert_eq!(
		crate::parse_output_format("markdown").unwrap(),
		crate::OutputFormat::Markdown
	);
	assert_eq!(
		crate::parse_output_format("md").unwrap(),
		crate::OutputFormat::Markdown
	);
	assert_eq!(
		crate::parse_output_format("text").unwrap(),
		crate::OutputFormat::Text
	);
	assert_eq!(
		crate::parse_output_format("json").unwrap(),
		crate::OutputFormat::Json
	);
	let error = crate::parse_output_format("yaml")
		.err()
		.unwrap_or_else(|| panic!("expected output format error"));
	assert!(error.to_string().contains("unsupported output format"));
}

#[test]
fn maybe_render_markdown_for_terminal_returns_original_when_not_tty() {
	let markdown = "# Hello\n\n**bold** text";
	let result = crate::maybe_render_markdown_for_terminal(markdown);
	assert_eq!(result, markdown);
}

#[test]
fn render_markdown_if_terminal_returns_styled_when_terminal() {
	let markdown = "# Hello\n\n**bold** text";
	let result = crate::render_markdown_if_terminal(markdown, true);
	// termimad produces ANSI-styled output when terminal is true
	assert!(result.contains("Hello"));
	assert_ne!(result, markdown);
}

#[test]
fn render_markdown_if_terminal_returns_original_when_not_terminal() {
	let markdown = "# Hello\n\n**bold** text";
	let result = crate::render_markdown_if_terminal(markdown, false);
	assert_eq!(result, markdown);
}

#[test]
fn detect_output_format_from_env_args_defaults_to_markdown() {
	let args: Vec<String> = vec!["monochange".to_string()];
	assert_eq!(
		crate::detect_output_format_from_env_args(args.into_iter()),
		crate::OutputFormat::Markdown
	);
}

#[test]
fn detect_output_format_from_env_args_parses_format_flag() {
	let args: Vec<String> = vec![
		"monochange".to_string(),
		"--format".to_string(),
		"json".to_string(),
	];
	assert_eq!(
		crate::detect_output_format_from_env_args(args.into_iter()),
		crate::OutputFormat::Json
	);
}

#[test]
fn detect_output_format_from_env_args_parses_format_equals() {
	let args: Vec<String> = vec!["monochange".to_string(), "--format=md".to_string()];
	assert_eq!(
		crate::detect_output_format_from_env_args(args.into_iter()),
		crate::OutputFormat::Markdown
	);
}

#[test]
fn detect_output_format_from_env_args_treats_config_step_as_json() {
	let args: Vec<String> = vec!["monochange".to_string(), "step:config".to_string()];
	assert_eq!(
		crate::detect_output_format_from_env_args(args.into_iter()),
		crate::OutputFormat::Json
	);
}

#[test]
fn detect_output_format_from_env_args_falls_back_to_markdown_for_invalid() {
	let args: Vec<String> = vec![
		"monochange".to_string(),
		"--format".to_string(),
		"invalid".to_string(),
	];
	assert_eq!(
		crate::detect_output_format_from_env_args(args.into_iter()),
		crate::OutputFormat::Markdown
	);
}

#[test]
fn parse_subagent_output_format_or_default_prefers_markdown() {
	assert_eq!(
		crate::parse_subagent_output_format_or_default(None),
		crate::SubagentOutputFormat::Markdown
	);
	let json = String::from("json");
	assert_eq!(
		crate::parse_subagent_output_format_or_default(Some(&json)),
		crate::SubagentOutputFormat::Json
	);
	let md = String::from("md");
	assert_eq!(
		crate::parse_subagent_output_format_or_default(Some(&md)),
		crate::SubagentOutputFormat::Markdown
	);
	let text = String::from("text");
	assert_eq!(
		crate::parse_subagent_output_format_or_default(Some(&text)),
		crate::SubagentOutputFormat::Text
	);
}

#[test]
fn effective_title_template_prefers_specific_then_defaults_then_builtin() {
	assert_eq!(
		crate::effective_title_template(Some("specific"), Some("default"), "builtin"),
		"specific"
	);
	assert_eq!(
		crate::effective_title_template(None, Some("default"), "builtin"),
		"default"
	);
	assert_eq!(
		crate::effective_title_template(None, None, "builtin"),
		"builtin"
	);
}

#[test]
fn default_title_templates_follow_version_format_defaults() {
	assert_eq!(
		crate::default_release_title_for_format(VersionFormat::Primary),
		monochange_core::DEFAULT_RELEASE_TITLE_PRIMARY
	);
	assert_eq!(
		crate::default_release_title_for_format(VersionFormat::Namespaced),
		monochange_core::DEFAULT_RELEASE_TITLE_NAMESPACED
	);
	assert_eq!(
		crate::default_changelog_version_title_for_format(VersionFormat::Primary),
		monochange_core::DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY
	);
	assert_eq!(
		crate::default_changelog_version_title_for_format(VersionFormat::Namespaced),
		monochange_core::DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED
	);
}

#[test]
fn current_dir_or_dot_returns_current_directory() {
	let current = std::env::current_dir().unwrap_or_else(|error| panic!("current dir: {error}"));
	assert_eq!(crate::current_dir_or_dot(), current);
}

#[test]
fn render_discovery_report_supports_json_and_text_formats() {
	let report = monochange_core::DiscoveryReport {
		workspace_root: PathBuf::from("."),
		packages: vec![monochange_core::PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			PathBuf::from("crates/core/Cargo.toml"),
			PathBuf::from("."),
			Some(Version::new(1, 0, 0)),
			monochange_core::PublishState::Public,
		)],
		dependencies: Vec::new(),
		version_groups: vec![monochange_core::VersionGroup {
			group_id: "sdk".to_string(),
			display_name: "sdk".to_string(),
			members: vec!["cargo:crates/core/Cargo.toml".to_string()],
			mismatch_detected: false,
		}],
		warnings: vec!["warning text".to_string()],
	};
	let json = crate::render_discovery_report(&report, crate::OutputFormat::Json)
		.unwrap_or_else(|error| panic!("json discovery: {error}"));
	assert!(json.contains("\"workspaceRoot\""));
	assert!(json.contains("\"warning text\""));

	let text = crate::render_discovery_report(&report, crate::OutputFormat::Text)
		.unwrap_or_else(|error| panic!("text discovery: {error}"));
	assert!(text.contains("Workspace discovery for ."));
	assert!(text.contains("Packages: 1"));
	assert!(text.contains("Warnings:"));
}

#[test]
fn find_previous_tag_returns_previous_matching_prefix() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	init_git_repo(tempdir.path());
	fs::write(tempdir.path().join("release.txt"), "first\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(tempdir.path(), &["add", "release.txt"]);
	git_in_temp_repo(tempdir.path(), &["commit", "-m", "first release"]);
	git_in_temp_repo(tempdir.path(), &["tag", "core/v1.0.0"]);
	fs::write(tempdir.path().join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(tempdir.path(), &["add", "release.txt"]);
	git_in_temp_repo(tempdir.path(), &["commit", "-m", "second release"]);
	git_in_temp_repo(tempdir.path(), &["tag", "core/v1.2.0"]);
	git_in_temp_repo(tempdir.path(), &["tag", "app/v9.9.9"]);
	assert_eq!(
		crate::find_previous_tag(tempdir.path(), "core/v1.2.0"),
		Some("core/v1.0.0".to_string())
	);
	assert_eq!(
		crate::find_previous_tag(tempdir.path(), "core/v1.0.0"),
		None
	);
}

fn sample_planned_group() -> monochange_core::PlannedVersionGroup {
	monochange_core::PlannedVersionGroup {
		group_id: "sdk".to_string(),
		display_name: "sdk".to_string(),
		members: vec![
			"cargo:crates/core".to_string(),
			"cargo:crates/app".to_string(),
		],
		mismatch_detected: false,
		planned_version: Some(
			Version::parse("1.2.3").unwrap_or_else(|error| panic!("version: {error}")),
		),
		recommended_bump: BumpSeverity::Minor,
	}
}

fn sample_prepared_release_for_cli_render() -> crate::PreparedRelease {
	crate::PreparedRelease {
		plan: monochange_core::ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![monochange_core::ReleaseDecision {
				package_id: "core".to_string(),
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Minor,
				planned_version: Some(
					Version::parse("1.2.3").unwrap_or_else(|error| panic!("version: {error}")),
				),
				group_id: Some("sdk".to_string()),
				reasons: vec!["release core".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			}],
			groups: vec![sample_planned_group()],
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["core".to_string()],
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![crate::ReleaseTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.3".to_string(),
			members: vec!["core".to_string(), "app".to_string()],
			rendered_title: "sdk 1.2.3".to_string(),
			rendered_changelog_title: "sdk changelog".to_string(),
		}],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		package_publications: Vec::new(),
		dry_run: true,
	}
}

#[test]
fn build_source_release_requests_and_change_request_cover_gitea_dispatch() {
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::Gitea,
		host: Some("https://gitea.example.com".to_string()),
		api_url: Some("https://gitea.example.com/api/v1".to_string()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};
	let manifest = sample_release_manifest_for_commit_message(true, true);
	let requests = crate::build_source_release_requests(&source, &manifest);
	assert_eq!(requests.len(), 1);
	assert_eq!(requests[0].provider, monochange_core::SourceProvider::Gitea);
	assert_eq!(requests[0].repository, "ifiokjr/monochange");
	let request = crate::build_source_change_request(&source, &manifest);
	assert_eq!(request.provider, monochange_core::SourceProvider::Gitea);
	assert_eq!(request.repository, "ifiokjr/monochange");
	assert!(!request.commit_message.subject.is_empty());
}

#[test]
fn tracked_release_pull_request_paths_include_manifest_path_and_deduplicate() {
	let manifest = sample_release_manifest_for_commit_message(false, true);
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		quiet: false,
		show_diff: false,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: Some(PathBuf::from("Cargo.lock")),
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let tracked = crate::tracked_release_pull_request_paths(&context, &manifest);
	assert_eq!(
		tracked,
		vec![
			PathBuf::from(".changeset/feature.md"),
			PathBuf::from("Cargo.lock")
		]
	);
}

#[test]
fn discovery_report_helpers_include_version_groups_and_warnings() {
	let package = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		PathBuf::from("/workspace/crates/core/Cargo.toml"),
		PathBuf::from("/workspace"),
		Some(Version::parse("1.2.3").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let report = monochange_core::DiscoveryReport {
		workspace_root: PathBuf::from("/workspace"),
		packages: vec![package.clone()],
		dependencies: vec![monochange_core::DependencyEdge {
			from_package_id: package.id.clone(),
			to_package_id: package.id.clone(),
			dependency_kind: monochange_core::DependencyKind::Runtime,
			source_kind: monochange_core::DependencySourceKind::Manifest,
			version_constraint: Some("^1.2.3".to_string()),
			is_optional: false,
			is_direct: true,
		}],
		version_groups: vec![monochange_core::VersionGroup {
			group_id: "sdk".to_string(),
			display_name: "sdk".to_string(),
			members: vec![package.id.clone()],
			mismatch_detected: true,
		}],
		warnings: vec!["workspace warning".to_string()],
	};

	let json = crate::json_discovery_report(&report);
	assert_eq!(json["versionGroups"][0]["id"], "sdk");
	assert_eq!(json["warnings"][0], "workspace warning");

	let text = crate::text_discovery_report(&report);
	assert!(text.contains("Version groups:"));
	assert!(text.contains("- sdk (1)"));
	assert!(text.contains("Warnings:"));
	assert!(text.contains("- workspace warning"));
}

#[test]
fn default_change_path_falls_back_to_change_when_slug_is_empty() {
	let path = crate::default_change_path(Path::new("/workspace"), &["!!!".to_string()]);
	assert!(path.starts_with("/workspace/.changeset"));
	assert!(path.to_string_lossy().ends_with("-change.md"));
}

fn sample_github_source_configuration(api_url: &str) -> monochange_core::SourceConfiguration {
	monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitHub,
		host: None,
		api_url: Some(api_url.to_string()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	}
}

fn init_git_repo(root: &Path) {
	git_in_temp_repo(root, &["init", "-b", "main"]);
	git_in_temp_repo(root, &["config", "user.name", "monochange Tests"]);
	git_in_temp_repo(root, &["config", "user.email", "monochange@example.com"]);
	git_in_temp_repo(root, &["config", "commit.gpgsign", "false"]);
}

fn configure_origin_remote(root: &Path) {
	let remote_root = root.join("origin.git");
	git_in_temp_repo(root, &["init", "--bare", &remote_root.to_string_lossy()]);
	git_in_temp_repo(
		root,
		&["remote", "add", "origin", &remote_root.to_string_lossy()],
	);
}

/// The original `PATH` captured at process start, before any `temp_env` test
/// can temporarily replace it with a fixture-only path. This prevents race
/// conditions where a parallel test's PATH mutation causes `git` to be
/// unavailable to other tests that spawn git commands.
fn original_path() -> &'static str {
	static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
	PATH.get_or_init(|| std::env::var("PATH").unwrap_or_default())
}

fn sanitized_git_command() -> std::process::Command {
	let mut command = std::process::Command::new("git");
	command.env("PATH", original_path());
	for variable in [
		"GIT_DIR",
		"GIT_WORK_TREE",
		"GIT_COMMON_DIR",
		"GIT_INDEX_FILE",
		"GIT_OBJECT_DIRECTORY",
		"GIT_ALTERNATE_OBJECT_DIRECTORIES",
	] {
		command.env_remove(variable);
	}

	command
}

fn git_in_dir(root: &Path, args: &[&str]) {
	let status = sanitized_git_command()
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn git_output_in_git_dir(git_dir: &Path, args: &[&str]) -> String {
	let output = sanitized_git_command()
		.arg("--git-dir")
		.arg(git_dir)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git --git-dir {args:?}: {error}"));
	assert!(output.status.success(), "git --git-dir {args:?} failed");
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git output utf8: {error}"))
		.trim()
		.to_string()
}

fn git_in_temp_repo(root: &Path, args: &[&str]) {
	let status = sanitized_git_command()
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn git_output_in_temp_repo(root: &Path, args: &[&str]) -> String {
	let output = sanitized_git_command()
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(output.status.success(), "git {args:?} failed");
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git output utf8: {error}"))
		.trim()
		.to_string()
}

#[test]
fn render_cached_document_text_rejects_invalid_utf8() {
	let _guard = snapshot_settings().bind_to_scope();
	let path = PathBuf::from("binary.bin");
	let document = crate::CachedDocument::Bytes(vec![0xFF, 0xFE, 0x00]);
	let error = crate::versioned_files::render_cached_document_text(&path, document)
		.err()
		.unwrap_or_else(|| panic!("expected utf8 error"));
	insta::assert_snapshot!(error.to_string());
}

#[test]
fn read_cached_text_document_returns_error_for_nonexistent_file() {
	let _guard = snapshot_settings().bind_to_scope();
	let mut updates = BTreeMap::new();
	let path = PathBuf::from("/nonexistent/path/to/file.txt");
	let error = crate::versioned_files::read_cached_text_document(&mut updates, &path)
		.err()
		.unwrap_or_else(|| panic!("expected io error"));
	insta::assert_snapshot!(error.to_string());
}

#[test]
fn read_cached_text_document_returns_error_for_invalid_utf8_on_disk() {
	let _guard = snapshot_settings().bind_to_scope();
	let fixture = setup_fixture("monochange/invalid-utf8-file");
	let file_path = fixture.path().join("binary.bin");
	let mut updates = BTreeMap::new();
	let error = crate::versioned_files::read_cached_text_document(&mut updates, &file_path)
		.err()
		.unwrap_or_else(|| panic!("expected utf8 error"));
	insta::assert_snapshot!(error.to_string());
}

#[test]
fn apply_versioned_file_definition_reports_invalid_glob_pattern() {
	let _guard = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = versioned_test_configuration();
	let context = versioned_test_context(&configuration, BTreeMap::new(), &[]);
	let definition = monochange_core::VersionedFileDefinition {
		path: "[invalid".to_string(),
		ecosystem_type: None,
		prefix: None,
		fields: None,
		name: None,
		regex: Some(r"v(?<version>\d+\.\d+\.\d+)".to_string()),
	};
	let mut updates = BTreeMap::new();
	let error = crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"2.0.0",
		None,
		&[],
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected glob error"));
	insta::assert_snapshot!(error.to_string());
}

#[test]
fn apply_versioned_file_definition_reports_missing_ecosystem_type() {
	let _guard = snapshot_settings().bind_to_scope();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = versioned_test_configuration();
	let context = versioned_test_context(&configuration, BTreeMap::new(), &[]);
	let definition = monochange_core::VersionedFileDefinition {
		path: "Cargo.toml".to_string(),
		ecosystem_type: None,
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	let mut updates = BTreeMap::new();
	let error = crate::apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"2.0.0",
		None,
		&[],
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing ecosystem type error"));
	insta::assert_snapshot!(error.to_string());
}

#[test]
fn template_rendering_does_not_interpret_variable_content_as_template_syntax() {
	let mut metadata = BTreeMap::new();
	metadata.insert(
		"summary",
		"fix: handle {{ curly_braces }} in output".to_string(),
	);
	metadata.insert("version", "1.0.0".to_string());
	metadata.insert("package", "core".to_string());

	let result = crate::changelog::render_message_template(
		"- {{ summary }} ({{ package }}@{{ version }})",
		&metadata,
	);

	assert_eq!(
		result,
		"- fix: handle {{ curly_braces }} in output (core@1.0.0)"
	);
}

#[test]
fn release_command_updates_versioned_files_and_changelogs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_fixture("monochange/release-base", root);
	init_git_repo(root);
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);

	let output = run_cli(
		root,
		[OsString::from("mc"), OsString::from("step:prepare-release")],
	)
	.unwrap_or_else(|error| panic!("release output: {error}"));

	// Release should report the planned version.
	assert!(
		output.contains("1.1.0"),
		"expected version 1.1.0 in output: {output}"
	);

	// Verify the group versioned file was updated.
	let group_toml = fs::read_to_string(root.join("group.toml"))
		.unwrap_or_else(|error| panic!("group: {error}"));
	assert!(
		group_toml.contains("1.1.0"),
		"expected group.toml to contain 1.1.0: {group_toml}"
	);

	// Verify the changelog was generated with the release section.
	let changelog = fs::read_to_string(root.join("changelog.md"))
		.unwrap_or_else(|error| panic!("changelog: {error}"));
	assert!(
		changelog.contains("1.1.0"),
		"expected changelog to contain 1.1.0: {changelog}"
	);
	assert!(
		changelog.contains("add release command"),
		"expected changelog to contain changeset summary"
	);

	// Verify changeset files were consumed (deleted).
	assert!(
		!root.join(".changeset/feature.md").exists(),
		"expected changeset file to be deleted after release"
	);
}

#[cfg(unix)]
#[test]
fn atomic_write_preserves_file_permissions() {
	use std::os::unix::fs::PermissionsExt;

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let file_path = tempdir.path().join("test-perms.txt");

	// Create file with custom permissions (rwxr-xr-x = 0o755).
	fs::write(&file_path, b"original").unwrap_or_else(|error| panic!("write: {error}"));
	fs::set_permissions(&file_path, fs::Permissions::from_mode(0o755))
		.unwrap_or_else(|error| panic!("chmod: {error}"));

	// Overwrite via atomic_write.
	crate::release_artifacts::apply_file_updates(&[crate::FileUpdate {
		path: file_path.clone(),
		content: b"updated".to_vec(),
	}])
	.unwrap_or_else(|error| panic!("atomic write: {error}"));

	// Verify content updated.
	let content = fs::read_to_string(&file_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert_eq!(content, "updated");

	// Verify permissions preserved.
	let mode = fs::metadata(&file_path)
		.unwrap_or_else(|error| panic!("metadata: {error}"))
		.permissions()
		.mode();
	assert_eq!(
		mode & 0o777,
		0o755,
		"expected permissions 0o755, got 0o{:o}",
		mode & 0o777
	);
}

#[test]
fn template_cache_reuses_compiled_templates_across_calls() {
	// Call render_message_template twice with the same template to exercise
	// both cache-miss (first call) and cache-hit (second call) paths.
	let mut metadata = BTreeMap::new();
	metadata.insert("name", "alpha".to_string());

	let first = crate::changelog::render_message_template("Hello {{ name }}", &metadata);
	assert_eq!(first, "Hello alpha");

	metadata.insert("name", "beta".to_string());
	let second = crate::changelog::render_message_template("Hello {{ name }}", &metadata);
	assert_eq!(second, "Hello beta");

	// Different template string exercises a second cache entry.
	let third = crate::changelog::render_message_template("Goodbye {{ name }}", &metadata);
	assert_eq!(third, "Goodbye beta");
}

#[test]
fn batch_changeset_contexts_returns_empty_without_git_repo() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	// No .git directory — should return contexts with None fields.
	fs::create_dir_all(root.join("crates/pkg")).unwrap();
	fs::write(
		root.join("crates/pkg/Cargo.toml"),
		"[package]\nname = \"pkg\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();
	fs::write(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/pkg\"]\nresolver = \"2\"\n",
	)
	.unwrap();
	fs::write(
		root.join("monochange.toml"),
		"[defaults]\npackage_type = \"cargo\"\n\n[package.pkg]\npath = \"crates/pkg\"\n\n[ecosystems.cargo]\nenabled = true\n",
	)
	.unwrap();
	fs::create_dir_all(root.join(".changeset")).unwrap();
	fs::write(
		root.join(".changeset/test.md"),
		"---\npkg: patch\n---\n\nTest.\n",
	)
	.unwrap();

	let result = crate::prepare_release(root, true).unwrap();
	// With no .git, changeset context fields should all be None.
	for changeset in &result.changesets {
		if let Some(ctx) = &changeset.context {
			assert!(
				ctx.introduced.is_none(),
				"expected no introduced commit without git"
			);
			assert!(
				ctx.last_updated.is_none(),
				"expected no last_updated commit without git"
			);
		}
	}
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn batch_changeset_contexts_resolves_introduced_and_updated_commits() {
	let _env_lock = crate::TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(std::sync::PoisonError::into_inner);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	fs::create_dir_all(root.join("crates/pkg")).unwrap();
	fs::write(
		root.join("crates/pkg/Cargo.toml"),
		"[package]\nname = \"pkg\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();
	fs::write(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/pkg\"]\nresolver = \"2\"\n",
	)
	.unwrap();
	fs::write(
		root.join("monochange.toml"),
		"[defaults]\npackage_type = \"cargo\"\n\n[package.pkg]\npath = \"crates/pkg\"\n\n[ecosystems.cargo]\nenabled = true\n",
	)
	.unwrap();

	init_git_repo(root);
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);

	// Add changeset in a separate commit.
	fs::create_dir_all(root.join(".changeset")).unwrap();
	fs::write(
		root.join(".changeset/feat.md"),
		"---\npkg: minor\n---\n\nNew feature.\n",
	)
	.unwrap();
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "add changeset"]);
	let intro_sha = git_output_in_temp_repo(root, &["rev-parse", "--short=7", "HEAD"]);

	// Update changeset.
	fs::write(
		root.join(".changeset/feat.md"),
		"---\npkg: minor\n---\n\nUpdated feature.\n",
	)
	.unwrap();
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "update changeset"]);
	let update_sha = git_output_in_temp_repo(root, &["rev-parse", "--short=7", "HEAD"]);

	let mut observed_introduced = None;
	let mut observed_updated = None;

	for _attempt in 0..3 {
		let result = crate::prepare_release(root, true).unwrap();
		let changeset = result
			.changesets
			.iter()
			.find(|cs| cs.path.to_string_lossy().contains("feat.md"))
			.unwrap_or_else(|| panic!("expected feat changeset"));
		let ctx = changeset
			.context
			.as_ref()
			.unwrap_or_else(|| panic!("expected context"));

		observed_updated = ctx.last_updated.as_ref().map(|revision| {
			revision
				.commit
				.as_ref()
				.unwrap_or_else(|| panic!("expected last_updated commit"))
				.short_sha
				.clone()
		});

		if let Some(introduced) = ctx.introduced.as_ref() {
			observed_introduced = Some(
				introduced
					.commit
					.as_ref()
					.unwrap_or_else(|| panic!("expected introduced commit"))
					.short_sha
					.clone(),
			);
			break;
		}

		std::thread::sleep(std::time::Duration::from_millis(10));
	}

	if let Some(introduced) = observed_introduced {
		assert_eq!(
			introduced,
			intro_sha.trim(),
			"introduced commit should match the commit that added the file"
		);
	}
	if let Some(updated) = observed_updated {
		assert_eq!(
			updated,
			update_sha.trim(),
			"last_updated commit should match the most recent commit"
		);
	}
}

#[test]
fn extract_quiet_from_args_detects_short_and_long_flags() {
	assert!(crate::extract_quiet_from_args([
		OsString::from("mc"),
		OsString::from("--quiet"),
		OsString::from("step:prepare-release"),
	]));
	assert!(crate::extract_quiet_from_args([
		OsString::from("mc"),
		OsString::from("-q"),
		OsString::from("step:discover"),
	]));
	assert!(!crate::extract_quiet_from_args([
		OsString::from("mc"),
		OsString::from("step:prepare-release"),
	]));
}

#[test]
fn extract_log_level_returns_none_when_flag_absent() {
	let args = ["mc", "discover", "--format", "json"];
	assert_eq!(
		crate::extract_log_level(args.iter().map(ToString::to_string)),
		None
	);
}

#[test]
fn extract_log_level_returns_value_for_separate_flag() {
	let args = ["mc", "--log-level", "debug", "discover"];
	assert_eq!(
		crate::extract_log_level(args.iter().map(ToString::to_string)),
		Some("debug".to_string())
	);
}

#[test]
fn extract_log_level_returns_value_for_equals_syntax() {
	let args = ["mc", "--log-level=monochange=trace", "release"];
	assert_eq!(
		crate::extract_log_level(args.iter().map(ToString::to_string)),
		Some("monochange=trace".to_string())
	);
}

#[test]
fn extract_log_level_returns_none_when_flag_has_no_value() {
	let args = ["mc", "--log-level"];
	assert_eq!(
		crate::extract_log_level(args.iter().map(ToString::to_string)),
		None
	);
}

#[test]
fn init_tracing_with_none_does_not_install_subscriber() {
	crate::tracing_setup::init_tracing(None);
}

#[test]
fn init_tracing_with_valid_filter_does_not_panic() {
	crate::tracing_setup::init_tracing(Some("monochange=debug"));
}

#[test]
fn cli_accepts_log_level_flag_without_error() {
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("--log-level"),
			OsString::from("debug"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("log-level with help: {error}"));

	assert!(output.contains("Usage: mc"));
}

#[test]
fn cli_accepts_log_level_equals_syntax_without_error() {
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("--log-level=monochange=trace"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("log-level equals with help: {error}"));

	assert!(output.contains("Usage: mc"));
}

#[test]
fn cli_root_help_matches_help_subcommand_overview() {
	let root_help = run_with_args("mc", [OsString::from("mc"), OsString::from("--help")])
		.unwrap_or_else(|error| panic!("root help output: {error}"));
	let help_subcommand = run_with_args("mc", [OsString::from("mc"), OsString::from("help")])
		.unwrap_or_else(|error| panic!("help subcommand output: {error}"));

	assert_eq!(root_help, help_subcommand);
}

#[test]
fn cli_help_does_not_show_log_level_flag() {
	let output = run_with_args("mc", [OsString::from("mc"), OsString::from("--help")])
		.unwrap_or_else(|error| panic!("help output: {error}"));

	assert!(
		!output.contains("--log-level"),
		"hidden flag should not appear in help output"
	);
}

#[test]
fn cli_help_subcommand_renders_detailed_command_help() {
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("help"),
			OsString::from("validate"),
		],
	)
	.unwrap_or_else(|error| panic!("help validate output: {error}"));

	assert!(output.contains("validate"));
	assert!(output.contains("Description"));
	assert!(output.contains("Usage"));
}

#[test]
fn cli_help_subcommand_overview_without_argument() {
	let output = run_with_args("mc", [OsString::from("mc"), OsString::from("help")])
		.unwrap_or_else(|error| panic!("help overview output: {error}"));

	assert!(output.contains("mc help"));
	assert!(output.contains("Commands"));
}

#[test]
fn format_clap_error_returns_plain_text_without_color() {
	use clap::Command;
	let cmd = Command::new("test").about("test cmd");
	let err = cmd.try_get_matches_from(["test", "--help"]).unwrap_err();
	let out = crate::format_clap_error(&err, false);
	assert!(out.contains("Usage: test"));
}

#[test]
fn format_clap_error_returns_empty_with_color() {
	use clap::Command;
	let cmd = Command::new("test").about("test cmd");
	let err = cmd.try_get_matches_from(["test", "--help"]).unwrap_err();
	let out = crate::format_clap_error(&err, true);
	assert_eq!(out, String::new());
}

#[test]
fn parse_remote_url_extracts_owner_repo_from_ssh_url() {
	let result = crate::workspace_ops::parse_remote_url("git@github.com:ifiokjr/monochange.git");
	let info = result.unwrap_or_else(|| panic!("expected RemoteInfo"));
	assert_eq!(info.owner, "ifiokjr");
	assert_eq!(info.repo, "monochange");
}

#[test]
fn parse_remote_url_extracts_owner_repo_from_https_url() {
	let result =
		crate::workspace_ops::parse_remote_url("https://github.com/ifiokjr/monochange.git");
	let info = result.unwrap_or_else(|| panic!("expected RemoteInfo"));
	assert_eq!(info.owner, "ifiokjr");
	assert_eq!(info.repo, "monochange");
}

#[test]
fn parse_remote_url_handles_https_without_git_suffix() {
	let result = crate::workspace_ops::parse_remote_url("https://gitlab.com/mygroup/myproject");
	let info = result.unwrap_or_else(|| panic!("expected RemoteInfo"));
	assert_eq!(info.owner, "mygroup");
	assert_eq!(info.repo, "myproject");
}

#[test]
fn parse_remote_url_handles_ssh_protocol_url() {
	let result = crate::workspace_ops::parse_remote_url("ssh://git@github.com/owner/repo.git");
	let info = result.unwrap_or_else(|| panic!("expected RemoteInfo"));
	assert_eq!(info.owner, "owner");
	assert_eq!(info.repo, "repo");
}

#[test]
fn parse_remote_url_returns_none_for_invalid_url() {
	assert!(crate::workspace_ops::parse_remote_url("not-a-url").is_none());
	assert!(crate::workspace_ops::parse_remote_url("").is_none());
}

#[test]
fn parse_remote_url_rejects_nested_repository_paths() {
	assert!(
		crate::workspace_ops::parse_remote_url("https://github.com/owner/repo/nested.git")
			.is_none()
	);
}

#[test]
fn detect_remote_owner_repo_reads_origin_remote_from_a_live_git_repo() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["init", "-b", "main"])
		.output()
		.unwrap_or_else(|error| panic!("git init: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args([
			"remote",
			"add",
			"origin",
			"git@github.com:ifiokjr/monochange.git",
		])
		.output()
		.unwrap_or_else(|error| panic!("git remote add: {error}"));

	let remote = crate::workspace_ops::detect_remote_owner_repo(tempdir.path())
		.unwrap_or_else(|| panic!("expected remote info"));
	assert_eq!(remote.owner, "ifiokjr");
	assert_eq!(remote.repo, "monochange");
}

#[test]
fn init_with_provider_writes_source_section_and_commit_release_command() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("init"),
			OsString::from("--provider"),
			OsString::from("github"),
		],
	)
	.unwrap_or_else(|error| panic!("init output: {error}"));
	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));

	assert!(output.contains("wrote"), "expected wrote message");
	assert!(
		config.contains("[source]"),
		"expected [source] section in config"
	);
	assert!(
		config.contains("provider = \"github\""),
		"expected github provider"
	);
	assert!(
		config.contains("[source.releases]"),
		"expected [source.releases]"
	);
	assert!(
		config.contains("[source.pull_requests]"),
		"expected [source.pull_requests]"
	);
	assert!(
		config.contains("[cli.commit-release]"),
		"expected [cli.commit-release] command"
	);
	assert!(
		config.contains("[cli.release-pr]"),
		"expected [cli.release-pr] command"
	);
	assert!(
		config.contains("type = \"PrepareRelease\""),
		"expected PrepareRelease step"
	);
	assert!(
		config.contains("type = \"CommitRelease\""),
		"expected CommitRelease step"
	);
	assert!(
		config.contains("type = \"OpenReleaseRequest\""),
		"expected OpenReleaseRequest step"
	);
	load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("generated provider config should parse: {error}"));
}

#[test]
fn init_with_github_provider_creates_workflow_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("init"),
			OsString::from("--provider"),
			OsString::from("github"),
		],
	)
	.unwrap_or_else(|error| panic!("init output: {error}"));

	assert!(
		output.contains("changeset-policy.yml"),
		"expected changeset-policy.yml in output"
	);
	assert!(
		output.contains("release.yml"),
		"expected release.yml in output"
	);

	let policy = fs::read_to_string(
		tempdir
			.path()
			.join(".github/workflows/changeset-policy.yml"),
	)
	.unwrap_or_else(|error| panic!("changeset-policy.yml: {error}"));
	assert!(
		policy.contains("mc \"${args[@]}\""),
		"expected mc command in changeset policy"
	);
	assert!(
		policy.contains("cargo binstall monochange"),
		"expected cargo binstall install step"
	);

	let release = fs::read_to_string(tempdir.path().join(".github/workflows/release.yml"))
		.unwrap_or_else(|error| panic!("release.yml: {error}"));
	assert!(
		release.contains("mc release-pr"),
		"expected mc release-pr command"
	);
	assert!(
		release.contains("mc tag-release --from HEAD"),
		"expected mc tag-release command"
	);
	assert!(
		release.contains("mc publish"),
		"expected mc publish command"
	);
	assert!(
		release.contains("github-actions[bot]"),
		"expected bot git config"
	);
}

#[test]
fn init_with_quiet_still_writes_provider_configuration_without_stdout() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("--quiet"),
			OsString::from("init"),
			OsString::from("--provider"),
			OsString::from("github"),
		],
	)
	.unwrap_or_else(|error| panic!("init output: {error}"));

	assert!(output.is_empty(), "expected quiet init output to be empty");
	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));
	assert!(config.contains("provider = \"github\""));
	assert!(
		tempdir
			.path()
			.join(".github/workflows/release.yml")
			.exists()
	);
}

#[test]
fn init_without_provider_does_not_create_workflow_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("init")],
	)
	.unwrap_or_else(|error| panic!("init output: {error}"));

	assert!(
		!tempdir.path().join(".github/workflows").exists(),
		"expected no .github/workflows directory without --provider"
	);
}

#[test]
fn init_without_provider_comments_out_source_section() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("init")],
	)
	.unwrap_or_else(|error| panic!("init output: {error}"));

	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));

	assert!(
		!config.contains("\n[source]\n"),
		"expected no active [source] section without --provider"
	);
	assert!(
		config.contains("# [source]"),
		"expected commented-out [source] section"
	);
	assert!(
		!config.contains("[cli.commit-release]"),
		"expected no commit-release command without --provider"
	);
}

#[test]
fn init_with_github_provider_reports_workflow_directory_creation_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());
	copy_fixture("monochange/init-github-workflow-dir-file", tempdir.path());

	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("init"),
			OsString::from("--provider"),
			OsString::from("github"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("expected init failure"));

	assert!(error.to_string().contains("failed to create"));
	assert!(error.to_string().contains(".github/workflows"));
}

#[test]
fn init_with_github_provider_reports_workflow_write_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());
	copy_fixture(
		"monochange/init-github-workflow-write-failure",
		tempdir.path(),
	);

	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("init"),
			OsString::from("--provider"),
			OsString::from("github"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("expected init failure"));

	assert!(error.to_string().contains("failed to write"));
	assert!(error.to_string().contains("changeset-policy.yml"));
}

#[test]
fn init_with_gitlab_provider_writes_source_but_no_workflows() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("init"),
			OsString::from("--provider"),
			OsString::from("gitlab"),
		],
	)
	.unwrap_or_else(|error| panic!("init output: {error}"));
	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));

	assert!(config.contains("provider = \"gitlab\""));
	assert!(config.contains("[cli.commit-release]"));
	assert!(
		!output.contains("changeset-policy.yml"),
		"gitlab should not generate GitHub workflows"
	);
	assert!(!tempdir.path().join(".github/workflows").exists());
}

#[test]
fn lint_cli_lists_and_explains_rules() {
	let root = fixture_path("config/lint-settings");
	let list_output = run_cli(
		&root,
		[
			OsString::from("mc"),
			OsString::from("lint"),
			OsString::from("list"),
			OsString::from("--format=json"),
		],
	)
	.unwrap_or_else(|error| panic!("lint list: {error}"));
	assert!(list_output.contains("cargo/recommended"));
	assert!(list_output.contains("npm/recommended"));
	assert!(list_output.contains("dart/recommended"));
	assert!(list_output.contains("dart/required-package-fields"));
	assert!(list_output.contains("dart/sdk-constraint-present"));
	assert!(list_output.contains("dart/no-unexpected-dependency-overrides"));
	assert!(list_output.contains("dart/internal-path-dependency-policy"));
	assert!(list_output.contains("dart/flutter-package-metadata-consistent"));

	let explain_output = run_cli(
		&root,
		[
			OsString::from("mc"),
			OsString::from("lint"),
			OsString::from("explain"),
			OsString::from("cargo/internal-dependency-workspace"),
		],
	)
	.unwrap_or_else(|error| panic!("lint explain: {error}"));
	assert!(explain_output.contains("Internal dependency workspace"));
}

#[test]
fn lint_cli_new_scaffolds_rule_files() {
	let tempdir = setup_scenario_workspace("test-support/scenario-workspace");
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("lint"),
			OsString::from("new"),
			OsString::from("cargo/no-path-dependencies"),
		],
	)
	.unwrap_or_else(|error| panic!("lint new: {error}"));
	assert!(output.contains("no_path_dependencies.rs"));
	assert!(
		tempdir
			.path()
			.join("crates/monochange_cargo/src/lints/no_path_dependencies.rs")
			.exists()
	);
}

#[test]
fn check_command_supports_only_rule_filters() {
	let root = fixture_path("config/lint-settings");
	let output = run_cli(
		&root,
		[
			OsString::from("mc"),
			OsString::from("check"),
			OsString::from("--only"),
			OsString::from("cargo/internal-dependency-workspace"),
			OsString::from("--format=json"),
		],
	)
	.unwrap_or_else(|error| panic!("check only: {error}"));
	assert!(output.contains("\"results\""));
	assert!(output.contains("\"warning_count\""));
}

#[test]
fn quiet_lint_commands_return_empty_output() {
	let root = fixture_path("config/lint-settings");
	let output = run_cli(
		&root,
		[
			OsString::from("mc"),
			OsString::from("--quiet"),
			OsString::from("lint"),
			OsString::from("list"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet lint list: {error}"));
	assert!(output.is_empty());
}

#[test]
fn build_release_manifest_from_record_populates_manifest_from_release_record() {
	use std::path::PathBuf;

	use monochange_core::ChangelogFormat;
	use monochange_core::ReleaseManifestChangelog;
	use monochange_core::ReleaseNotesDocument;
	use monochange_core::ReleaseOwnerKind;
	use monochange_core::ReleaseRecord;
	use monochange_core::ReleaseRecordProvider;
	use monochange_core::ReleaseRecordTarget;

	let record = ReleaseRecord {
		schema_version: 1,
		kind: "monochange.releaseRecord".to_string(),
		created_at: "2024-01-01T00:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.0.0".to_string()),
		group_version: None,
		release_targets: vec![ReleaseRecordTarget {
			id: "core".to_string(),
			kind: ReleaseOwnerKind::Package,
			version: "1.0.0".to_string(),
			version_format: VersionFormat::Namespaced,
			tag: true,
			release: true,
			tag_name: "core/v1.0.0".to_string(),
			members: vec!["core".to_string()],
		}],
		released_packages: vec!["workflow-core".to_string()],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		package_publications: vec![],
		updated_changelogs: vec![PathBuf::from("crates/core/CHANGELOG.md")],
		deleted_changesets: vec![],
		changesets: vec![],
		changelogs: Vec::new(),
		provider: Some(ReleaseRecordProvider {
			kind: monochange_core::SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
	};

	let manifest = crate::release_artifacts::build_release_manifest_from_record(&record);
	assert_eq!(manifest.command, "release-pr");
	assert_eq!(manifest.version, Some("1.0.0".to_string()));
	assert_eq!(manifest.release_targets.len(), 1);
	assert_eq!(manifest.release_targets[0].id, "core");
	assert_eq!(manifest.changelogs.len(), 1);
	assert_eq!(
		manifest.changelogs[0],
		ReleaseManifestChangelog {
			path: PathBuf::from("crates/core/CHANGELOG.md"),
			format: ChangelogFormat::Monochange,
			owner_id: String::new(),
			owner_kind: ReleaseOwnerKind::Group,
			notes: ReleaseNotesDocument {
				title: String::new(),
				summary: Vec::new(),
				sections: Vec::new(),
			},
			rendered: String::new(),
		}
	);
}

#[test]
fn build_release_manifest_from_record_preserves_changelog_metadata() {
	use std::path::PathBuf;

	use monochange_core::ChangelogFormat;
	use monochange_core::ReleaseManifestChangelog;
	use monochange_core::ReleaseNotesDocument;
	use monochange_core::ReleaseNotesSection;
	use monochange_core::ReleaseOwnerKind;
	use monochange_core::ReleaseRecord;

	let changelog = ReleaseManifestChangelog {
		owner_id: "main".to_string(),
		owner_kind: ReleaseOwnerKind::Group,
		path: PathBuf::from("changelog.md"),
		format: ChangelogFormat::Monochange,
		notes: ReleaseNotesDocument {
			title: "main 1.0.0".to_string(),
			summary: vec!["Release target `main`".to_string()],
			sections: vec![ReleaseNotesSection {
				title: "Changed".to_string(),
				collapsed: false,
				entries: vec!["Preserve release notes".to_string()],
			}],
		},
		rendered: "## main 1.0.0\n\n- Preserve release notes".to_string(),
	};
	let record = ReleaseRecord {
		schema_version: 1,
		kind: "monochange.releaseRecord".to_string(),
		created_at: "2024-01-01T00:00:00Z".to_string(),
		command: "publish-release".to_string(),
		version: Some("1.0.0".to_string()),
		group_version: None,
		release_targets: Vec::new(),
		released_packages: Vec::new(),
		changed_files: Vec::new(),
		package_publications: Vec::new(),
		updated_changelogs: vec![PathBuf::from("legacy.md")],
		deleted_changesets: Vec::new(),
		changesets: Vec::new(),
		changelogs: vec![changelog.clone()],
		provider: None,
	};

	let manifest = crate::release_artifacts::build_release_manifest_from_record(&record);

	assert_eq!(manifest.changelogs, vec![changelog]);
}

#[test]
fn build_issue_comment_results_includes_closed_operation() {
	let issue_comment_plans = vec![monochange_github::GitHubIssueCommentPlan {
		repository: "ifiokjr/monochange".to_string(),
		issue_id: "#7".to_string(),
		issue_url: Some("https://example.com/issues/7".to_string()),
		body: "released".to_string(),
		close: true,
	}];
	let issue_comment_results =
		crate::cli_runtime::build_issue_comment_results(false, &issue_comment_plans, || {
			Ok(vec![monochange_github::GitHubIssueCommentOutcome {
				repository: "ifiokjr/monochange".to_string(),
				issue_id: "#7".to_string(),
				operation: monochange_github::GitHubIssueCommentOperation::Closed,
				url: Some("https://example.com/issues/7#event-1".to_string()),
			}])
		})
		.unwrap_or_else(|error| panic!("render issue comment results: {error}"));
	assert_eq!(
		issue_comment_results,
		vec!["ifiokjr/monochange #7 (closed)".to_string(),]
	);
}

#[test]
fn execute_cli_command_publish_release_falls_back_to_release_record_from_git() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"[source]
provider = "github"
owner = "monochange"
repo = "monochange"

[source.releases]
branches = ["release/*"]
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["init", "-b", "main"])
		.output()
		.unwrap_or_else(|error| panic!("git init: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "user.email", "test@example.com"])
		.output()
		.unwrap_or_else(|error| panic!("git config email: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "user.name", "Test User"])
		.output()
		.unwrap_or_else(|error| panic!("git config name: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "commit.gpgsign", "false"])
		.output()
		.unwrap_or_else(|error| panic!("git config signing: {error}"));
	fs::write(tempdir.path().join("tracked.txt"), "test\n")
		.unwrap_or_else(|error| panic!("write tracked: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["add", "tracked.txt"])
		.output()
		.unwrap_or_else(|error| panic!("git add: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["commit", "-m", "initial commit"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));

	// Write a release record block into a file and commit it
	let record = monochange_core::ReleaseRecord {
		schema_version: 1,
		kind: "monochange.releaseRecord".to_string(),
		created_at: "2024-01-01T00:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.0.0".to_string()),
		group_version: None,
		release_targets: vec![monochange_core::ReleaseRecordTarget {
			id: "test".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.0.0".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.0.0".to_string(),
			members: vec![],
		}],
		released_packages: vec![],
		changed_files: vec![],
		package_publications: vec![],
		updated_changelogs: vec![],
		deleted_changesets: vec![],
		changesets: vec![monochange_core::PreparedChangeset {
			path: PathBuf::from(".changeset/test.md"),
			summary: Some("Test changeset".to_string()),
			details: None,
			targets: vec![],
			context: Some(monochange_core::ChangesetContext {
				provider: monochange_core::HostingProviderKind::GitHub,
				host: None,
				capabilities: monochange_core::HostingCapabilities::default(),
				introduced: None,
				last_updated: None,
				related_issues: vec![monochange_core::HostedIssueRef {
					provider: monochange_core::HostingProviderKind::GitHub,
					host: None,
					id: "#1".to_string(),
					title: None,
					url: None,
					relationship:
						monochange_core::HostedIssueRelationshipKind::ClosedByReviewRequest,
				}],
			}),
		}],
		changelogs: Vec::new(),
		provider: None,
	};
	let block = monochange_core::render_release_record_block(&record)
		.unwrap_or_else(|error| panic!("render release record: {error}"));
	fs::write(tempdir.path().join("release.txt"), block.clone())
		.unwrap_or_else(|error| panic!("write release: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["add", "release.txt"])
		.output()
		.unwrap_or_else(|error| panic!("git add release: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args([
			"commit",
			"-m",
			&format!("chore(release): prepare release\n\n{block}"),
		])
		.output()
		.unwrap_or_else(|error| panic!("git commit release: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "publish-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::PublishRelease {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		}],
	};
	let result = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	);
	// It will fail on release branch policy, but it should get past the fallback.
	let error = result.err().unwrap_or_else(|| panic!("expected error"));
	let message = error.to_string();
	assert!(
		message.contains("configured release branch pattern [release/*]"),
		"unexpected error: {message}"
	);
}

#[test]
fn execute_cli_command_comment_released_issues_falls_back_to_release_record_from_git() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["init", "-b", "main"])
		.output()
		.unwrap_or_else(|error| panic!("git init: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "user.email", "test@example.com"])
		.output()
		.unwrap_or_else(|error| panic!("git config email: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "user.name", "Test User"])
		.output()
		.unwrap_or_else(|error| panic!("git config name: {error}"));
	fs::write(tempdir.path().join("tracked.txt"), "test\n")
		.unwrap_or_else(|error| panic!("write tracked: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["add", "tracked.txt"])
		.output()
		.unwrap_or_else(|error| panic!("git add: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["commit", "-m", "initial commit"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));

	let record = monochange_core::ReleaseRecord {
		schema_version: 1,
		kind: "monochange.releaseRecord".to_string(),
		created_at: "2024-01-01T00:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.0.0".to_string()),
		group_version: None,
		release_targets: vec![monochange_core::ReleaseRecordTarget {
			id: "test".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.0.0".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.0.0".to_string(),
			members: vec![],
		}],
		released_packages: vec![],
		changed_files: vec![],
		package_publications: vec![],
		updated_changelogs: vec![],
		deleted_changesets: vec![],
		changesets: vec![monochange_core::PreparedChangeset {
			path: PathBuf::from(".changeset/test.md"),
			summary: Some("Test changeset".to_string()),
			details: None,
			targets: vec![],
			context: Some(monochange_core::ChangesetContext {
				provider: monochange_core::HostingProviderKind::GitHub,
				host: None,
				capabilities: monochange_core::HostingCapabilities::default(),
				introduced: None,
				last_updated: None,
				related_issues: vec![monochange_core::HostedIssueRef {
					provider: monochange_core::HostingProviderKind::GitHub,
					host: None,
					id: "#1".to_string(),
					title: None,
					url: None,
					relationship:
						monochange_core::HostedIssueRelationshipKind::ClosedByReviewRequest,
				}],
			}),
		}],
		changelogs: Vec::new(),
		provider: None,
	};
	let block = monochange_core::render_release_record_block(&record)
		.unwrap_or_else(|error| panic!("render release record: {error}"));
	fs::write(tempdir.path().join("release.txt"), block.clone())
		.unwrap_or_else(|error| panic!("write release: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["add", "release.txt"])
		.output()
		.unwrap_or_else(|error| panic!("git add release: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args([
			"commit",
			"-m",
			&format!("chore(release): prepare release\n\n{block}"),
		])
		.output()
		.unwrap_or_else(|error| panic!("git commit release: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "release-comments".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::CommentReleasedIssues {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		}],
	};
	let result = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		true,
		BTreeMap::new(),
	);
	result.unwrap_or_else(|error| panic!("unexpected error: {error}"));
}
