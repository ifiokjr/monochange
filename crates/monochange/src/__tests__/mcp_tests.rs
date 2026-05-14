use std::fs;
use std::path::PathBuf;

use insta::assert_snapshot;
use monochange_analysis::ChangeAnalysis;
use monochange_analysis::ChangeFrame;
use monochange_analysis::DetectionLevel;
use monochange_analysis::PackageChangeAnalysis;
use monochange_config::LoadedChangesetFile;
use monochange_core::ChangeSignal;
use monochange_core::Ecosystem;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;
use monochange_test_helpers::content_text;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::current_test_name;
use monochange_test_helpers::git;
use monochange_test_helpers::snapshot_settings;
use rmcp::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use tempfile::tempdir;

use super::AffectedParam;
use super::ChangeParam;
use super::DiagnosticsParam;
use super::EmptyParam;
use super::LintExplainParam;
use super::McpChangeBump;
use super::MonochangeMcpServer;
use super::PathParam;
use super::json_error_result;
use super::json_result;
use super::parse_frame;
use super::resolve_root;

macro_rules! assert_readable_json_snapshot {
	($value:expr) => {{
		let (redacted, multiline_fields) = redact_multiline_strings(&$value);
		insta::assert_json_snapshot!(redacted);
		for (path, contents) in multiline_fields {
			insta::assert_snapshot!(format!("multiline_{}", snapshot_path_slug(&path)), contents);
		}
	}};
}

fn redact_multiline_strings(
	value: &serde_json::Value,
) -> (serde_json::Value, Vec<(String, String)>) {
	let mut redacted = value.clone();
	let mut multiline_fields = Vec::new();
	redact_multiline_strings_at(&mut redacted, "$", &mut multiline_fields);
	(redacted, multiline_fields)
}

fn snapshot_path_slug(path: &str) -> String {
	path.chars()
		.map(|character| {
			match character {
				'a'..='z' | 'A'..='Z' | '0'..='9' => character.to_ascii_lowercase(),
				_ => '_',
			}
		})
		.collect::<String>()
		.trim_matches('_')
		.to_owned()
}

fn redact_multiline_strings_at(
	value: &mut serde_json::Value,
	path: &str,
	multiline_fields: &mut Vec<(String, String)>,
) {
	match value {
		serde_json::Value::String(contents) if contents.contains('\n') => {
			multiline_fields.push((path.to_owned(), contents.clone()));
			*contents = "[multiline text]".to_owned();
		}
		serde_json::Value::Array(items) => {
			for (index, item) in items.iter_mut().enumerate() {
				redact_multiline_strings_at(item, &format!("{path}[{index}]"), multiline_fields);
			}
		}
		serde_json::Value::Object(fields) => {
			for (key, field) in fields {
				redact_multiline_strings_at(field, &format!("{path}.{key}"), multiline_fields);
			}
		}
		_ => {}
	}
}

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_scenario_workspace(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[test]
fn parse_frame_supports_named_range_and_pull_request_inputs() {
	assert_eq!(parse_frame("working"), ChangeFrame::WorkingDirectory);
	assert_eq!(
		parse_frame("main...feature/indexing"),
		ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature/indexing".to_string(),
		}
	);
	assert_eq!(
		parse_frame("pr:main,fix/indexing"),
		ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "fix/indexing".to_string(),
		}
	);
}

fn setup_analysis_workspace() -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("analysis/cargo-public-api-diff/before"),
		tempdir.path(),
	);
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);
	copy_directory(
		&fixture_path("analysis/cargo-public-api-diff/after"),
		tempdir.path(),
	);
	tempdir
}

fn setup_analysis_workspace_without_git() -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("analysis/cargo-public-api-diff/after"),
		tempdir.path(),
	);
	tempdir
}

fn setup_analysis_workspace_without_commits() -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("analysis/cargo-public-api-diff/after"),
		tempdir.path(),
	);
	git(tempdir.path(), &["init"]);
	tempdir
}

