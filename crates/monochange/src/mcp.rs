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
			"Monochange manages versions and releases across Cargo, npm, Deno, and Dart/Flutter \
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
			&params.package,
			bump.into(),
			params.version.as_deref(),
			&params.reason,
			params.change_type.as_deref(),
			params.details.as_deref(),
			output,
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

	use rmcp::handler::server::wrapper::Parameters;
	use tempfile::tempdir;

	use super::AffectedParam;
	use super::ChangeParam;
	use super::McpChangeBump;
	use super::MonochangeMcpServer;
	use super::PathParam;

	fn content_text(result: &rmcp::model::CallToolResult) -> String {
		result
			.content
			.first()
			.and_then(|content| match &content.raw {
				rmcp::model::RawContent::Text(text) => Some(text.text.clone()),
				_ => None,
			})
			.unwrap_or_default()
	}

	#[tokio::test]
	async fn discover_reports_workspace_packages() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::create_dir_all(tempdir.path().join("crates/core"))
			.unwrap_or_else(|error| panic!("mkdir: {error}"));
		fs::write(
			tempdir.path().join("crates/core/Cargo.toml"),
			"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
		)
		.unwrap_or_else(|error| panic!("cargo write: {error}"));

		let result = MonochangeMcpServer::new()
			.discover(Parameters(PathParam {
				path: Some(tempdir.path().display().to_string()),
			}))
			.await
			.unwrap_or_else(|error| panic!("discover: {error}"));

		assert!(content_text(&result).contains("Discovered 1 package(s)"));
	}

	#[tokio::test]
	async fn change_writes_markdown_changeset() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::create_dir_all(tempdir.path().join("crates/core"))
			.unwrap_or_else(|error| panic!("mkdir: {error}"));
		fs::write(
			tempdir.path().join("crates/core/Cargo.toml"),
			"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
		)
		.unwrap_or_else(|error| panic!("cargo write: {error}"));
		fs::write(
			tempdir.path().join("monochange.toml"),
			r#"
[package.core]
path = "crates/core"
type = "cargo"
"#,
		)
		.unwrap_or_else(|error| panic!("config write: {error}"));

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

		assert!(content_text(&result).contains("Wrote change file"));
		let contents = fs::read_to_string(tempdir.path().join(".changeset/core.md"))
			.unwrap_or_else(|error| panic!("changeset read: {error}"));
		assert!(contents.contains("core: patch"));
	}

	#[test]
	fn mcp_change_bump_none_maps_to_change_bump_none() {
		assert_eq!(
			crate::ChangeBump::from(McpChangeBump::None),
			crate::ChangeBump::None
		);
	}

	#[tokio::test]
	async fn change_writes_type_only_markdown_changeset() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::create_dir_all(tempdir.path().join("crates/core"))
			.unwrap_or_else(|error| panic!("mkdir: {error}"));
		fs::write(
			tempdir.path().join("crates/core/Cargo.toml"),
			"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
		)
		.unwrap_or_else(|error| panic!("cargo write: {error}"));
		fs::write(
			tempdir.path().join("monochange.toml"),
			r#"
[package.core]
path = "crates/core"
type = "cargo"
extra_changelog_sections = [{ name = "Documentation", types = ["docs"] }]
"#,
		)
		.unwrap_or_else(|error| panic!("config write: {error}"));

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

		assert!(content_text(&result).contains("Wrote change file"));
		let contents = fs::read_to_string(tempdir.path().join(".changeset/core-docs.md"))
			.unwrap_or_else(|error| panic!("changeset read: {error}"));
		assert!(contents.contains("core: docs"));
		assert!(!contents.contains("bump:"));
	}

	#[tokio::test]
	async fn affected_packages_reports_success_for_documentation_only_changes() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::write(
			tempdir.path().join("monochange.toml"),
			r#"
[changesets.verify]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["crates/**"]
ignored_paths = ["docs/**"]

[cli.affected]
help_text = "Evaluate pull-request changeset policy"

[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.affected.inputs]]
name = "label"
type = "string_list"

[[cli.affected.steps]]
type = "AffectedPackages"
"#,
		)
		.unwrap_or_else(|error| panic!("config write: {error}"));

		let result = MonochangeMcpServer::new()
			.affected_packages(Parameters(AffectedParam {
				path: Some(tempdir.path().display().to_string()),
				changed_paths: vec!["docs/readme.md".to_string()],
				labels: Vec::new(),
			}))
			.await
			.unwrap_or_else(|error| panic!("affected: {error}"));

		let text = content_text(&result);
		assert!(text.contains("\"ok\": true"));
		assert!(text.contains("\"status\": "));
		assert!(
			text.contains("not_required") || text.contains("skipped") || text.contains("passed")
		);
	}
}
