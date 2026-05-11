use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::ChangelogSettings;
use monochange_core::ChangesetPolicyEvaluation;
use monochange_core::ChangesetPolicyStatus;
use monochange_core::CliCommandDefinition;
use monochange_core::CliStepDefinition;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleasePlan;
use monochange_core::ShellConfig;
use monochange_core::SourceProvider;
use monochange_core::VersionFormat;
use serde::Serialize;
use tempfile::tempdir;

use super::*;
use crate::TEST_ENV_LOCK;

fn cli_context() -> CliContext {
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

fn sample_source_configuration() -> SourceConfiguration {
	let provider = serde_json::from_str::<SourceProvider>("\"github\"")
		.unwrap_or_else(|error| panic!("source provider: {error}"));
	SourceConfiguration {
		provider,
		owner: "monochange".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	}
}

#[test]
fn publish_release_source_configuration_preserves_configured_draft_default() {
	let mut source = sample_source_configuration();
	source.releases.draft = true;

	let configured = publish_release_source_configuration(Some(&source), &BTreeMap::new())
		.unwrap_or_else(|error| panic!("publish source: {error}"));

	assert!(configured.releases.draft);
}

#[test]
fn publish_release_source_configuration_applies_draft_step_override() {
	let source = sample_source_configuration();
	let inputs = BTreeMap::from([("draft".to_string(), vec!["true".to_string()])]);

	let configured = publish_release_source_configuration(Some(&source), &inputs)
		.unwrap_or_else(|error| panic!("publish source: {error}"));

	assert!(configured.releases.draft);
	assert!(!source.releases.draft);
}

#[test]
fn publish_release_source_configuration_keeps_draft_disabled_without_override() {
	let source = sample_source_configuration();

	let configured = publish_release_source_configuration(Some(&source), &BTreeMap::new())
		.unwrap_or_else(|error| panic!("publish source: {error}"));

	assert!(!configured.releases.draft);
}

#[test]
fn publish_release_source_configuration_requires_source_configuration() {
	let error = publish_release_source_configuration(None, &BTreeMap::new())
		.err()
		.unwrap_or_else(|| panic!("expected source configuration error"));

	assert!(error.to_string().contains("[source]"));
}

fn sample_configuration(root: &Path) -> monochange_core::WorkspaceConfiguration {
	monochange_core::WorkspaceConfiguration {
		root_path: root.to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
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
	}
}

fn sample_prepared_release() -> PreparedRelease {
	PreparedRelease {
		plan: ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
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
		dry_run: true,
	}
}

fn sample_prepared_release_with_versions() -> PreparedRelease {
	PreparedRelease {
		plan: ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![
				monochange_core::ReleaseDecision {
					package_id: "core".to_string(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Minor,
					planned_version: Some(semver::Version::new(1, 2, 0)),
					group_id: Some("sdk".to_string()),
					reasons: vec!["feature".to_string()],
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
				monochange_core::ReleaseDecision {
					package_id: "web".to_string(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::Patch,
					planned_version: Some(semver::Version::new(1, 2, 1)),
					group_id: Some("sdk".to_string()),
					reasons: vec!["fix".to_string()],
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
				monochange_core::ReleaseDecision {
					package_id: "docs".to_string(),
					trigger_type: "changeset".to_string(),
					recommended_bump: BumpSeverity::None,
					planned_version: Some(semver::Version::new(9, 9, 9)),
					group_id: None,
					reasons: Vec::new(),
					upstream_sources: Vec::new(),
					warnings: Vec::new(),
				},
			],
			groups: vec![monochange_core::PlannedVersionGroup {
				group_id: "sdk".to_string(),
				display_name: "SDK".to_string(),
				members: vec!["core".to_string(), "web".to_string()],
				mismatch_detected: false,
				planned_version: Some(semver::Version::new(2, 0, 0)),
				recommended_bump: BumpSeverity::Minor,
			}],
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["core".to_string(), "web".to_string()],
		package_publications: Vec::new(),
		version: Some("1.2.1".to_string()),
		group_version: Some("2.0.0".to_string()),
		release_targets: Vec::new(),
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		dry_run: true,
	}
}

fn parse_validate_matches(root: &Path) -> (monochange_core::WorkspaceConfiguration, ArgMatches) {
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("workspace configuration: {error}"));
	let matches = build_command_with_cli("mc", &configuration.cli)
		.try_get_matches_from(["mc", "step:discover"])
		.unwrap_or_else(|error| panic!("discover matches: {error}"));
	(configuration, matches)
}

fn default_cli_command(name: &str) -> CliCommandDefinition {
	let command_name = if name.starts_with("step:") {
		name.to_string()
	} else {
		format!("step:{name}")
	};

	synthetic_step_command_definition(&command_name)
		.unwrap_or_else(|error| panic!("expected default cli command `{name}`: {error}"))
}

fn read_telemetry_events(path: &Path) -> Vec<serde_json::Value> {
	fs::read_to_string(path)
		.unwrap_or_else(|error| panic!("telemetry file should be written: {error}"))
		.lines()
		.map(|line| {
			serde_json::from_str(line)
				.unwrap_or_else(|error| panic!("valid telemetry json: {error}"))
		})
		.collect()
}

#[test]
fn telemetry_progress_format_uses_stable_labels() {
	assert_eq!(telemetry_progress_format(ProgressFormat::Auto), "auto");
	assert_eq!(
		telemetry_progress_format(ProgressFormat::Unicode),
		"unicode"
	);
	assert_eq!(telemetry_progress_format(ProgressFormat::Ascii), "ascii");
	assert_eq!(telemetry_progress_format(ProgressFormat::Json), "json");
}

#[test]
fn default_cli_command_accepts_prefixed_step_names() {
	let command = default_cli_command("step:discover");

	assert_eq!(command.name, "step:discover");
}

fn sample_package_publish_outcome(
	status: package_publish::PackagePublishStatus,
	trust_status: package_publish::TrustedPublishingStatus,
) -> package_publish::PackagePublishOutcome {
	package_publish::PackagePublishOutcome {
		package: "@scope/pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		registry: "npm".to_string(),
		version: "1.2.3".to_string(),
		status,
		message: "published package to npm".to_string(),
		placeholder: false,
		trusted_publishing: package_publish::TrustedPublishingOutcome {
			status: trust_status,
			repository: Some("monochange/monochange".to_string()),
			workflow: Some("publish.yml".to_string()),
			environment: Some("release".to_string()),
			setup_url: Some("https://docs.npmjs.com/cli/v11/commands/npm-trust".to_string()),
			message: "trusted publishing already configured".to_string(),
		},
		command: None,
		stdout: None,
		stderr: None,
	}
}

fn sample_rate_limit_report() -> monochange_core::PublishRateLimitReport {
	monochange_core::PublishRateLimitReport {
		dry_run: true,
		windows: vec![monochange_core::RegistryRateLimitWindowPlan {
			registry: monochange_core::RegistryKind::PubDev,
			operation: monochange_core::RateLimitOperation::Publish,
			limit: Some(12),
			window_seconds: Some(86_400),
			pending: 13,
			batches_required: 2,
			fits_single_window: false,
			confidence: monochange_core::RateLimitConfidence::Medium,
			notes: "pub.dev limit".to_string(),
			evidence: Vec::new(),
		}],
		batches: vec![
			monochange_core::PublishRateLimitBatch {
				registry: monochange_core::RegistryKind::PubDev,
				operation: monochange_core::RateLimitOperation::Publish,
				batch_index: 1,
				total_batches: 2,
				packages: vec!["pkg-a".to_string()],
				recommended_wait_seconds: None,
			},
			monochange_core::PublishRateLimitBatch {
				registry: monochange_core::RegistryKind::PubDev,
				operation: monochange_core::RateLimitOperation::Publish,
				batch_index: 2,
				total_batches: 2,
				packages: vec!["pkg-b".to_string()],
				recommended_wait_seconds: Some(86_400),
			},
		],
		warnings: vec!["needs 2 batches".to_string()],
	}
}

fn git_in_dir(root: &Path, args: &[&str]) {
	let status = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

#[test]
fn evaluate_cli_step_condition_returns_false_for_blank_conditions() {
	assert!(
		!evaluate_cli_step_condition("   ", &cli_context(), &BTreeMap::new())
			.unwrap_or_else(|error| panic!("blank conditions should be treated as false: {error}"))
	);
}

#[test]
fn parse_template_as_boolean_supports_number_null_and_single_item_arrays() {
	assert!(
		parse_template_as_boolean(&serde_json::json!(2), "{{ count }}")
			.unwrap_or_else(|error| panic!("non-zero numbers should be truthy: {error}"))
	);
	assert!(
		!parse_template_as_boolean(&serde_json::Value::Null, "{{ release }}")
			.unwrap_or_else(|error| panic!("null values should be falsey: {error}"))
	);
	assert!(
		!parse_template_as_boolean(&serde_json::json!([""]), "{{ items }}").unwrap_or_else(
			|error| panic!("single-item arrays should recurse into the item value: {error}")
		)
	);
}

#[test]
fn parse_template_as_boolean_rejects_objects() {
	let error = parse_template_as_boolean(&serde_json::json!({ "nested": true }), "{{ inputs }}")
		.unwrap_err();
	assert!(error.to_string().contains("is not a scalar boolean value"));
}

#[test]
fn render_helpers_cover_release_commit_and_markdown_sections() {
	let report = CommitReleaseReport {
		subject: "chore(release): publish".to_string(),
		body: "body".to_string(),
		commit: Some("1234567890abcdef".to_string()),
		tracked_paths: vec![PathBuf::from("Cargo.toml"), PathBuf::from("CHANGELOG.md")],
		dry_run: false,
		status: "already_exists".to_string(),
	};
	let text_lines = render_release_commit_report(&report);
	assert!(
		text_lines
			.iter()
			.any(|line| line.contains("subject: chore(release): publish"))
	);
	assert!(
		text_lines
			.iter()
			.any(|line| line.contains("commit: 1234567"))
	);
	assert!(
		text_lines
			.iter()
			.any(|line| line.contains("tracked paths:"))
	);
	assert!(
		text_lines
			.iter()
			.any(|line| line.contains("status: already-exists"))
	);

	let markdown_lines = render_release_commit_report_markdown(&report, true);
	assert!(
		markdown_lines
			.iter()
			.any(|line| line.contains("**Subject:**"))
	);
	assert!(
		markdown_lines
			.iter()
			.any(|line| line.contains("**Tracked paths:**"))
	);

	assert_eq!(yes_no(true), "yes");
	assert_eq!(yes_no(false), "no");
	assert_eq!(
		paint_markdown_inline("plain", MarkdownStyle::Muted, false),
		"plain"
	);
	assert!(paint_markdown_inline("code", MarkdownStyle::Code, true).contains("\u{1b}[35m"));
	assert!(render_markdown_section("Empty", &[], false).starts_with("## Empty"));
}

#[derive(Debug)]
struct BrokenSerialize;

impl Serialize for BrokenSerialize {
	fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		Err(serde::ser::Error::custom("broken serialize"))
	}
}

#[test]
fn render_json_output_reports_context_on_serialization_failure() {
	let error = render_json_output(&BrokenSerialize, "changeset diagnostics")
		.unwrap_err()
		.to_string();
	assert!(error.contains("failed to render changeset diagnostics as json"));
	assert!(error.contains("broken serialize"));
}

#[test]
fn execute_affected_packages_step_supports_from_git_input() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::create_dir_all(root.join("crates/core/src"))
		.unwrap_or_else(|error| panic!("create workspace directories: {error}"));
	fs::write(
		root.join("monochange.toml"),
		r#"[defaults]
package_type = "cargo"

[changesets.affected]
enabled = true
required = true

[package.core]
path = "crates/core"
"#,
	)
	.unwrap_or_else(|error| panic!("write monochange config: {error}"));
	fs::write(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/core\"]\n",
	)
	.unwrap_or_else(|error| panic!("write workspace Cargo.toml: {error}"));
	fs::write(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
	)
	.unwrap_or_else(|error| panic!("write package Cargo.toml: {error}"));
	fs::write(
		root.join("crates/core/src/lib.rs"),
		"pub fn version() -> &'static str { \"v1\" }\n",
	)
	.unwrap_or_else(|error| panic!("write initial source file: {error}"));

	git_in_dir(root, &["init", "-b", "main"]);
	git_in_dir(root, &["config", "user.name", "monochange Tests"]);
	git_in_dir(root, &["config", "user.email", "monochange@example.com"]);
	git_in_dir(root, &["add", "."]);
	git_in_dir(root, &["commit", "-m", "initial"]);

	fs::write(
		root.join("crates/core/src/lib.rs"),
		"pub fn version() -> &'static str { \"v2\" }\n",
	)
	.unwrap_or_else(|error| panic!("update source file: {error}"));

	let evaluation = execute_affected_packages_step(
		root,
		&BTreeMap::from([("from".to_string(), vec!["HEAD".to_string()])]),
		true,
	)
	.unwrap_or_else(|error| panic!("execute affected packages step: {error}"));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
	assert_eq!(
		evaluation.changed_paths,
		vec!["crates/core/src/lib.rs".to_string()]
	);
	assert_eq!(evaluation.affected_package_ids, vec!["core".to_string()]);
	assert_eq!(evaluation.uncovered_package_ids, vec!["core".to_string()]);
}

#[test]
fn render_cli_command_results_include_release_details_policy_and_logs() {
	let cli_command = default_cli_command("prepare-release");
	let mut context = cli_context();
	context.show_diff = true;
	context.release_manifest_path = Some(PathBuf::from(".monochange/local/release.json"));
	context.release_results = vec!["published v1.2.3".to_string()];
	context.release_request_result = Some("opened release request".to_string());
	context.issue_comment_results = vec!["commented on #42".to_string()];
	context.release_commit_report = Some(CommitReleaseReport {
		subject: "chore(release): publish".to_string(),
		body: "body".to_string(),
		commit: Some("abcdef1234567890".to_string()),
		tracked_paths: vec![PathBuf::from("Cargo.toml")],
		dry_run: false,
		status: "completed".to_string(),
	});
	context.prepared_release = Some(PreparedRelease {
		plan: ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: vec![PathBuf::from(".changeset/feature.md")],
		changesets: Vec::new(),
		released_packages: vec!["core".to_string()],
		version: Some("1.2.3".to_string()),
		group_version: None,
		release_targets: vec![ReleaseTarget {
			id: "core".to_string(),
			kind: ReleaseOwnerKind::Package,
			version: "1.2.3".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.3".to_string(),
			members: Vec::new(),
			rendered_title: "core v1.2.3".to_string(),
			rendered_changelog_title: "core v1.2.3".to_string(),
		}],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: vec![PathBuf::from(".changeset/feature.md")],
		package_publications: Vec::new(),
		dry_run: true,
	});
	context.prepared_file_diffs = vec![PreparedFileDiff {
		path: PathBuf::from("Cargo.toml"),
		diff: "-old\n+new".to_string(),
		display_diff: "--- a/Cargo.toml\n+++ b/Cargo.toml\n-old\n+new".to_string(),
	}];
	context.command_logs = vec!["ran cargo check".to_string()];
	context.changeset_policy_evaluation = Some(ChangesetPolicyEvaluation {
		enforce: true,
		required: true,
		status: ChangesetPolicyStatus::Failed,
		summary: "coverage missing".to_string(),
		comment: None,
		labels: Vec::new(),
		matched_skip_labels: vec!["skip-changeset".to_string()],
		changed_paths: vec!["crates/core/src/lib.rs".to_string()],
		matched_paths: vec!["crates/core/src/lib.rs".to_string()],
		ignored_paths: Vec::new(),
		changeset_paths: vec![".changeset/feature.md".to_string()],
		affected_package_ids: vec!["core".to_string()],
		covered_package_ids: Vec::new(),
		uncovered_package_ids: vec!["core".to_string()],
		errors: vec!["missing changeset".to_string()],
	});

	let text = render_cli_command_result(&cli_command, &context);
	assert!(text.contains("release manifest: .monochange/local/release.json"));
	assert!(text.contains("releases:"));
	assert!(text.contains("release request:"));
	assert!(text.contains("issue comments:"));
	assert!(text.contains("changed files:"));
	assert!(text.contains("file diffs:"));
	assert!(text.contains("deleted changesets:"));
	assert!(text.contains("matched paths:"));
	assert!(text.contains("changeset files:"));
	assert!(text.contains("errors:"));
	assert!(text.contains("commands:"));

	let markdown = render_cli_command_markdown_result(&cli_command, &context);
	assert!(markdown.contains("## Release targets"));
	assert!(markdown.contains("## Release manifest"));
	assert!(markdown.contains("## Release commit"));
	assert!(markdown.contains("## Changed files"));
	assert!(markdown.contains("## File diffs"));
	assert!(markdown.contains("## Deleted changesets"));
	assert!(markdown.contains("## Commands"));
}

#[test]
fn render_display_versions_output_supports_text_markdown_and_json() {
	let prepared_release = sample_prepared_release_with_versions();

	let text = render_display_versions_output(
		&prepared_release,
		&BTreeMap::from([("format".to_string(), vec!["text".to_string()])]),
	)
	.unwrap_or_else(|error| panic!("versions text output: {error}"));
	insta::assert_snapshot!("display_versions_text", text);

	let markdown = render_display_versions_output(
		&prepared_release,
		&BTreeMap::from([("format".to_string(), vec!["markdown".to_string()])]),
	)
	.unwrap_or_else(|error| panic!("versions markdown output: {error}"));
	insta::assert_snapshot!("display_versions_markdown", markdown);

	let json = render_display_versions_output(
		&prepared_release,
		&BTreeMap::from([("format".to_string(), vec!["json".to_string()])]),
	)
	.unwrap_or_else(|error| panic!("versions json output: {error}"));
	let parsed: serde_json::Value = serde_json::from_str(&json)
		.unwrap_or_else(|error| panic!("parse versions json output: {error}"));
	insta::assert_json_snapshot!("display_versions_json", parsed);
}

#[test]
fn release_version_summary_renderers_cover_empty_and_single_section_states() {
	let empty = ReleaseVersionSummary {
		groups: BTreeMap::new(),
		packages: BTreeMap::new(),
	};
	assert_eq!(
		render_release_version_summary_text(&empty),
		"no package or group versions were planned"
	);
	assert_eq!(
		render_release_version_summary_markdown(&empty),
		"No package or group versions were planned."
	);

	let groups_only = ReleaseVersionSummary {
		groups: BTreeMap::from([("sdk".to_string(), "2.0.0".to_string())]),
		packages: BTreeMap::new(),
	};
	assert_eq!(
		render_release_version_summary_text(&groups_only),
		"group versions:\n- sdk: 2.0.0"
	);
	assert_eq!(
		render_release_version_summary_markdown(&groups_only),
		"## Group versions\n\n- `sdk`: `2.0.0`"
	);

	let packages_only = ReleaseVersionSummary {
		groups: BTreeMap::new(),
		packages: BTreeMap::from([
			("core".to_string(), "1.2.0".to_string()),
			("web".to_string(), "1.2.1".to_string()),
		]),
	};
	assert_eq!(
		render_release_version_summary_text(&packages_only),
		"package versions:\n- core: 1.2.0\n- web: 1.2.1"
	);
	assert_eq!(
		render_release_version_summary_markdown(&packages_only),
		"## Package versions\n\n- `core`: `1.2.0`\n- `web`: `1.2.1`"
	);
}

#[test]
fn render_cli_command_results_include_package_publish_reports() {
	let cli_command = CliCommandDefinition {
		name: "publish".to_string(),
		help_text: Some("publish packages".to_string()),
		inputs: vec![monochange_core::CliInputDefinition {
			name: "format".to_string(),
			kind: CliInputKind::Choice,
			help_text: Some("Output format".to_string()),
			required: false,
			default: Some("text".to_string()),
			choices: vec![
				"text".to_string(),
				"markdown".to_string(),
				"json".to_string(),
			],
			short: None,
		}],
		steps: vec![CliStepDefinition::PublishPackages {
			name: Some("publish packages".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};
	let mut context = cli_context();
	context.package_publish_report = Some(package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![package_publish::PackagePublishOutcome {
			package: "@scope/pkg".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: "npm".to_string(),
			version: "1.2.3".to_string(),
			status: package_publish::PackagePublishStatus::Published,
			message: "published package to npm".to_string(),
			placeholder: false,
			trusted_publishing: package_publish::TrustedPublishingOutcome {
				status: package_publish::TrustedPublishingStatus::Configured,
				repository: Some("monochange/monochange".to_string()),
				workflow: Some("publish.yml".to_string()),
				environment: Some("release".to_string()),
				setup_url: None,
				message: "trusted publishing already configured".to_string(),
			},
			command: None,
			stdout: None,
			stderr: None,
		}],
	});
	context.command_logs = vec!["ran npm trust".to_string()];

	let text = render_cli_command_result(&cli_command, &context);
	assert!(text.contains("package publishing:"));
	assert!(text.contains("@scope/pkg"));
	assert!(text.contains("trusted publishing: configured"));
	assert!(text.contains("repository: monochange/monochange"));
	assert!(text.contains("commands:"));

	let markdown = render_cli_command_markdown_result(&cli_command, &context);
	assert!(markdown.contains("## Package publishing"));
	assert!(markdown.contains("**Trusted publishing:** configured"));
	assert!(markdown.contains("**Workflow:** `publish.yml`"));
	assert!(markdown.contains("## Commands"));
}

#[test]
fn filter_placeholder_publish_report_hides_completed_dry_run_packages_by_default() {
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Placeholder,
		dry_run: true,
		packages: vec![
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::Planned,
				package_publish::TrustedPublishingStatus::Planned,
			),
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::Blocked,
				package_publish::TrustedPublishingStatus::Planned,
			),
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::SkippedExisting,
				package_publish::TrustedPublishingStatus::Configured,
			),
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::SkippedExternal,
				package_publish::TrustedPublishingStatus::Disabled,
			),
		],
	};

	let filtered = filter_placeholder_publish_report(report.clone(), false);
	let statuses = filtered
		.packages
		.iter()
		.map(|package| package.status)
		.collect::<Vec<_>>();

	assert_eq!(
		statuses,
		vec![
			package_publish::PackagePublishStatus::Planned,
			package_publish::PackagePublishStatus::Blocked,
		]
	);
	assert_eq!(
		filter_placeholder_publish_report(report, true)
			.packages
			.len(),
		4
	);
}

