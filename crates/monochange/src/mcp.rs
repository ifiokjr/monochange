use std::path::Path;
use std::path::PathBuf;

use monochange_config::validate_workspace;
use monochange_core::CliCommandDefinition;
use monochange_core::ReleaseManifest;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::schemars;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;
use rmcp::ErrorData as McpError;
use rmcp::ServerHandler;
use rmcp::ServiceExt;
use serde::Deserialize;
use serde_json::json;

use crate::ChangeBump;
use crate::PreparedRelease;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PathParam {
	pub path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ChangeParam {
	pub path: Option<String>,
	pub package: Vec<String>,
	pub bump: McpChangeBump,
	pub version: Option<String>,
	pub reason: String,
	#[serde(rename = "type")]
	pub change_type: Option<String>,
	pub details: Option<String>,
	pub output: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AffectedParam {
	pub path: Option<String>,
	pub changed_paths: Vec<String>,
	#[serde(default)]
	pub labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpChangeBump {
	None,
	Patch,
	Minor,
	Major,
}

impl From<McpChangeBump> for ChangeBump {
	fn from(value: McpChangeBump) -> Self {
		match value {
			McpChangeBump::None => Self::None,
			McpChangeBump::Patch => Self::Patch,
			McpChangeBump::Minor => Self::Minor,
			McpChangeBump::Major => Self::Major,
		}
	}
}

#[derive(Debug, Clone)]
pub struct MonochangeMcpServer {
	tool_router: ToolRouter<Self>,
}

#[tool_handler]
impl ServerHandler for MonochangeMcpServer {
	fn get_info(&self) -> ServerInfo {
		let mut info = ServerInfo::default();
		info.instructions = Some(
			"monochange manages versions and releases across Cargo, npm, Deno, and Dart/Flutter \
			 workspaces. Prefer validation and dry-run planning before mutating release state. \
			 Read monochange.toml first, inspect the normalized model with discover, use change \
			 to write explicit .changeset files, and use release preview or release manifest \
			 tools before source-provider publishing."
				.into(),
		);
		info.capabilities = ServerCapabilities::builder().enable_tools().build();
		info
	}
}

fn resolve_root(path: Option<&str>) -> PathBuf {
	path.map_or_else(
		|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
		PathBuf::from,
	)
}

fn json_result(value: serde_json::Value) -> CallToolResult {
	let text = serde_json::to_string_pretty(&value)
		.unwrap_or_else(|_| "{\"ok\":false,\"summary\":\"failed to serialize\"}".to_string());
	let mut result = CallToolResult::success(vec![Content::text(text)]);
	result.structured_content = Some(value);
	result
}

fn json_error_result(value: serde_json::Value) -> CallToolResult {
	let text = serde_json::to_string_pretty(&value)
		.unwrap_or_else(|_| "{\"ok\":false,\"summary\":\"failed to serialize\"}".to_string());
	let mut result = CallToolResult::error(vec![Content::text(text)]);
	result.structured_content = Some(value);
	result
}

fn manifest_for_prepared_release(prepared_release: &PreparedRelease) -> ReleaseManifest {
	let cli_command = CliCommandDefinition {
		name: "release-manifest".to_string(),
		help_text: Some("Render a release manifest for MCP consumers".to_string()),
		inputs: Vec::new(),
		steps: Vec::new(),
	};
	super::build_release_manifest(&cli_command, prepared_release, &[])
}

fn prepared_release_value(prepared_release: &PreparedRelease) -> serde_json::Value {
	json!({
		"dry_run": prepared_release.dry_run,
		"version": prepared_release.version,
		"group_version": prepared_release.group_version,
		"released_packages": prepared_release.released_packages,
		"release_targets": prepared_release.release_targets,
		"changed_files": prepared_release.changed_files,
		"updated_changelogs": prepared_release.updated_changelogs,
		"deleted_changesets": prepared_release.deleted_changesets,
		"changesets": prepared_release.changesets,
		"plan": prepared_release.plan,
	})
}

#[tool_router]
impl MonochangeMcpServer {
	pub fn new() -> Self {
		Self {
			tool_router: Self::tool_router(),
		}
	}

	#[tool(
		name = "monochange_validate",
		description = "Validate monochange.toml and .changeset targets for a repository."
	)]
	async fn validate(
		&self,
		Parameters(params): Parameters<PathParam>,
	) -> Result<CallToolResult, McpError> {
		let root = resolve_root(params.path.as_deref());
		match validate_workspace(&root) {
			Ok(()) => Ok(json_result(json!({
				"ok": true,
				"action": "validate",
				"root": root,
				"summary": "workspace validation passed"
			}))),
			Err(error) => Ok(json_error_result(json!({
				"ok": false,
				"action": "validate",
				"root": root,
				"summary": error.render(),
				"error": error.render()
			}))),
		}
	}

	#[tool(
		name = "monochange_discover",
		description = "Discover packages, dependencies, and groups across the repository."
	)]
	async fn discover(
		&self,
		Parameters(params): Parameters<PathParam>,
	) -> Result<CallToolResult, McpError> {
		let root = resolve_root(params.path.as_deref());
		match crate::discover_workspace(&root) {
			Ok(report) => Ok(json_result(json!({
				"ok": true,
				"action": "discover",
				"summary": format!(
					"Discovered {} package(s) and {} dependency edge(s).",
					report.packages.len(),
					report.dependencies.len()
				),
				"report": report,
			}))),
			Err(error) => Ok(json_error_result(json!({
				"ok": false,
				"action": "discover",
				"root": root,
				"summary": error.render(),
				"error": error.render()
			}))),
		}
	}

	#[tool(
		name = "monochange_change",
		description = "Write a .changeset markdown file for one or more package or group ids."
	)]
	async fn change(
		&self,
		Parameters(params): Parameters<ChangeParam>,
	) -> Result<CallToolResult, McpError> {
		let root = resolve_root(params.path.as_deref());
		let output = params.output.as_deref().map(Path::new);
		let bump = ChangeBump::from(params.bump);
		match crate::add_change_file(
			&root,
			crate::AddChangeFileRequest::builder()
				.package_refs(&params.package)
				.bump(bump.into())
				.reason(&params.reason)
				.version(params.version.as_deref())
				.change_type(params.change_type.as_deref())
				.details(params.details.as_deref())
				.output(output)
				.build(),
		) {
			Ok(path) => Ok(json_result(json!({
				"ok": true,
				"action": "change",
				"root": root,
				"path": path,
				"summary": format!("Wrote change file {}", path.display())
			}))),
			Err(error) => Ok(json_error_result(json!({
				"ok": false,
				"action": "change",
				"root": root,
				"summary": error.render(),
				"error": error.render()
			}))),
		}
	}

	#[tool(
		name = "monochange_release_preview",
		description = "Prepare a dry-run release preview from discovered .changeset files."
	)]
	async fn release_preview(
		&self,
		Parameters(params): Parameters<PathParam>,
	) -> Result<CallToolResult, McpError> {
		let root = resolve_root(params.path.as_deref());
		match crate::prepare_release(&root, true) {
			Ok(prepared_release) => Ok(json_result(json!({
				"ok": true,
				"action": "release_preview",
				"summary": format!(
					"Prepared dry-run release preview with {} release target(s).",
					prepared_release.release_targets.len()
				),
				"release": prepared_release_value(&prepared_release)
			}))),
			Err(error) => Ok(json_error_result(json!({
				"ok": false,
				"action": "release_preview",
				"root": root,
				"summary": error.render(),
				"error": error.render()
			}))),
		}
	}

	#[tool(
		name = "monochange_release_manifest",
		description = "Generate a dry-run release manifest JSON document for downstream automation."
	)]
	async fn release_manifest(
		&self,
		Parameters(params): Parameters<PathParam>,
	) -> Result<CallToolResult, McpError> {
		let root = resolve_root(params.path.as_deref());
		match crate::prepare_release(&root, true) {
			Ok(prepared_release) => {
				let manifest = manifest_for_prepared_release(&prepared_release);
				Ok(json_result(json!({
					"ok": true,
					"action": "release_manifest",
					"summary": format!(
						"Generated dry-run release manifest with {} release target(s).",
						manifest.release_targets.len()
					),
					"manifest": manifest,
				})))
			}
			Err(error) => Ok(json_error_result(json!({
				"ok": false,
				"action": "release_manifest",
				"root": root,
				"summary": error.render(),
				"error": error.render()
			}))),
		}
	}

	#[tool(
		name = "monochange_affected_packages",
		description = "Evaluate changeset policy from changed paths and optional labels."
	)]
	async fn affected_packages(
		&self,
		Parameters(params): Parameters<AffectedParam>,
	) -> Result<CallToolResult, McpError> {
		let root = resolve_root(params.path.as_deref());
		match crate::affected_packages(&root, &params.changed_paths, &params.labels) {
			Ok(evaluation) => Ok(json_result(json!({
				"ok": evaluation.status != monochange_core::ChangesetPolicyStatus::Failed,
				"action": "affected_packages",
				"summary": evaluation.summary,
				"evaluation": evaluation,
			}))),
			Err(error) => Ok(json_error_result(json!({
				"ok": false,
				"action": "affected_packages",
				"root": root,
				"summary": error.render(),
				"error": error.render()
			}))),
		}
	}
}