fn sample_package_analysis(
	package_id: &str,
	semantic_changes: Vec<SemanticChange>,
) -> PackageChangeAnalysis {
	PackageChangeAnalysis {
		package_id: package_id.to_string(),
		package_record_id: package_id.to_string(),
		package_name: package_id.to_string(),
		ecosystem: Ecosystem::Cargo,
		analyzer_id: Some("test".to_string()),
		changed_files: vec![PathBuf::from("src/lib.rs")],
		semantic_changes,
		warnings: Vec::new(),
	}
}

fn sample_change_analysis(package_analysis: PackageChangeAnalysis) -> ChangeAnalysis {
	let mut analyses_by_package = std::collections::BTreeMap::new();
	analyses_by_package.insert(package_analysis.package_id.clone(), package_analysis);
	ChangeAnalysis {
		frame: ChangeFrame::WorkingDirectory,
		detection_level: DetectionLevel::Signature,
		package_analyses: analyses_by_package,
		warnings: Vec::new(),
	}
}

fn sample_changeset(summary: Option<&str>, package_id: &str) -> LoadedChangesetFile {
	LoadedChangesetFile {
		path: PathBuf::from(".changeset/test.md"),
		summary: summary.map(ToString::to_string),
		details: None,
		targets: Vec::new(),
		signals: vec![ChangeSignal {
			package_id: package_id.to_string(),
			requested_bump: None,
			explicit_version: None,
			change_origin: "test".to_string(),
			evidence_refs: Vec::new(),
			notes: None,
			details: None,
			change_type: None,
			caused_by: Vec::new(),
			source_path: PathBuf::from(".changeset/test.md"),
		}],
	}
}

fn sample_semantic_change(item_path: &str) -> SemanticChange {
	SemanticChange {
		category: SemanticChangeCategory::PublicApi,
		kind: SemanticChangeKind::Modified,
		item_kind: "function".to_string(),
		item_path: item_path.to_string(),
		summary: format!("function `{item_path}` modified"),
		file_path: PathBuf::from("src/lib.rs"),
		before_signature: Some(format!("fn {item_path}()")),
		after_signature: Some(format!("fn {item_path}(arg: &str)")),
	}
}

#[test]
fn shared_fs_test_support_helpers_cover_names_and_scenario_copying() {
	assert_eq!(
		current_test_name(),
		"shared_fs_test_support_helpers_cover_names_and_scenario_copying"
	);
	let named = std::thread::Builder::new()
		.name("case_1_mcp_helper_thread".to_string())
		.spawn(current_test_name)
		.unwrap_or_else(|error| panic!("spawn thread: {error}"))
		.join()
		.unwrap_or_else(|error| panic!("join thread: {error:?}"));
	assert_eq!(named, "mcp_helper_thread");
	let scenario = setup_scenario_workspace("test-support/scenario-workspace");
	assert_eq!(
		fs::read_to_string(scenario.path().join("workspace-only.txt"))
			.unwrap_or_else(|error| panic!("read scenario: {error}")),
		"workspace marker\n"
	);
	assert!(!scenario.path().join("expected").exists());
}

#[test]
fn get_info_exposes_tool_instructions_and_capabilities() {
	let info = MonochangeMcpServer::new().get_info();
	assert!(info.instructions.as_ref().is_some_and(|text| {
		text.contains(
			"monochange manages versions and releases across Cargo, npm, Deno, and Dart/Flutter workspaces",
		)
	}));
	assert!(info.capabilities.tools.is_some());
}

#[test]
fn resolve_root_prefers_explicit_paths() {
	let explicit = PathBuf::from("/tmp/monochange-mcp-test");
	assert_eq!(resolve_root(explicit.to_str()), explicit);
}