#[test]
fn filter_placeholder_publish_report_hides_unchanged_real_run_packages_by_default() {
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Placeholder,
		dry_run: false,
		packages: vec![
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::Published,
				package_publish::TrustedPublishingStatus::Configured,
			),
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::Failed,
				package_publish::TrustedPublishingStatus::Disabled,
			),
			sample_package_publish_outcome(
				package_publish::PackagePublishStatus::SkippedExisting,
				package_publish::TrustedPublishingStatus::Configured,
			),
		],
	};

	let filtered = filter_placeholder_publish_report(report, false);
	let statuses = filtered
		.packages
		.iter()
		.map(|package| package.status)
		.collect::<Vec<_>>();

	assert_eq!(
		statuses,
		vec![
			package_publish::PackagePublishStatus::Published,
			package_publish::PackagePublishStatus::Failed,
		]
	);
}

#[test]
fn render_package_publish_reports_cover_empty_and_detailed_variants() {
	let empty_placeholder = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Placeholder,
		dry_run: true,
		packages: Vec::new(),
	};
	let text_lines = render_package_publish_report(&empty_placeholder);
	assert_eq!(text_lines[0], "placeholder publishing:");
	assert_eq!(
		text_lines[1],
		"- no packages matched the publishing criteria"
	);
	assert_eq!(
		render_package_publish_report_markdown(&empty_placeholder, false),
		vec!["- no packages matched the publishing criteria".to_string()]
	);

	let detailed_report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![sample_package_publish_outcome(
			package_publish::PackagePublishStatus::SkippedExternal,
			package_publish::TrustedPublishingStatus::ManualActionRequired,
		)],
	};
	let text = render_package_publish_report(&detailed_report).join("\n");
	assert!(text.contains("repository: monochange/monochange"));
	assert!(text.contains("workflow: publish.yml"));
	assert!(text.contains("environment: release"));
	assert!(text.contains("setup: https://docs.npmjs.com/cli/v11/commands/npm-trust"));

	let markdown = render_package_publish_report_markdown(&detailed_report, false).join("\n");
	assert!(markdown.contains("**Repository:** `monochange/monochange`"));
	assert!(markdown.contains("**Workflow:** `publish.yml`"));
	assert!(markdown.contains("**Environment:** `release`"));
	assert!(markdown.contains("**Setup:** `https://docs.npmjs.com/cli/v11/commands/npm-trust`"));
}