pub async fn run_server() {
	let server = MonochangeMcpServer::new();
	let transport = rmcp::transport::io::stdio();

	match server.serve(transport).await {
		Ok(running) => {
			let _ = running.waiting().await;
		}
		Err(error) => {
			eprintln!("monochange-mcp: failed to start server: {error}");
		}
	}
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod __tests {
	use std::fs;
	use std::path::PathBuf;

	use insta::assert_snapshot;
	use monochange_test_helpers::content_text;
	use monochange_test_helpers::copy_directory;
	use monochange_test_helpers::current_test_name;
	use monochange_test_helpers::snapshot_settings;
	use rmcp::handler::server::wrapper::Parameters;
	use rmcp::ServerHandler;
	use tempfile::tempdir;

	use super::json_error_result;
	use super::json_result;
	use super::resolve_root;
	use super::AffectedParam;
	use super::ChangeParam;
	use super::McpChangeBump;
	use super::MonochangeMcpServer;
	use super::PathParam;

	fn setup_fixture(relative: &str) -> tempfile::TempDir {
		monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
	}

	fn setup_scenario_workspace(relative: &str) -> tempfile::TempDir {
		monochange_test_helpers::fs::setup_scenario_workspace_from(
			env!("CARGO_MANIFEST_DIR"),
			relative,
		)
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
		assert!(info.instructions.as_ref().is_some_and(|text| text
			.contains("monochange manages versions and releases across Cargo, npm, Deno, and Dart/Flutter workspaces")));
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
		let original =
			std::env::current_dir().unwrap_or_else(|error| panic!("current dir: {error}"));
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
		let prepared = crate::prepare_release(tempdir.path(), true)
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

	#[tokio::test]
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

	#[tokio::test]
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

	#[tokio::test]
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

	#[tokio::test]
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

	#[tokio::test]
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

	#[tokio::test]
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

	#[tokio::test]
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
				details: None,
				output: None,
			}))
			.await
			.unwrap_or_else(|error| panic!("change: {error}"));

		assert_snapshot!(content_text(&result));
	}

	#[tokio::test]
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

	#[tokio::test]
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

		assert_snapshot!(content_text(&result));
	}

	#[tokio::test]
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

	#[tokio::test]
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

	#[tokio::test]
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

		assert_snapshot!(content_text(&result));
	}

	#[tokio::test]
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

	#[tokio::test]
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
			rendered.contains("unknown")
				|| rendered.contains("empty id")
				|| rendered.contains("step")
		);
	}
}