#[test]
fn resolve_root_defaults_to_current_directory() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let original = std::env::current_dir().unwrap_or_else(|error| panic!("current dir: {error}"));
	std::env::set_current_dir(tempdir.path())
		.unwrap_or_else(|error| panic!("set current dir: {error}"));
	let resolved = resolve_root(None);
	std::env::set_current_dir(&original)
		.unwrap_or_else(|error| panic!("restore current dir: {error}"));
	assert_eq!(
		monochange_core::normalize_path(&resolved),
		monochange_core::normalize_path(tempdir.path())
	);
}

#[test]
fn json_result_and_error_result_render_structured_content() {
	let success = json_result(serde_json::json!({"ok": true, "summary": "done"}));
	let failure = json_error_result(serde_json::json!({"ok": false, "summary": "bad"}));
	assert!(success.is_error.is_none_or(|value| !value));
	assert_eq!(
		content_text(&success),
		"{\n  \"ok\": true,\n  \"summary\": \"done\"\n}"
	);
	assert_eq!(failure.is_error, Some(true));
	assert_eq!(
		content_text(&failure),
		"{\n  \"ok\": false,\n  \"summary\": \"bad\"\n}"
	);
}

#[test]
fn content_text_returns_empty_for_non_text_content() {
	let result = rmcp::model::CallToolResult::success(vec![rmcp::model::Content::new(
		rmcp::model::RawContent::image("aGVsbG8=", "image/png"),
		None,
	)]);
	assert_eq!(content_text(&result), String::new());
}

#[test]
fn manifest_helpers_expose_release_preview_shapes() {
	let tempdir = setup_fixture("monochange/release-base");
	let prepared =
		crate::cli_runtime::block_on_in_context(crate::prepare_release(tempdir.path(), true))
			.unwrap_or_else(|error| panic!("prepare release: {error}"));
	let manifest = super::manifest_for_prepared_release(&prepared);
	assert_eq!(manifest.command, "release-manifest");
	assert!(!manifest.release_targets.is_empty());
	let prepared_value = super::prepared_release_value(&prepared);
	assert_eq!(prepared_value["dry_run"].as_bool(), Some(true));
	assert!(prepared_value["release_targets"].is_array());
}