#[test]
fn render_package_publish_reports_include_command_output_blocks() {
	let mut outcome = sample_package_publish_outcome(
		package_publish::PackagePublishStatus::Published,
		package_publish::TrustedPublishingStatus::Disabled,
	);
	outcome.command = Some("npm publish --access public".to_string());
	outcome.stdout = Some("published\nwith provenance".to_string());
	outcome.stderr = Some("npm notice package".to_string());
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Placeholder,
		dry_run: false,
		packages: vec![outcome],
	};

	let text = render_package_publish_report(&report).join("\n");
	assert!(text.contains("command: npm publish --access public"));
	assert!(text.contains("stdout:\n    │ published\n    │ with provenance"));
	assert!(text.contains("stderr:\n    │ npm notice package"));

	let markdown = render_package_publish_report_markdown(&report, false).join("\n");
	assert!(markdown.contains("**Command:** `npm publish --access public`"));
	assert!(markdown.contains("**stdout:**\n  ```text\n  published\n  with provenance\n  ```"));
	assert!(markdown.contains("**stderr:**\n  ```text\n  npm notice package\n  ```"));
}

#[test]
fn render_package_publish_reports_include_manual_registry_guidance() {
	let report = package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![package_publish::PackagePublishOutcome {
			package: "pkg".to_string(),
			ecosystem: Ecosystem::Cargo,
			registry: "crates_io".to_string(),
			version: "1.2.3".to_string(),
			status: package_publish::PackagePublishStatus::SkippedExternal,
			message: "skipped built-in publish".to_string(),
			placeholder: false,
			trusted_publishing: package_publish::TrustedPublishingOutcome {
				status: package_publish::TrustedPublishingStatus::ManualActionRequired,
				repository: Some("monochange/monochange".to_string()),
				workflow: Some("publish.yml".to_string()),
				environment: Some("release".to_string()),
				setup_url: Some("https://crates.io/crates/pkg".to_string()),
				message:
					"configure trusted publishing manually for `pkg` before the next built-in release publish"
						.to_string(),
			},
			command: None,
			stdout: None,
			stderr: None,
		}],
	};

	let text = render_package_publish_report(&report).join("\n");
	assert!(text.contains("trusted publishing: manual-action-required"));
	assert!(text.contains("trust message: configure trusted publishing manually for `pkg`"));
	assert!(text.contains("setup: https://crates.io/crates/pkg"));
	assert!(text.contains(
		"next: open the setup URL, configure trusted publishing for this package, then rerun `mc publish`"
	));

	let markdown = render_package_publish_report_markdown(&report, false).join("\n");
	assert!(markdown.contains("**Trusted publishing:** manual-action-required"));
	assert!(
		markdown.contains("**Trust message:** configure trusted publishing manually for `pkg`")
	);
	assert!(markdown.contains("**Setup:** `https://crates.io/crates/pkg`"));
	assert!(markdown.contains("**Next:** open the setup URL, configure trusted publishing for this package, then rerun `mc publish`"));
}

#[test]
fn package_publish_status_labels_cover_all_variants() {
	assert_eq!(
		package_publish_status_label(package_publish::PackagePublishStatus::Planned),
		"planned"
	);
	assert_eq!(
		package_publish_status_label(package_publish::PackagePublishStatus::Published),
		"published"
	);
	assert_eq!(
		package_publish_status_label(package_publish::PackagePublishStatus::SkippedExisting),
		"skipped-existing"
	);
	assert_eq!(
		package_publish_status_label(package_publish::PackagePublishStatus::SkippedExternal),
		"skipped-external"
	);
	assert_eq!(
		package_publish_status_label(package_publish::PackagePublishStatus::Blocked),
		"blocked"
	);
	assert_eq!(
		package_publish_status_label(package_publish::PackagePublishStatus::Failed),
		"failed"
	);
}

#[test]
fn trusted_publishing_status_labels_cover_all_variants() {
	assert_eq!(
		trusted_publishing_status_label(package_publish::TrustedPublishingStatus::Disabled),
		"disabled"
	);
	assert_eq!(
		trusted_publishing_status_label(package_publish::TrustedPublishingStatus::Planned),
		"planned"
	);
	assert_eq!(
		trusted_publishing_status_label(package_publish::TrustedPublishingStatus::Configured),
		"configured"
	);
	assert_eq!(
		trusted_publishing_status_label(
			package_publish::TrustedPublishingStatus::ManualActionRequired
		),
		"manual-action-required"
	);
}

#[test]
fn resolve_command_output_supports_package_publish_json_without_release_state() {
	let cli_command = CliCommandDefinition {
		name: "placeholder-publish".to_string(),
		help_text: Some("publish placeholders".to_string()),
		inputs: vec![monochange_core::CliInputDefinition {
			name: "format".to_string(),
			kind: CliInputKind::Choice,
			help_text: Some("Output format".to_string()),
			required: false,
			default: Some("text".to_string()),
			choices: vec![
				"text".to_string(),
				"markdown".to_string(),
				"json".to_string(),
			],
			short: None,
		}],
		steps: vec![CliStepDefinition::PlaceholderPublish {
			name: Some("publish placeholder packages".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};
	let mut context = cli_context();
	context.last_step_inputs = BTreeMap::from([("format".to_string(), vec!["json".to_string()])]);
	context.package_publish_report = Some(package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Placeholder,
		dry_run: true,
		packages: vec![package_publish::PackagePublishOutcome {
			package: "core".to_string(),
			ecosystem: Ecosystem::Cargo,
			registry: "crates_io".to_string(),
			version: "0.0.0".to_string(),
			status: package_publish::PackagePublishStatus::Planned,
			message: "would publish placeholder package".to_string(),
			placeholder: true,
			trusted_publishing: package_publish::TrustedPublishingOutcome {
				status: package_publish::TrustedPublishingStatus::ManualActionRequired,
				repository: None,
				workflow: None,
				environment: None,
				setup_url: Some("https://crates.io/docs/trusted-publishing".to_string()),
				message: "configure trusted publishing manually after the placeholder release"
					.to_string(),
			},
			command: None,
			stdout: None,
			stderr: None,
		}],
	});

	let rendered = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("package publish json output: {error}"));
	let parsed: serde_json::Value = serde_json::from_str(&rendered)
		.unwrap_or_else(|error| panic!("parse package publish json output: {error}"));
	assert_eq!(
		parsed["packagePublish"]["mode"],
		serde_json::json!("placeholder")
	);
	assert_eq!(parsed["packagePublish"]["dryRun"], serde_json::json!(true));
	assert_eq!(
		parsed["packagePublish"]["packages"][0]["package"],
		serde_json::json!("core")
	);
	assert_eq!(
		parsed["packagePublish"]["packages"][0]["trustedPublishing"]["status"],
		serde_json::json!("manual_action_required")
	);
}

#[test]
fn resolve_command_output_supports_package_publish_text_and_markdown_without_release_state() {
	let cli_command = CliCommandDefinition {
		name: "placeholder-publish".to_string(),
		help_text: Some("publish placeholders".to_string()),
		inputs: vec![monochange_core::CliInputDefinition {
			name: "format".to_string(),
			kind: CliInputKind::Choice,
			help_text: Some("Output format".to_string()),
			required: false,
			default: Some("text".to_string()),
			choices: vec![
				"text".to_string(),
				"markdown".to_string(),
				"json".to_string(),
			],
			short: None,
		}],
		steps: vec![CliStepDefinition::PlaceholderPublish {
			name: Some("publish placeholder packages".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};

	let mut text_context = cli_context();
	text_context.last_step_inputs =
		BTreeMap::from([("format".to_string(), vec!["text".to_string()])]);
	text_context.package_publish_report = Some(package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Placeholder,
		dry_run: true,
		packages: Vec::new(),
	});
	let text = resolve_command_output(&cli_command, &text_context, true, None)
		.unwrap_or_else(|error| panic!("package publish text output: {error}"));
	assert!(text.contains("placeholder publishing:"));
	assert!(text.contains("no packages matched the publishing criteria"));

	let mut markdown_context = cli_context();
	markdown_context.last_step_inputs =
		BTreeMap::from([("format".to_string(), vec!["markdown".to_string()])]);
	markdown_context.package_publish_report = Some(package_publish::PackagePublishReport {
		mode: package_publish::PackagePublishRunMode::Placeholder,
		dry_run: true,
		packages: Vec::new(),
	});
	let markdown = resolve_command_output(&cli_command, &markdown_context, true, None)
		.unwrap_or_else(|error| panic!("package publish markdown output: {error}"));
	assert!(markdown.contains("## Placeholder publishing"));
	assert!(markdown.contains("no packages matched the publishing criteria"));
}

#[test]
fn resolve_command_output_supports_publish_rate_limit_reports_without_release_state() {
	let cli_command = CliCommandDefinition {
		name: "publish-plan".to_string(),
		help_text: Some("plan publish rate limits".to_string()),
		inputs: vec![monochange_core::CliInputDefinition {
			name: "format".to_string(),
			kind: CliInputKind::Choice,
			help_text: Some("Output format".to_string()),
			required: false,
			default: Some("text".to_string()),
			choices: vec!["text".to_string(), "json".to_string()],
			short: None,
		}],
		steps: vec![CliStepDefinition::PlanPublishRateLimits {
			name: Some("plan publish rate limits".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};

	let mut context = cli_context();
	context.last_step_inputs = BTreeMap::from([("format".to_string(), vec!["text".to_string()])]);
	context.rate_limit_report = Some(sample_rate_limit_report());

	let text = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit text output: {error}"));
	assert!(text.contains("publish rate limits:"));
	assert!(text.contains("batches=2"));
	assert!(text.contains("planned batches:"));
	assert!(text.contains("wait: 86400s before this batch"));

	context.last_step_inputs = BTreeMap::from([("format".to_string(), vec!["json".to_string()])]);
	let json = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit json output: {error}"));
	assert!(json.contains("batchesRequired"));
	assert!(json.contains("publishRateLimits"));

	context.last_step_inputs =
		BTreeMap::from([("ci".to_string(), vec!["github-actions".to_string()])]);
	let github = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit github snippet: {error}"));
	assert!(github.contains("jobs:"));
	assert!(github.contains("wait_seconds: 86400"));
	assert!(github.contains("mc publish"));

	context.last_step_inputs = BTreeMap::from([("ci".to_string(), vec!["gitlab-ci".to_string()])]);
	let gitlab = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit gitlab snippet: {error}"));
	assert!(gitlab.contains("publish_batches:"));
	assert!(gitlab.contains("WAIT_SECONDS: \"86400\""));

	context.last_step_inputs = BTreeMap::from([("format".to_string(), vec!["text".to_string()])]);
	context.rate_limit_report = Some(monochange_core::PublishRateLimitReport {
		dry_run: true,
		windows: Vec::new(),
		batches: Vec::new(),
		warnings: Vec::new(),
	});
	let empty = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("empty rate limit output: {error}"));
	assert!(empty.contains("no publish operations matched the current plan"));

	let mut windows_without_batches = sample_rate_limit_report();
	windows_without_batches.batches.clear();
	context.rate_limit_report = Some(windows_without_batches);
	let no_batches = resolve_command_output(&cli_command, &context, true, None)
		.unwrap_or_else(|error| panic!("rate limit output without batches: {error}"));
	assert!(no_batches.contains("publish rate limits:"));
	assert!(!no_batches.contains("planned batches:"));
}

#[test]
fn normalize_when_expression_preserves_inequality_and_mid_token_bangs() {
	assert_eq!(
		normalize_when_expression("{{ flag != other }}"),
		"{{ flag != other }}"
	);
	assert_eq!(normalize_when_expression("{{ foo!bar }}"), "{{ foo!bar }}");
}

#[test]
fn publish_rate_limit_helpers_parse_package_filters_modes_and_ci_renderers() {
	assert_eq!(
		selected_package_ids(&BTreeMap::from([(
			"package".to_string(),
			vec!["core".to_string(), "web".to_string(), "core".to_string()],
		)])),
		BTreeSet::from(["core".to_string(), "web".to_string()])
	);
	assert_eq!(
		publish_rate_limit_mode_from_inputs(&BTreeMap::new())
			.unwrap_or_else(|error| panic!("default mode: {error}")),
		publish_rate_limits::PublishRateLimitMode::Publish
	);
	assert_eq!(
		publish_rate_limit_mode_from_inputs(&BTreeMap::from([(
			"mode".to_string(),
			vec!["placeholder".to_string()],
		)]))
		.unwrap_or_else(|error| panic!("placeholder mode: {error}")),
		publish_rate_limits::PublishRateLimitMode::Placeholder
	);
	assert_eq!(
		requested_ci_renderer(&BTreeMap::from([(
			"ci".to_string(),
			vec!["gitlab-ci".to_string()],
		)]))
		.unwrap_or_else(|error| panic!("ci renderer: {error}")),
		Some("gitlab-ci")
	);

	let mode_error = publish_rate_limit_mode_from_inputs(&BTreeMap::from([(
		"mode".to_string(),
		vec!["ship-it".to_string()],
	)]))
	.expect_err("expected invalid mode error");
	assert!(
		mode_error
			.to_string()
			.contains("unsupported publish plan mode `ship-it`")
	);

	let renderer_error = requested_ci_renderer(&BTreeMap::from([(
		"ci".to_string(),
		vec!["circleci".to_string()],
	)]))
	.expect_err("expected invalid renderer error");
	assert!(
		renderer_error
			.to_string()
			.contains("unsupported publish CI renderer `circleci`")
	);

	let snippet_error =
		render_publish_rate_limit_ci_snippet(&sample_rate_limit_report(), "circleci")
			.expect_err("expected unsupported snippet renderer error");
	assert!(
		snippet_error
			.to_string()
			.contains("unsupported publish CI renderer `circleci`")
	);
}

#[test]
fn build_cli_template_context_exposes_publish_rate_limits_without_publish_results() {
	let mut context = cli_context();
	context.rate_limit_report = Some(sample_rate_limit_report());

	let template_context = build_cli_template_context(&context, &BTreeMap::new(), None);
	assert_eq!(
		template_context
			.get("publish_rate_limits")
			.and_then(serde_json::Value::as_object)
			.and_then(|value| value.get("dryRun"))
			.and_then(serde_json::Value::as_bool),
		Some(true)
	);
	assert!(!template_context.contains_key("publish"));
}

#[test]
fn parse_string_as_boolean_rejects_invalid_values() {
	let error = parse_string_as_boolean("maybe", "{{ inputs.run }}").unwrap_err();
	assert_eq!(
		error.to_string(),
		"config error: `when` condition `{{ inputs.run }}` must be a boolean, got `maybe`"
	);
}

#[test]
fn map_process_spawn_result_reports_io_failures() {
	let error = map_process_spawn_result(Err(io::Error::other("boom")), "echo hello").unwrap_err();
	assert_eq!(
		error.to_string(),
		"io error: failed to run command `echo hello`: boom"
	);
}

#[test]
fn render_display_versions_output_rejects_unknown_formats() {
	let error = render_display_versions_output(
		&sample_prepared_release_with_versions(),
		&BTreeMap::from([("format".to_string(), vec!["yaml".to_string()])]),
	)
	.unwrap_err();
	assert_eq!(
		error.to_string(),
		"config error: unsupported output format `yaml`"
	);
}

#[test]
fn execute_matches_uses_progress_format_from_environment_and_rejects_invalid_values() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	temp_env::with_var("MONOCHANGE_PROGRESS_FORMAT", Some("json"), || {
		let (configuration, matches) = parse_validate_matches(tempdir.path());
		let step_matches = matches
			.subcommand_matches("step:discover")
			.unwrap_or_else(|| panic!("step:discover subcommand matches"));
		execute_matches(
			tempdir.path(),
			&configuration,
			"step:discover",
			step_matches,
			false,
		)
		.unwrap_or_else(|error| panic!("step:discover with env progress format: {error}"));
	});

	temp_env::with_var("MONOCHANGE_PROGRESS_FORMAT", Some("wat"), || {
		let (configuration, matches) = parse_validate_matches(tempdir.path());
		let step_matches = matches
			.subcommand_matches("step:discover")
			.unwrap_or_else(|| panic!("step:discover subcommand matches"));
		let error = execute_matches(
			tempdir.path(),
			&configuration,
			"step:discover",
			step_matches,
			false,
		)
		.unwrap_err();
		assert_eq!(
			error.to_string(),
			"config error: unknown progress format `wat`; expected one of: auto, unicode, ascii, json"
		);
	});
}

#[test]
fn run_cli_command_command_streams_output_when_progress_is_enabled() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let mut context = cli_context();
	context.root = tempdir.path().to_path_buf();
	let step_inputs = BTreeMap::new();
	let step = CliStepDefinition::Command {
		name: Some("announce release".to_string()),
		when: None,
		always_run: false,
		command: "printf 'streamed line\\n'".to_string(),
		dry_run_command: None,
		show_progress: None,
		shell: ShellConfig::Default,
		id: Some("stream".to_string()),
		variables: None,
		inputs: BTreeMap::new(),
	};
	let cli_command = CliCommandDefinition {
		name: "release".to_string(),
		help_text: Some("release".to_string()),
		inputs: Vec::new(),
		steps: vec![step.clone()],
		dry_run: false,
	};
	let mut progress = CliProgressReporter::new(&cli_command, false, false, ProgressFormat::Json);

	run_cli_command_command(
		&mut context,
		&step,
		0,
		&mut progress,
		true,
		CommandStepOptions {
			command: "printf 'streamed line\\n'",
			dry_run_command: None,
			shell: &ShellConfig::Default,
			step_id: Some("stream"),
			variables: None,
			step_inputs: &step_inputs,
		},
	)
	.unwrap_or_else(|error| panic!("streaming command step: {error}"));

	assert_eq!(context.command_logs, vec!["streamed line".to_string()]);
	assert_eq!(
		context
			.step_outputs
			.get("stream")
			.map(|output| output.stdout.as_str()),
		Some("streamed line")
	);
}

#[test]
fn take_process_stream_reports_missing_pipes() {
	let error = take_process_stream::<Vec<u8>>(None, "stdout", "echo hello").unwrap_err();
	assert_eq!(
		error.to_string(),
		"io error: failed to capture stdout for command `echo hello`"
	);
}

#[test]
fn step_shows_progress_disables_interactive_change_steps_by_default() {
	let step = CliStepDefinition::CreateChangeFile {
		show_progress: None,
		name: Some("interactive change".to_string()),
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	};
	let mut step_inputs = BTreeMap::new();
	step_inputs.insert("interactive".to_string(), vec!["true".to_string()]);
	assert!(!step_shows_progress(&step, &step_inputs));
	step_inputs.insert("interactive".to_string(), vec!["false".to_string()]);
	assert!(step_shows_progress(&step, &step_inputs));
}

#[test]
fn step_shows_progress_respects_explicit_step_flags() {
	let step = CliStepDefinition::Command {
		show_progress: Some(false),
		name: Some("interactive shell".to_string()),
		when: None,
		always_run: false,
		command: "echo hello".to_string(),
		dry_run_command: None,
		shell: ShellConfig::Default,
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	assert!(!step_shows_progress(&step, &BTreeMap::new()));
}

#[test]
fn drain_stream_events_collects_stdout_stderr_and_handles_closed_channels() {
	let cli_command = CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
		dry_run: false,
	};
	let mut progress = CliProgressReporter::new(&cli_command, false, false, ProgressFormat::Auto);
	let step = CliStepDefinition::Command {
		show_progress: None,
		name: Some("stream output".to_string()),
		when: None,
		always_run: false,
		command: "echo hello".to_string(),
		dry_run_command: None,
		shell: ShellConfig::Default,
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	let (sender, receiver) = mpsc::channel();
	sender
		.send(StreamEvent::Chunk(
			CommandStream::Stdout,
			b"hello\n".to_vec(),
		))
		.unwrap_or_else(|error| panic!("send stdout: {error}"));
	sender
		.send(StreamEvent::Chunk(
			CommandStream::Stderr,
			b"warn\n".to_vec(),
		))
		.unwrap_or_else(|error| panic!("send stderr: {error}"));
	sender
		.send(StreamEvent::Closed(CommandStream::Stdout))
		.unwrap_or_else(|error| panic!("close stdout: {error}"));
	sender
		.send(StreamEvent::Closed(CommandStream::Stderr))
		.unwrap_or_else(|error| panic!("close stderr: {error}"));
	drop(sender);
	let (stdout, stderr) = drain_stream_events(&receiver, &mut progress, 0, &step);
	assert_eq!(stdout, b"hello\n");
	assert_eq!(stderr, b"warn\n");

	let (sender, receiver) = mpsc::channel();
	drop(sender);
	let (stdout, stderr) = drain_stream_events(&receiver, &mut progress, 0, &step);
	assert!(stdout.is_empty());
	assert!(stderr.is_empty());
}

#[test]
fn map_process_wait_result_reports_io_failures() {
	let error =
		map_process_wait_result(Err(io::Error::other("wait failed")), "echo hello").unwrap_err();
	assert_eq!(
		error.to_string(),
		"io error: failed to wait for command `echo hello`: wait failed"
	);
}

#[test]
fn configured_config_step_uses_generic_completion_without_config_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = sample_configuration(tempdir.path());
	let cli_command = CliCommandDefinition {
		name: "configured-config".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![CliStepDefinition::Config {
			name: None,
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};

	let output = execute_cli_command_with_options(
		tempdir.path(),
		&configuration,
		&cli_command,
		ExecuteCliCommandOptions {
			dry_run: true,
			quiet: true,
			show_diff: false,
			inputs: BTreeMap::new(),
			prepared_release_path: None,
			progress_format: ProgressFormat::Auto,
		},
	)
	.unwrap_or_else(|error| panic!("config command: {error}"));

	assert_eq!(output, "command `configured-config` completed (dry-run)");
	assert!(!output.contains("projectRoot"));
}

#[test]
fn execute_cli_command_captures_telemetry_when_step_input_resolution_fails() {
	let _guard = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let telemetry_path = tempdir.path().join("telemetry-input-error.jsonl");
	let telemetry_path_value = telemetry_path.to_string_lossy().to_string();
	let configuration = sample_configuration(tempdir.path());
	let cli_command = CliCommandDefinition {
		name: "telemetry-input-error".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![CliStepDefinition::Validate {
			name: Some("invalid input".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::from([(
				"target".to_string(),
				CliStepInputValue::String("{{".to_string()),
			)]),
		}],
		dry_run: false,
	};

	temp_env::with_vars(
		[
			("MC_TELEMETRY", None::<&str>),
			("MC_TELEMETRY_FILE", Some(telemetry_path_value.as_str())),
		],
		|| {
			let error = execute_cli_command(
				tempdir.path(),
				&configuration,
				&cli_command,
				true,
				BTreeMap::new(),
			)
			.unwrap_err();
			assert!(matches!(error, MonochangeError::Config(_)));
		},
	);

	let events = read_telemetry_events(&telemetry_path);
	let step_event = events
		.iter()
		.find(|event| {
			event["body"]["string_value"] == "command_step"
				&& event["attributes"]["outcome"] == "error"
		})
		.unwrap_or_else(|| panic!("expected command_step event: {events:#?}"));
	let run_event = events
		.iter()
		.find(|event| {
			event["body"]["string_value"] == "command_run"
				&& event["attributes"]["outcome"] == "error"
		})
		.unwrap_or_else(|| panic!("expected command_run event: {events:#?}"));

	assert_eq!(step_event["attributes"]["outcome"], "error");
	assert_eq!(step_event["attributes"]["error_kind"], "config_error");
	assert_eq!(run_event["attributes"]["outcome"], "error");
	assert_eq!(run_event["attributes"]["error_kind"], "config_error");
}

#[test]
fn execute_cli_command_captures_telemetry_when_step_condition_fails() {
	let _guard = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let telemetry_path = tempdir.path().join("telemetry-condition-error.jsonl");
	let telemetry_path_value = telemetry_path.to_string_lossy().to_string();
	let configuration = sample_configuration(tempdir.path());
	let cli_command = CliCommandDefinition {
		name: "telemetry-condition-error".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![CliStepDefinition::Validate {
			name: Some("invalid condition".to_string()),
			when: Some("{{ missing.path }}".to_string()),
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};

	temp_env::with_vars(
		[
			("MC_TELEMETRY", None::<&str>),
			("MC_TELEMETRY_FILE", Some(telemetry_path_value.as_str())),
		],
		|| {
			let error = execute_cli_command(
				tempdir.path(),
				&configuration,
				&cli_command,
				true,
				BTreeMap::new(),
			)
			.unwrap_err();
			assert!(matches!(error, MonochangeError::Config(_)));
		},
	);

	let events = read_telemetry_events(&telemetry_path);
	let step_event = events
		.iter()
		.find(|event| {
			event["body"]["string_value"] == "command_step"
				&& event["attributes"]["outcome"] == "error"
		})
		.unwrap_or_else(|| panic!("expected command_step event: {events:#?}"));
	let run_event = events
		.iter()
		.find(|event| {
			event["body"]["string_value"] == "command_run"
				&& event["attributes"]["outcome"] == "error"
		})
		.unwrap_or_else(|| panic!("expected command_run event: {events:#?}"));

	assert_eq!(step_event["attributes"]["outcome"], "error");
	assert_eq!(step_event["attributes"]["error_kind"], "config_error");
	assert_eq!(run_event["attributes"]["outcome"], "error");
	assert_eq!(run_event["attributes"]["error_kind"], "config_error");
}

#[test]
fn execute_cli_command_reports_command_failures_after_progress_callbacks() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let cli_command = CliCommandDefinition {
		name: "fail".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![CliStepDefinition::Command {
			show_progress: None,
			name: Some("fail loud".to_string()),
			when: None,
			always_run: false,
			command: "printf 'boom\\n' >&2; exit 3".to_string(),
			dry_run_command: None,
			shell: ShellConfig::Default,
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};

	let configuration = monochange_core::WorkspaceConfiguration {
		root_path: tempdir.path().to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
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
	let error = execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	)
	.unwrap_err();
	assert_eq!(
		error.to_string(),
		"discovery error: command `printf 'boom\\n' >&2; exit 3` failed: boom"
	);
}

#[test]
fn build_release_template_value_serializes_file_diffs() {
	let mut context = cli_context();
	context.prepared_release = Some(PreparedRelease {
		plan: ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["core".to_string()],
		version: Some("1.2.3".to_string()),
		group_version: None,
		release_targets: Vec::new(),
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		package_publications: Vec::new(),
		dry_run: true,
	});
	context.prepared_file_diffs = vec![PreparedFileDiff {
		path: PathBuf::from("Cargo.toml"),
		diff: "-old\n+new".to_string(),
		display_diff: "--- a/Cargo.toml\n+++ b/Cargo.toml\n-old\n+new".to_string(),
	}];

	let manifest = build_release_template_value(&context);

	let file_diffs = manifest
		.get("file_diffs")
		.and_then(serde_json::Value::as_array)
		.unwrap_or_else(|| panic!("release template should include file_diffs"));
	assert_eq!(file_diffs.len(), 1);
	assert_eq!(file_diffs[0]["path"], serde_json::json!("Cargo.toml"));
	assert_eq!(file_diffs[0]["diff"], serde_json::json!("-old\n+new"));
}

#[test]
fn execute_cli_command_with_options_covers_final_artifact_save_call() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let cli_command = CliCommandDefinition {
		name: "noop".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
		dry_run: false,
	};

	let output = execute_cli_command_with_options(
		tempdir.path(),
		&sample_configuration(tempdir.path()),
		&cli_command,
		ExecuteCliCommandOptions {
			dry_run: false,
			quiet: true,
			show_diff: false,
			inputs: BTreeMap::new(),
			prepared_release_path: None,
			progress_format: ProgressFormat::Auto,
		},
	)
	.unwrap_or_else(|error| panic!("execute noop command: {error}"));

	assert_eq!(output, "command `noop` completed");
}

#[test]
fn execute_cli_command_with_options_plans_publish_rate_limits_from_prepared_release_artifact() {
	let root = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
		.unwrap_or_else(|error| panic!("workspace root: {error}"));
	let configuration = sample_configuration(&root);
	let artifact_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let artifact_path = artifact_dir.path().join("prepared-release.json");
	let cli_command = CliCommandDefinition {
		name: "publish-plan".to_string(),
		help_text: Some("plan publish rate limits".to_string()),
		inputs: Vec::new(),
		steps: vec![
			CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
				allow_empty_changesets: false,
			},
			CliStepDefinition::PlanPublishRateLimits {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			},
		],
		dry_run: false,
	};
	save_prepared_release_execution(
		&root,
		&configuration,
		&sample_prepared_release(),
		&[],
		Some(artifact_path.as_path()),
	)
	.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));

	let output = execute_cli_command_with_options(
		&root,
		&configuration,
		&cli_command,
		ExecuteCliCommandOptions {
			dry_run: true,
			quiet: false,
			show_diff: false,
			inputs: BTreeMap::new(),
			prepared_release_path: Some(artifact_path),
			progress_format: ProgressFormat::Auto,
		},
	)
	.unwrap_or_else(|error| panic!("execute publish-plan command: {error}"));

	assert!(output.contains("reused prepared release artifact"));
}

#[test]
fn execute_cli_command_with_options_rejects_readiness_for_placeholder_publish_plans() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let cli_command = CliCommandDefinition {
		name: "publish-plan".to_string(),
		help_text: Some("plan publish rate limits".to_string()),
		inputs: Vec::new(),
		steps: vec![CliStepDefinition::PlanPublishRateLimits {
			name: None,
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};

	let error = execute_cli_command_with_options(
		tempdir.path(),
		&sample_configuration(tempdir.path()),
		&cli_command,
		ExecuteCliCommandOptions {
			dry_run: true,
			quiet: true,
			show_diff: false,
			inputs: BTreeMap::from([
				("mode".to_string(), vec!["placeholder".to_string()]),
				("readiness".to_string(), vec!["readiness.json".to_string()]),
			]),
			prepared_release_path: None,
			progress_format: ProgressFormat::Auto,
		},
	)
	.expect_err("placeholder publish plans should reject readiness artifacts");

	assert!(
		error
			.to_string()
			.contains("only supported for publish rate-limit plans")
	);
}

#[test]
fn execute_cli_command_with_options_reuses_prepared_release_artifact_for_versions() {
	let root = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
		.unwrap_or_else(|error| panic!("workspace root: {error}"));
	let configuration = sample_configuration(&root);
	let artifact_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let artifact_path = artifact_dir.path().join("prepared-release.json");
	save_prepared_release_execution(
		&root,
		&configuration,
		&sample_prepared_release_with_versions(),
		&[],
		Some(artifact_path.as_path()),
	)
	.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));

	let output = execute_cli_command_with_options(
		&root,
		&configuration,
		&default_cli_command("display-versions"),
		ExecuteCliCommandOptions {
			dry_run: false,
			quiet: false,
			show_diff: false,
			inputs: BTreeMap::from([("format".to_string(), vec!["json".to_string()])]),
			prepared_release_path: Some(artifact_path),
			progress_format: ProgressFormat::Auto,
		},
	)
	.unwrap_or_else(|error| panic!("execute versions command: {error}"));
	let parsed: serde_json::Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("parse versions output: {error}"));

	assert_eq!(parsed["groups"]["sdk"], serde_json::json!("2.0.0"));
	assert_eq!(parsed["packages"]["core"], serde_json::json!("1.2.0"));
	assert_eq!(parsed["packages"]["web"], serde_json::json!("1.2.1"));
}

#[test]
fn execute_cli_command_with_options_reports_invalid_versions_artifacts() {
	let root = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
		.unwrap_or_else(|error| panic!("workspace root: {error}"));
	let configuration = sample_configuration(&root);
	let artifact_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let invalid_artifact_path = artifact_dir.path().join("prepared-release.json");
	fs::write(&invalid_artifact_path, "not json")
		.unwrap_or_else(|error| panic!("write invalid artifact: {error}"));

	let error = execute_cli_command_with_options(
		&root,
		&configuration,
		&default_cli_command("display-versions"),
		ExecuteCliCommandOptions {
			dry_run: false,
			quiet: false,
			show_diff: false,
			inputs: BTreeMap::new(),
			prepared_release_path: Some(invalid_artifact_path),
			progress_format: ProgressFormat::Auto,
		},
	)
	.expect_err("invalid prepared release artifact should fail");

	assert!(
		error
			.to_string()
			.contains("failed to parse prepared release artifact")
	);
}

#[test]
fn execute_cli_command_with_options_reports_invalid_versions_output_formats() {
	let root = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
		.unwrap_or_else(|error| panic!("workspace root: {error}"));
	let configuration = sample_configuration(&root);
	let artifact_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let artifact_path = artifact_dir.path().join("prepared-release.json");
	save_prepared_release_execution(
		&root,
		&configuration,
		&sample_prepared_release_with_versions(),
		&[],
		Some(artifact_path.as_path()),
	)
	.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));

	let error = execute_cli_command_with_options(
		&root,
		&configuration,
		&default_cli_command("display-versions"),
		ExecuteCliCommandOptions {
			dry_run: false,
			quiet: false,
			show_diff: false,
			inputs: BTreeMap::from([("format".to_string(), vec!["yaml".to_string()])]),
			prepared_release_path: Some(artifact_path),
			progress_format: ProgressFormat::Auto,
		},
	)
	.expect_err("unsupported versions output format should fail");

	assert_eq!(
		error.to_string(),
		"config error: unsupported output format `yaml`"
	);
}

#[test]
fn record_skipped_and_failure_helpers_cover_silent_paths() {
	let cli_command = default_cli_command("validate");
	let step = CliStepDefinition::Validate {
		name: Some("validate".to_string()),
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	};
	let mut context = cli_context();
	let mut progress = CliProgressReporter::new(&cli_command, false, true, ProgressFormat::Auto);

	record_skipped_cli_step(&mut context, &step, 0, &mut progress, false);
	report_cli_step_failure(
		&mut progress,
		false,
		0,
		&step,
		Duration::from_millis(1),
		&MonochangeError::Config("boom".to_string()),
	);

	assert!(context.command_logs.is_empty());
}

#[test]
fn render_cli_command_result_includes_release_results_and_changed_files() {
	let cli_command = default_cli_command("prepare-release");
	let mut context = cli_context();
	let mut prepared_release = sample_prepared_release();
	prepared_release.changed_files = vec![PathBuf::from("Cargo.toml")];
	context.prepared_release = Some(prepared_release);
	context.release_manifest_path = Some(PathBuf::from(".monochange/local/prepared-release.json"));
	context.release_results = vec!["published core".to_string()];

	let rendered = render_cli_command_result(&cli_command, &context);

	assert!(rendered.contains("release manifest: .monochange/local/prepared-release.json"));
	assert!(rendered.contains("releases:"));
	assert!(rendered.contains("- published core"));
	assert!(rendered.contains("changed files:"));
	assert!(rendered.contains("- Cargo.toml"));
}

#[test]
fn save_prepared_release_artifact_returns_explicit_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let mut context = cli_context();
	context.prepared_release = Some(sample_prepared_release());

	let error = save_prepared_release_artifact(
		tempdir.path(),
		&sample_configuration(tempdir.path()),
		&context,
		Some(tempdir.path().join("prepared-release.json").as_path()),
	)
	.err()
	.unwrap_or_else(|| panic!("expected explicit artifact save error"));

	assert!(!error.to_string().is_empty());
}