#[test]
fn copy_directory_panics_when_destination_is_not_a_directory() {
	let source = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(source.path().join("file.txt"), "hello\n")
		.unwrap_or_else(|error| panic!("write source file: {error}"));
	let destination_root = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let destination_file = destination_root.path().join("already-a-file");
	fs::write(&destination_file, "occupied\n")
		.unwrap_or_else(|error| panic!("write destination file: {error}"));
	let result = std::panic::catch_unwind(|| copy_directory(source.path(), &destination_file));
	assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_reports_success_for_valid_workspace_fixture() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/release-base");

	let result = MonochangeMcpServer::new()
		.validate(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_reports_config_errors_for_invalid_workspace_fixture() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("config/rejects-unknown-template-vars");

	let result = MonochangeMcpServer::new()
		.validate(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn discover_reports_workspace_packages() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/release-base");

	let result = MonochangeMcpServer::new()
		.discover(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("discover: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn change_writes_markdown_changeset() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/release-base");

	let result = MonochangeMcpServer::new()
		.change(Parameters(ChangeParam {
			path: Some(tempdir.path().display().to_string()),
			package: vec!["core".to_string()],
			bump: McpChangeBump::Patch,
			version: None,
			reason: "add test coverage".to_string(),
			change_type: None,
			caused_by: Vec::new(),
			details: None,
			output: Some(
				tempdir
					.path()
					.join(".changeset/core.md")
					.display()
					.to_string(),
			),
		}))
		.await
		.unwrap_or_else(|error| panic!("change: {error}"));

	assert_snapshot!("response", content_text(&result));
	let contents = fs::read_to_string(tempdir.path().join(".changeset/core.md"))
		.unwrap_or_else(|error| panic!("changeset read: {error}"));
	assert_snapshot!("changeset", contents);
}

#[test]
fn mcp_change_bump_variants_map_to_change_bump_variants() {
	assert_eq!(
		crate::ChangeBump::from(McpChangeBump::None),
		crate::ChangeBump::None
	);
	assert_eq!(
		crate::ChangeBump::from(McpChangeBump::Patch),
		crate::ChangeBump::Patch
	);
	assert_eq!(
		crate::ChangeBump::from(McpChangeBump::Minor),
		crate::ChangeBump::Minor
	);
	assert_eq!(
		crate::ChangeBump::from(McpChangeBump::Major),
		crate::ChangeBump::Major
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn change_writes_type_only_markdown_changeset() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("changeset-target-metadata/cli-type-only-change");

	let result = MonochangeMcpServer::new()
		.change(Parameters(ChangeParam {
			path: Some(tempdir.path().display().to_string()),
			package: vec!["core".to_string()],
			bump: McpChangeBump::None,
			version: None,
			reason: "document the migration".to_string(),
			change_type: Some("docs".to_string()),
			caused_by: Vec::new(),
			details: None,
			output: Some(
				tempdir
					.path()
					.join(".changeset/core-docs.md")
					.display()
					.to_string(),
			),
		}))
		.await
		.unwrap_or_else(|error| panic!("change: {error}"));

	assert_snapshot!("response", content_text(&result));
	let contents = fs::read_to_string(tempdir.path().join(".changeset/core-docs.md"))
		.unwrap_or_else(|error| panic!("changeset read: {error}"));
	assert_snapshot!("changeset", contents);
}

#[tokio::test(flavor = "multi_thread")]
async fn discover_reports_config_errors_for_invalid_workspace_fixture() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("config/rejects-unknown-template-vars");

	let result = MonochangeMcpServer::new()
		.discover(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("discover: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn diagnostics_returns_changeset_context() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/diagnostics-base");

	let result = MonochangeMcpServer::new()
		.diagnostics(Parameters(DiagnosticsParam {
			path: Some(tempdir.path().display().to_string()),
			changeset: Vec::new(),
		}))
		.await
		.unwrap_or_else(|error| panic!("diagnostics: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn diagnostics_reports_missing_requested_changesets() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/diagnostics-base");

	let result = MonochangeMcpServer::new()
		.diagnostics(Parameters(DiagnosticsParam {
			path: Some(tempdir.path().display().to_string()),
			changeset: vec!["missing.md".to_string()],
		}))
		.await
		.unwrap_or_else(|error| panic!("diagnostics: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn change_reports_errors_for_unknown_package_ids() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/release-base");

	let result = MonochangeMcpServer::new()
		.change(Parameters(ChangeParam {
			path: Some(tempdir.path().display().to_string()),
			package: vec!["missing".to_string()],
			bump: McpChangeBump::Patch,
			version: None,
			reason: "missing package".to_string(),
			change_type: None,
			caused_by: Vec::new(),
			details: None,
			output: None,
		}))
		.await
		.unwrap_or_else(|error| panic!("change: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn release_preview_returns_dry_run_release_summary() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/release-base");

	let result = MonochangeMcpServer::new()
		.release_preview(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("release preview: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn release_manifest_returns_dry_run_manifest() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/release-base");

	let result = MonochangeMcpServer::new()
		.release_manifest(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("release manifest: {error}"));

	let value = serde_json::from_str::<serde_json::Value>(&content_text(&result))
		.unwrap_or_else(|error| panic!("parse release manifest result: {error}"));
	assert_readable_json_snapshot!(value);
}

#[tokio::test(flavor = "multi_thread")]
async fn release_preview_reports_config_errors_when_changesets_are_missing() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("cli-output/ungrouped-no-changeset");

	let result = MonochangeMcpServer::new()
		.release_preview(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("release preview: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn release_manifest_reports_config_errors_when_changesets_are_missing() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("cli-output/ungrouped-no-changeset");

	let result = MonochangeMcpServer::new()
		.release_manifest(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("release manifest: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_reports_failed_policy_for_uncovered_changes() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("changeset-policy/no-changeset");

	let result = MonochangeMcpServer::new()
		.affected_packages(Parameters(AffectedParam {
			path: Some(tempdir.path().display().to_string()),
			changed_paths: vec!["crates/core/src/lib.rs".to_string()],
			labels: Vec::new(),
		}))
		.await
		.unwrap_or_else(|error| panic!("affected: {error}"));

	let value = serde_json::from_str::<serde_json::Value>(&content_text(&result))
		.unwrap_or_else(|error| panic!("parse affected result: {error}"));
	assert_readable_json_snapshot!(value);
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_reports_success_for_documentation_only_changes() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("cli-step-input-overrides/workspace");

	let result = MonochangeMcpServer::new()
		.affected_packages(Parameters(AffectedParam {
			path: Some(tempdir.path().display().to_string()),
			changed_paths: vec!["docs/readme.md".to_string()],
			labels: vec!["no-changeset-required".to_string()],
		}))
		.await
		.unwrap_or_else(|error| panic!("affected: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_reports_configuration_errors() {
	let tempdir = setup_fixture("config/rejects-empty-step-id");
	let result = MonochangeMcpServer::new()
		.affected_packages(Parameters(AffectedParam {
			path: Some(tempdir.path().display().to_string()),
			changed_paths: vec!["crates/core/src/lib.rs".to_string()],
			labels: Vec::new(),
		}))
		.await
		.unwrap_or_else(|error| panic!("affected: {error}"));
	let rendered = content_text(&result);
	assert!(rendered.contains("\"ok\": false"));
	assert!(
		rendered.contains("unknown") || rendered.contains("empty id") || rendered.contains("step")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn lint_catalog_lists_rules_and_presets() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let result = MonochangeMcpServer::new()
		.lint_catalog(Parameters(EmptyParam::default()))
		.await
		.unwrap_or_else(|error| panic!("lint catalog: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn lint_explain_returns_rule_and_preset() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let server = MonochangeMcpServer::new();
	let rule = server
		.lint_explain(Parameters(LintExplainParam {
			id: "cargo/internal-dependency-workspace".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("lint explain rule: {error}"));
	let preset = server
		.lint_explain(Parameters(LintExplainParam {
			id: "cargo/recommended".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("lint explain preset: {error}"));

	let combined = serde_json::json!({
		"rule": serde_json::from_str::<serde_json::Value>(&content_text(&rule))
			.unwrap_or_else(|error| panic!("parse rule result: {error}")),
		"preset": serde_json::from_str::<serde_json::Value>(&content_text(&preset))
			.unwrap_or_else(|error| panic!("parse preset result: {error}")),
	});

	assert_readable_json_snapshot!(combined);
}

#[tokio::test(flavor = "multi_thread")]
async fn lint_explain_reports_unknown_ids() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let result = MonochangeMcpServer::new()
		.lint_explain(Parameters(LintExplainParam {
			id: "cargo/does-not-exist".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("lint explain missing id: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_eval_release_workflow_stays_machine_readable() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/diagnostics-base");
	let server = MonochangeMcpServer::new();

	let discover = server
		.discover(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("discover: {error}"));
	let diagnostics = server
		.diagnostics(Parameters(DiagnosticsParam {
			path: Some(tempdir.path().display().to_string()),
			changeset: Vec::new(),
		}))
		.await
		.unwrap_or_else(|error| panic!("diagnostics: {error}"));
	let manifest = server
		.release_manifest(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("release manifest: {error}"));

	let eval = serde_json::json!({
		"discover": serde_json::from_str::<serde_json::Value>(&content_text(&discover))
			.unwrap_or_else(|error| panic!("parse discover result: {error}")),
		"diagnostics": serde_json::from_str::<serde_json::Value>(&content_text(&diagnostics))
			.unwrap_or_else(|error| panic!("parse diagnostics result: {error}")),
		"releaseManifest": serde_json::from_str::<serde_json::Value>(&content_text(&manifest))
			.unwrap_or_else(|error| panic!("parse release manifest result: {error}")),
	});

	assert_readable_json_snapshot!(eval);
}

#[tokio::test(flavor = "multi_thread")]
async fn analyze_changes_returns_semantic_diff_for_cargo_workspace() {
	let tempdir = setup_analysis_workspace();
	let result = MonochangeMcpServer::new()
		.analyze_changes(Parameters(super::AnalyzeChangesParam {
			path: Some(tempdir.path().display().to_string()),
			frame: Some("working".to_string()),
			detection_level: Some("signature".to_string()),
			max_suggestions: None,
		}))
		.await
		.unwrap_or_else(|error| panic!("analyze_changes: {error}"));

	snapshot_settings().bind(|| {
		assert_snapshot!(content_text(&result));
	});
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_changeset_reports_stale_symbol_references() {
	let tempdir = setup_analysis_workspace();
	let changeset_dir = tempdir.path().join(".changeset");
	fs::create_dir_all(&changeset_dir)
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::copy(
		fixture_path("analysis/cargo-public-api-diff/changeset-stale.md"),
		changeset_dir.join("feature.md"),
	)
	.unwrap_or_else(|error| panic!("copy changeset fixture: {error}"));

	let result = MonochangeMcpServer::new()
		.validate_changeset(Parameters(super::ValidateChangesetParam {
			path: Some(tempdir.path().display().to_string()),
			changeset_path: ".changeset/feature.md".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate_changeset: {error}"));

	snapshot_settings().bind(|| {
		assert_snapshot!(content_text(&result));
	});
}

#[test]
fn validate_changeset_content_reports_missing_package_diffs() {
	let changeset = sample_changeset(Some("mention `greet`"), "core");
	let analysis = ChangeAnalysis {
		frame: ChangeFrame::WorkingDirectory,
		detection_level: DetectionLevel::Signature,
		package_analyses: std::collections::BTreeMap::new(),
		warnings: Vec::new(),
	};

	let issues = super::validate_changeset_content(&changeset, &analysis);

	assert_eq!(issues.len(), 2);
	assert!(
		issues
			.iter()
			.any(|issue| issue.message.contains("has no current diff"))
	);
	assert!(
		issues
			.iter()
			.any(|issue| issue.message.contains("references `greet`"))
	);
}

#[test]
fn validate_changeset_content_warns_when_semantic_changes_are_empty() {
	let changeset = sample_changeset(Some("internal cleanup"), "core");
	let analysis = sample_change_analysis(sample_package_analysis("core", Vec::new()));

	let issues = super::validate_changeset_content(&changeset, &analysis);

	assert_eq!(issues.len(), 1);
	assert!(
		issues[0]
			.message
			.contains("no semantic changes were detected")
	);
	assert_eq!(issues[0].severity, "warning");
}

#[test]
fn validate_changeset_content_skips_warning_when_summary_mentions_detected_item() {
	let changeset = sample_changeset(Some("update `greet` behavior"), "core");
	let analysis = sample_change_analysis(sample_package_analysis(
		"core",
		vec![sample_semantic_change("greet")],
	));

	let issues = super::validate_changeset_content(&changeset, &analysis);

	assert!(issues.is_empty());
}

#[test]
fn validate_changeset_content_skips_warning_when_details_mentions_detected_item() {
	let mut changeset = sample_changeset(Some("update public API"), "core");
	changeset.details = Some("The changed item is `greet`.".to_string());
	let analysis = sample_change_analysis(sample_package_analysis(
		"core",
		vec![sample_semantic_change("greet")],
	));

	let issues = super::validate_changeset_content(&changeset, &analysis);

	assert!(issues.is_empty());
}

#[test]
fn validate_changeset_content_suggests_detected_items_when_summary_is_generic() {
	let changeset = sample_changeset(Some("update semantic analysis messaging"), "core");
	let analysis = sample_change_analysis(sample_package_analysis(
		"core",
		vec![
			sample_semantic_change("greet"),
			sample_semantic_change("Greeter"),
		],
	));

	let issues = super::validate_changeset_content(&changeset, &analysis);

	assert_eq!(issues.len(), 1);
	assert!(
		issues[0]
			.message
			.contains("does not mention any detected semantic item")
	);
	assert!(issues[0].suggestion.as_ref().is_some_and(|suggestion| {
		suggestion.contains("`greet`") && suggestion.contains("`Greeter`")
	}));
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_changeset_reports_frame_detection_errors() {
	let tempdir = setup_analysis_workspace_without_git();
	let changeset_dir = tempdir.path().join(".changeset");
	fs::create_dir_all(&changeset_dir)
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::write(
		changeset_dir.join("feature.md"),
		"---\ncore: patch\n---\n\n#### test change\n",
	)
	.unwrap_or_else(|error| panic!("write changeset fixture: {error}"));

	let result = MonochangeMcpServer::new()
		.validate_changeset(Parameters(super::ValidateChangesetParam {
			path: Some(tempdir.path().display().to_string()),
			changeset_path: ".changeset/feature.md".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate_changeset: {error}"));
	let rendered = content_text(&result);

	assert!(rendered.contains("Failed to detect change frame"));
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_changeset_reports_semantic_analysis_errors() {
	let tempdir = setup_analysis_workspace_without_commits();
	let changeset_dir = tempdir.path().join(".changeset");
	fs::create_dir_all(&changeset_dir)
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::write(
		changeset_dir.join("feature.md"),
		"---\ncore: patch\n---\n\n#### test change\n",
	)
	.unwrap_or_else(|error| panic!("write changeset fixture: {error}"));

	let result = MonochangeMcpServer::new()
		.validate_changeset(Parameters(super::ValidateChangesetParam {
			path: Some(tempdir.path().display().to_string()),
			changeset_path: ".changeset/feature.md".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate_changeset: {error}"));
	let rendered = content_text(&result);

	assert!(rendered.contains("Semantic analysis failed"));
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_changeset_reports_current_lifecycle_when_items_are_mentioned() {
	let tempdir = setup_analysis_workspace();
	let changeset_dir = tempdir.path().join(".changeset");
	fs::create_dir_all(&changeset_dir)
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::write(
		changeset_dir.join("feature.md"),
		"---\ncore: patch\n---\n\n#### update `Greeter` behavior\n",
	)
	.unwrap_or_else(|error| panic!("write changeset fixture: {error}"));

	let result = MonochangeMcpServer::new()
		.validate_changeset(Parameters(super::ValidateChangesetParam {
			path: Some(tempdir.path().display().to_string()),
			changeset_path: ".changeset/feature.md".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate_changeset: {error}"));
	let rendered = content_text(&result);

	assert!(rendered.contains("\"lifecycle_status\": \"current\""));
	assert!(rendered.contains("\"valid\": true"));
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_changeset_reports_incomplete_lifecycle_for_warning_only_results() {
	let tempdir = setup_analysis_workspace();
	let changeset_dir = tempdir.path().join(".changeset");
	fs::create_dir_all(&changeset_dir)
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::write(
		changeset_dir.join("feature.md"),
		"---\ncore: patch\n---\n\n#### update semantic analysis messaging\n",
	)
	.unwrap_or_else(|error| panic!("write changeset fixture: {error}"));

	let result = MonochangeMcpServer::new()
		.validate_changeset(Parameters(super::ValidateChangesetParam {
			path: Some(tempdir.path().display().to_string()),
			changeset_path: ".changeset/feature.md".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate_changeset: {error}"));
	let rendered = content_text(&result);

	assert!(rendered.contains("\"lifecycle_status\": \"incomplete\""));
	assert!(rendered.contains("\"valid\": false"));
}