#[test]
fn append_changed_file_lines_returns_early_when_no_files_changed() {
	let mut lines = vec!["start".to_string()];
	append_changed_file_lines(&mut lines, &[]);
	assert_eq!(lines, vec!["start".to_string()]);
}

#[test]
fn step_references_release_file_diffs_detects_all_supported_locations() {
	let from_when = CliStepDefinition::Validate {
		name: Some("validate".to_string()),
		when: Some("{{ file_diffs }}".to_string()),
		always_run: false,
		inputs: BTreeMap::new(),
	};
	assert!(step_references_release_file_diffs(&from_when));

	let mut inputs = BTreeMap::new();
	inputs.insert(
		"paths".to_string(),
		CliStepInputValue::List(vec!["{{ file_diffs }}".to_string()]),
	);
	let from_inputs = CliStepDefinition::PublishRelease {
		name: Some("publish".to_string()),
		when: None,
		always_run: false,
		inputs,
	};
	assert!(step_references_release_file_diffs(&from_inputs));

	let from_variables = CliStepDefinition::Command {
		name: Some("command".to_string()),
		when: None,
		always_run: false,
		command: "echo done".to_string(),
		dry_run_command: None,
		show_progress: None,
		shell: ShellConfig::Default,
		id: None,
		variables: Some(BTreeMap::from([(
			"file_diffs_payload".to_string(),
			CommandVariable::ChangedFiles,
		)])),
		inputs: BTreeMap::new(),
	};
	assert!(step_references_release_file_diffs(&from_variables));

	let without_file_diffs = CliStepDefinition::Command {
		name: Some("command".to_string()),
		when: None,
		always_run: false,
		command: "echo done".to_string(),
		dry_run_command: None,
		show_progress: None,
		shell: ShellConfig::Default,
		id: None,
		variables: None,
		inputs: BTreeMap::from([("confirmed".to_string(), CliStepInputValue::Boolean(true))]),
	};
	assert!(!step_references_release_file_diffs(&without_file_diffs));
}

#[test]
fn render_cli_command_result_and_markdown_cover_empty_and_fallback_paths() {
	let cli_command = default_cli_command("prepare-release");
	let mut context = cli_context();
	context.command_logs = vec!["ran command".to_string()];
	let text = render_cli_command_result(&cli_command, &context);
	assert!(text.contains("commands:"));
	assert!(!text.contains("changed files:"));

	let markdown = render_cli_command_markdown_result(&cli_command, &context);
	assert_eq!(markdown, text);
}

#[test]
fn render_cli_command_result_and_markdown_include_release_target_details_without_diffs() {
	let cli_command = default_cli_command("prepare-release");
	let mut context = cli_context();
	context.prepared_release = Some(PreparedRelease {
		plan: ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["core".to_string(), "utils".to_string()],
		version: Some("1.2.3".to_string()),
		group_version: None,
		release_targets: vec![ReleaseTarget {
			id: "core".to_string(),
			kind: ReleaseOwnerKind::Package,
			version: "1.2.3".to_string(),
			tag: true,
			release: false,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.3".to_string(),
			members: Vec::new(),
			rendered_title: "core v1.2.3".to_string(),
			rendered_changelog_title: "core v1.2.3".to_string(),
		}],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		package_publications: Vec::new(),
		dry_run: true,
	});
	context.changeset_policy_evaluation = Some(ChangesetPolicyEvaluation {
		enforce: false,
		required: true,
		status: ChangesetPolicyStatus::Skipped,
		summary: "skip label matched".to_string(),
		comment: None,
		labels: vec!["docs-only".to_string()],
		matched_skip_labels: vec!["docs-only".to_string()],
		changed_paths: vec!["docs/readme.md".to_string()],
		matched_paths: Vec::new(),
		ignored_paths: Vec::new(),
		changeset_paths: Vec::new(),
		affected_package_ids: Vec::new(),
		covered_package_ids: Vec::new(),
		uncovered_package_ids: Vec::new(),
		errors: Vec::new(),
	});

	let text = render_cli_command_result(&cli_command, &context);
	assert!(text.contains("release targets:"));
	assert!(text.contains("tag: true, release: false"));
	assert!(text.contains("changed files:"));
	assert!(!text.contains("file diffs:"));
	assert!(text.contains("matched skip labels: docs-only"));

	let markdown = render_cli_command_markdown_result(&cli_command, &context);
	assert!(markdown.contains("## Release targets"));
	assert!(markdown.contains("tag: yes"));
	assert!(markdown.contains("release: no"));
	assert!(markdown.contains("## Changed files"));
	assert!(!markdown.contains("## Commands"));
}

#[test]
fn markdown_painting_covers_title_subtitle_and_muted_styles() {
	assert!(paint_markdown_inline("title", MarkdownStyle::Title, true).contains("[36;1m"));
	assert!(paint_markdown_inline("subtitle", MarkdownStyle::Subtitle, true).contains("[37;1m"));
	assert!(paint_markdown_inline("muted", MarkdownStyle::Muted, true).contains("[2m"));

	let _env_lock = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	temp_env::with_vars(
		[("NO_COLOR", Some("1")), ("TERM", Some("xterm-256color"))],
		|| {
			assert!(!stdout_supports_color());
		},
	);
}

#[test]
fn publish_rate_limit_selected_package_ids_uses_package_inputs_without_readiness() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = sample_configuration(tempdir.path());
	let inputs = BTreeMap::from([(
		"package".to_string(),
		vec!["core".to_string(), "web".to_string()],
	)]);

	let selected = publish_rate_limit_selected_package_ids(
		tempdir.path(),
		&configuration,
		None,
		&inputs,
		publish_rate_limits::PublishRateLimitMode::Placeholder,
	)
	.unwrap_or_else(|error| panic!("selected packages: {error}"));

	assert_eq!(
		selected,
		BTreeSet::from(["core".to_string(), "web".to_string()])
	);
}

#[test]
fn publish_rate_limit_selected_package_ids_rejects_readiness_for_placeholder_plans() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = sample_configuration(tempdir.path());
	let inputs = BTreeMap::from([(
		"readiness".to_string(),
		vec![".monochange/local/readiness.json".to_string()],
	)]);

	let error = publish_rate_limit_selected_package_ids(
		tempdir.path(),
		&configuration,
		Some(&sample_prepared_release()),
		&inputs,
		publish_rate_limits::PublishRateLimitMode::Placeholder,
	)
	.expect_err("placeholder publish plans should reject readiness artifacts");

	assert!(
		error
			.to_string()
			.contains("only supported for publish rate-limit plans")
	);
}

#[test]
fn publish_rate_limit_selected_package_ids_uses_readiness_artifact_for_publish_plans() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = sample_configuration(tempdir.path());
	let artifact_path = tempdir.path().join("readiness.json");
	let report = publish_readiness::PublishReadinessReport {
		schema_version: 2,
		kind: "monochange.publishReadiness".to_string(),
		status: publish_readiness::PublishReadinessGlobalStatus::Ready,
		from: "prepared-release".to_string(),
		resolved_commit: "prepared-release".to_string(),
		record_commit: "prepared-release".to_string(),
		package_set_fingerprint: String::new(),
		input_fingerprint: "fnv1a64:3a84781749cb9027".to_string(),
		packages: Vec::new(),
	};
	let inputs = BTreeMap::from([(
		"readiness".to_string(),
		vec![artifact_path.display().to_string()],
	)]);
	fs::write(
		&artifact_path,
		serde_json::to_string_pretty(&report)
			.unwrap_or_else(|error| panic!("serialize readiness artifact: {error}")),
	)
	.unwrap_or_else(|error| panic!("write readiness artifact: {error}"));

	let selected = publish_rate_limit_selected_package_ids(
		tempdir.path(),
		&configuration,
		Some(&sample_prepared_release()),
		&inputs,
		publish_rate_limits::PublishRateLimitMode::Publish,
	)
	.unwrap_or_else(|error| panic!("selected packages from readiness: {error}"));

	assert!(selected.is_empty());
}

#[test]
fn optional_publish_plan_readiness_artifact_path_trims_and_rejects_blank_values() {
	let inputs = BTreeMap::from([(
		"readiness".to_string(),
		vec![" .monochange/local/readiness.json ".to_string()],
	)]);
	let path = optional_publish_plan_readiness_artifact_path(&inputs)
		.unwrap_or_else(|error| panic!("readiness artifact path: {error}"));
	assert_eq!(
		path,
		Some(PathBuf::from(".monochange/local/readiness.json"))
	);

	let missing = BTreeMap::new();
	let missing_path = optional_publish_plan_readiness_artifact_path(&missing)
		.unwrap_or_else(|error| panic!("missing readiness artifact path: {error}"));
	assert_eq!(missing_path, None);

	let blank = BTreeMap::from([("readiness".to_string(), vec!["  ".to_string()])]);
	let blank_error = optional_publish_plan_readiness_artifact_path(&blank)
		.expect_err("blank readiness artifact path should fail");
	assert!(blank_error.to_string().contains("mc publish-readiness"));
}

#[test]
fn optional_publish_resume_and_output_paths_trim_and_reject_blank_values() {
	let inputs = BTreeMap::from([
		(
			"resume".to_string(),
			vec![" .monochange/local/previous-result.json ".to_string()],
		),
		(
			"output".to_string(),
			vec![" .monochange/local/publish-result.json ".to_string()],
		),
	]);

	let resume = optional_publish_resume_artifact_path(&inputs)
		.unwrap_or_else(|error| panic!("resume path: {error}"));
	let output = optional_publish_output_artifact_path(&inputs)
		.unwrap_or_else(|error| panic!("output path: {error}"));
	assert_eq!(
		resume,
		Some(PathBuf::from(".monochange/local/previous-result.json"))
	);
	assert_eq!(
		output,
		Some(PathBuf::from(".monochange/local/publish-result.json"))
	);

	let blank = BTreeMap::from([("resume".to_string(), vec!["  ".to_string()])]);
	let error =
		optional_publish_resume_artifact_path(&blank).expect_err("blank resume path should fail");
	assert!(error.to_string().contains("blank `resume` path"));
}

#[test]
fn has_remaining_always_run_steps_detects_always_run_later_in_sequence() {
	let steps = vec![
		CliStepDefinition::Validate {
			name: None,
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::Command {
			show_progress: None,
			name: None,
			when: None,
			always_run: true,
			command: String::new(),
			dry_run_command: None,
			shell: ShellConfig::Default,
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		},
	];
	assert!(has_remaining_always_run_steps(&steps, 0));
	assert!(!has_remaining_always_run_steps(&steps, 1));
}

#[test]
fn selected_group_ids_returns_empty_for_missing_input() {
	let inputs = BTreeMap::new();
	assert!(selected_group_ids(&inputs).is_empty());
}

#[test]
fn selected_group_ids_collects_comma_separated_values() {
	let inputs = BTreeMap::from([(
		"group".to_string(),
		vec!["group-a".to_string(), "group-b".to_string()],
	)]);
	let groups = selected_group_ids(&inputs);
	assert_eq!(
		groups,
		BTreeSet::from(["group-a".to_string(), "group-b".to_string()])
	);
}

#[test]
fn selected_ecosystem_ids_returns_empty_for_missing_input() {
	let inputs = BTreeMap::new();
	assert!(selected_ecosystem_ids(&inputs).unwrap().is_empty());
}

#[test]
fn selected_ecosystem_ids_parses_known_ecosystems() {
	let inputs = BTreeMap::from([(
		"ecosystem".to_string(),
		vec!["npm".to_string(), "cargo".to_string()],
	)]);
	let ecosystems = selected_ecosystem_ids(&inputs).unwrap();
	assert_eq!(
		ecosystems,
		BTreeSet::from([Ecosystem::Npm, Ecosystem::Cargo])
	);
}

#[test]
fn selected_ecosystem_ids_rejects_unknown_ecosystem() {
	let inputs = BTreeMap::from([(
		"ecosystem".to_string(),
		vec!["unknown-ecosystem".to_string()],
	)]);
	let error = selected_ecosystem_ids(&inputs).expect_err("expected unknown ecosystem");
	assert!(error.to_string().contains("unknown ecosystem"), "{error}");
}

#[test]
fn execute_cli_command_always_run_steps_continue_after_failure() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let marker = tempdir.path().join("always-run-marker");
	let skipped_marker = tempdir.path().join("skipped-marker");
	let cli_command = CliCommandDefinition {
		name: "test".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			CliStepDefinition::Command {
				show_progress: None,
				name: Some("fail".to_string()),
				when: None,
				always_run: false,
				command: "exit 1".to_string(),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			},
			CliStepDefinition::Command {
				show_progress: None,
				name: Some("always".to_string()),
				when: None,
				always_run: true,
				command: format!("touch {}", marker.display()),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			},
			CliStepDefinition::Command {
				show_progress: None,
				name: Some("skip".to_string()),
				when: None,
				always_run: false,
				command: format!("touch {}", skipped_marker.display()),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			},
		],
		dry_run: false,
	};

	let result = execute_cli_command_with_options(
		tempdir.path(),
		&sample_configuration(tempdir.path()),
		&cli_command,
		ExecuteCliCommandOptions {
			dry_run: false,
			quiet: true,
			show_diff: false,
			inputs: BTreeMap::new(),
			prepared_release_path: None,
			progress_format: ProgressFormat::Auto,
		},
	);

	assert!(result.is_err());
	assert!(marker.exists(), "always_run step should have executed");
	assert!(
		!skipped_marker.exists(),
		"non-always_run step after failure should be skipped"
	);
}

#[test]
fn execute_cli_command_always_run_continue_after_resolve_step_inputs_failure() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let marker = tempdir.path().join("always-run-marker");
	let cli_command = CliCommandDefinition {
		name: "test".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			CliStepDefinition::Command {
				show_progress: None,
				name: Some("fail".to_string()),
				when: None,
				always_run: false,
				command: "echo hi".to_string(),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::from([(
					"command".to_string(),
					CliStepInputValue::String("{{ bad".to_string()),
				)]),
			},
			CliStepDefinition::Command {
				show_progress: None,
				name: Some("always".to_string()),
				when: None,
				always_run: true,
				command: format!("touch {}", marker.display()),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			},
		],
		dry_run: false,
	};

	let result = execute_cli_command_with_options(
		tempdir.path(),
		&sample_configuration(tempdir.path()),
		&cli_command,
		ExecuteCliCommandOptions {
			dry_run: false,
			quiet: true,
			show_diff: false,
			inputs: BTreeMap::new(),
			prepared_release_path: None,
			progress_format: ProgressFormat::Auto,
		},
	);

	assert!(result.is_err());
	assert!(
		marker.exists(),
		"always_run step should have executed after resolve_step_inputs failure"
	);
}

#[test]
fn execute_cli_command_always_run_continue_after_should_execute_failure() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let marker = tempdir.path().join("always-run-marker");
	let cli_command = CliCommandDefinition {
		name: "test".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			CliStepDefinition::Command {
				show_progress: None,
				name: Some("fail".to_string()),
				when: Some("{{ unknown_var }}".to_string()),
				always_run: false,
				command: "echo hi".to_string(),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			},
			CliStepDefinition::Command {
				show_progress: None,
				name: Some("always".to_string()),
				when: None,
				always_run: true,
				command: format!("touch {}", marker.display()),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			},
		],
		dry_run: false,
	};

	let result = execute_cli_command_with_options(
		tempdir.path(),
		&sample_configuration(tempdir.path()),
		&cli_command,
		ExecuteCliCommandOptions {
			dry_run: false,
			quiet: true,
			show_diff: false,
			inputs: BTreeMap::new(),
			prepared_release_path: None,
			progress_format: ProgressFormat::Auto,
		},
	);

	assert!(result.is_err());
	assert!(
		marker.exists(),
		"always_run step should have executed after should_execute failure"
	);
}
