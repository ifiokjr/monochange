use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use monochange_config::validate_workspace;
use monochange_core::CliCommandDefinition;
use monochange_core::ReleaseManifest;
use rmcp::ErrorData as McpError;
use rmcp::ServerHandler;
use rmcp::ServiceExt;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::schemars;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;

use crate::ChangeBump;
use crate::PreparedRelease;

/// Common `path` parameter used by MCP tools that operate on a repository root.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PathParam {
	pub path: Option<String>,
}

/// Empty payload for tools that do not need arguments.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct EmptyParam {}

/// Input payload for the MCP changeset diagnostics tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DiagnosticsParam {
	pub path: Option<String>,
	#[serde(default)]
	pub changeset: Vec<String>,
}

/// Input payload for the MCP lint explanation tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LintExplainParam {
	pub id: String,
}

/// Input payload for the MCP change-file creation tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ChangeParam {
	pub path: Option<String>,
	pub package: Vec<String>,
	pub bump: McpChangeBump,
	pub version: Option<String>,
	pub reason: String,
	#[serde(rename = "type")]
	pub change_type: Option<String>,
	#[serde(default)]
	pub caused_by: Vec<String>,
	pub details: Option<String>,
	pub output: Option<String>,
}

/// Input payload for the MCP changeset-policy evaluation tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AffectedParam {
	pub path: Option<String>,
	pub changed_paths: Vec<String>,
	#[serde(default)]
	pub labels: Vec<String>,
}

/// Input payload for the MCP analyze-changes tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeChangesParam {
	pub path: Option<String>,
	/// Explicit frame specification (e.g., "working", "main...feature", "pr:target,source")
	pub frame: Option<String>,
	/// Detection level: "basic", "signature", or "semantic"
	pub detection_level: Option<String>,
	/// Maximum number of changeset suggestions to generate
	pub max_suggestions: Option<usize>,
}

/// Input payload for the MCP validate-changeset tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateChangesetParam {
	pub path: Option<String>,
	/// Path to the changeset file to validate
	pub changeset_path: String,
}

/// Validation issue for a changeset.
#[derive(Debug, Serialize, schemars::JsonSchema)]
#[non_exhaustive]
pub struct ValidationIssue {
	pub severity: String,
	pub message: String,
	pub suggestion: Option<String>,
}

/// Parse a frame string into a `ChangeFrame`.
fn parse_frame(frame_str: &str) -> monochange_analysis::ChangeFrame {
	use monochange_analysis::ChangeFrame;

	if frame_str == "working" || frame_str == "HEAD" {
		return ChangeFrame::WorkingDirectory;
	}

	if frame_str == "staged" {
		return ChangeFrame::StagedOnly;
	}

	// Try parsing as branch range: "main...feature"
	if frame_str.contains("...") {
		let parts: Vec<&str> = frame_str.split("...").collect();
		if parts.len() == 2 {
			return ChangeFrame::BranchRange {
				base: parts[0].to_string(),
				head: parts[1].to_string(),
			};
		}
	}

	// Try parsing as PR format: "pr:target,source"
	if let Some(stripped) = frame_str.strip_prefix("pr:") {
		let parts: Vec<&str> = stripped.split(',').collect();
		if parts.len() == 2 {
			return ChangeFrame::PullRequest {
				target: parts[0].to_string(),
				pr_branch: parts[1].to_string(),
			};
		}
	}

	// Default to working directory
	ChangeFrame::WorkingDirectory
}

/// Parse a detection level string.
fn parse_detection_level(level: &str) -> monochange_analysis::DetectionLevel {
	use monochange_analysis::DetectionLevel;

	match level.to_lowercase().as_str() {
		"basic" => DetectionLevel::Basic,
		"semantic" => DetectionLevel::Semantic,
		_ => DetectionLevel::Signature, // Default
	}
}

/// Validate changeset content against semantic diff output.
fn validate_changeset_content(
	changeset: &monochange_config::LoadedChangesetFile,
	analysis: &monochange_analysis::ChangeAnalysis,
) -> Vec<ValidationIssue> {
	let mut issues = Vec::new();
	let changeset_text = render_changeset_text(changeset);
	let mentioned_refs = extract_backticked_refs(&changeset_text);
	let mut known_item_refs = BTreeSet::new();
	let mut known_package_refs = BTreeSet::new();
	let mut targeted_package_ids = BTreeSet::new();

	for signal in &changeset.signals {
		targeted_package_ids.insert(signal.package_id.clone());
	}

	for package_id in &targeted_package_ids {
		let Some(package_analysis) = find_package_analysis(analysis, package_id) else {
			issues.push(ValidationIssue {
				severity: "error".to_string(),
				message: format!(
					"changeset targets `{package_id}` but that package has no current diff in the analyzed frame"
				),
				suggestion: Some(
					"remove the stale target, update the changeset to match the current diff, or analyze a different frame".to_string(),
				),
			});
			continue;
		};
		let package_label = &package_analysis.package_id;

		known_package_refs.insert(package_analysis.package_id.clone());
		known_package_refs.insert(package_analysis.package_name.clone());
		known_package_refs.insert(package_analysis.package_record_id.clone());

		let item_refs = semantic_item_refs(package_analysis);
		known_item_refs.extend(item_refs.iter().cloned());

		if package_analysis.semantic_changes.is_empty() {
			issues.push(ValidationIssue {
				severity: "warning".to_string(),
				message: format!(
					"changeset targets `{package_label}` but no semantic changes were detected for that package"
				),
				suggestion: Some(
					"if the change is intentionally internal, review it manually; otherwise update the changeset or the analyzed frame".to_string(),
				),
			});
			continue;
		}

		let mentions_detected_item = mentioned_refs
			.iter()
			.any(|reference| item_refs.contains(reference));
		if !mentioned_refs.is_empty() && mentions_detected_item {
			continue;
		}

		let examples = item_refs.into_iter().take(3).collect::<Vec<_>>();
		debug_assert!(
			!examples.is_empty(),
			"semantic changes should yield at least one item reference"
		);

		issues.push(ValidationIssue {
			severity: "warning".to_string(),
			message: format!(
				"changeset does not mention any detected semantic item for `{package_label}`"
			),
			suggestion: Some(format!(
				"mention one or more changed items such as {}",
				examples
					.iter()
					.map(|item| format!("`{item}`"))
					.collect::<Vec<_>>()
					.join(", ")
			)),
		});
	}

	for reference in mentioned_refs {
		if known_item_refs.contains(&reference) || known_package_refs.contains(&reference) {
			continue;
		}

		issues.push(ValidationIssue {
			severity: "error".to_string(),
			message: format!(
				"changeset references `{reference}` but that item was not found in the current semantic diff"
			),
			suggestion: Some(
				"remove the stale reference or update it to match the current code change"
					.to_string(),
			),
		});
	}

	issues
}

fn find_package_analysis<'a>(
	analysis: &'a monochange_analysis::ChangeAnalysis,
	package_id: &str,
) -> Option<&'a monochange_analysis::PackageChangeAnalysis> {
	analysis
		.package_analyses
		.values()
		.find(|package| package.package_id == package_id || package.package_record_id == package_id)
}

fn render_changeset_text(changeset: &monochange_config::LoadedChangesetFile) -> String {
	let mut parts = Vec::new();
	if let Some(summary) = &changeset.summary {
		parts.push(summary.clone());
	}
	if let Some(details) = &changeset.details {
		parts.push(details.clone());
	}
	parts.join("\n\n")
}

fn extract_backticked_refs(text: &str) -> BTreeSet<String> {
	let mut refs = BTreeSet::new();
	let mut current = String::new();
	let mut inside = false;

	for character in text.chars() {
		if character == '`' {
			if inside && !current.trim().is_empty() {
				refs.insert(current.trim().to_string());
			}
			inside = !inside;
			current.clear();
			continue;
		}

		if inside {
			current.push(character);
		}
	}

	refs
}

fn semantic_item_refs(
	package_analysis: &monochange_analysis::PackageChangeAnalysis,
) -> BTreeSet<String> {
	let mut refs = BTreeSet::new();
	for change in &package_analysis.semantic_changes {
		refs.insert(change.item_path.clone());
		if let Some(last_segment) = change.item_path.rsplit("::").next() {
			refs.insert(last_segment.to_string());
		}
	}
	refs
}

/// Semver bump accepted by the MCP change-file tool.
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

/// Stdio MCP server exposing structured `monochange` tools.
#[derive(Debug, Clone)]
pub struct MonochangeMcpServer {
	#[allow(dead_code)]
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
	let Some(path_str) = path else {
		return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
	};

	PathBuf::from(path_str)
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
	/// Construct a server with the default tool router.
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

		if let Err(error) = validate_workspace(&root) {
			return Ok(json_error_result(json!({
				"ok": false,
				"action": "validate",
				"root": root,
				"summary": error.render(),
				"error": error.render()
			})));
		}

		Ok(json_result(json!({
			"ok": true,
			"action": "validate",
			"root": root,
			"summary": "workspace validation passed"
		})))
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

		let report = match crate::discover_workspace(&root) {
			Ok(report) => report,
			Err(error) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "discover",
					"root": root,
					"summary": error.render(),
					"error": error.render()
				})));
			}
		};

		Ok(json_result(json!({
			"ok": true,
			"action": "discover",
			"summary": format!(
				"Discovered {} package(s) and {} dependency edge(s).",
				report.packages.len(),
				report.dependencies.len()
			),
			"report": report,
		})))
	}

	#[tool(
		name = "monochange_diagnostics",
		description = "Inspect pending changesets with git and review context as structured JSON."
	)]
	async fn diagnostics(
		&self,
		Parameters(params): Parameters<DiagnosticsParam>,
	) -> Result<CallToolResult, McpError> {
		let root = resolve_root(params.path.as_deref());

		let report = match crate::changesets::diagnose_changesets(&root, &params.changeset) {
			Ok(report) => report,
			Err(error) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "diagnostics",
					"root": root,
					"summary": error.render(),
					"error": error.render()
				})));
			}
		};

		Ok(json_result(json!({
			"ok": true,
			"action": "diagnostics",
			"summary": format!(
				"Loaded {} changeset diagnostic record(s).",
				report.changesets.len()
			),
			"report": report,
		})))
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

		let request = crate::AddChangeFileRequest::builder()
			.package_refs(&params.package)
			.bump(bump.into())
			.reason(&params.reason)
			.version(params.version.as_deref())
			.change_type(params.change_type.as_deref())
			.caused_by(&params.caused_by)
			.details(params.details.as_deref())
			.output(output)
			.build();

		let path = match crate::add_change_file(&root, request) {
			Ok(path) => path,
			Err(error) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "change",
					"root": root,
					"summary": error.render(),
					"error": error.render()
				})));
			}
		};

		Ok(json_result(json!({
			"ok": true,
			"action": "change",
			"root": root,
			"path": path,
			"summary": format!("Wrote change file {}", path.display())
		})))
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

		let prepared_release = match crate::prepare_release(&root, true) {
			Ok(release) => release,
			Err(error) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "release_preview",
					"root": root,
					"summary": error.render(),
					"error": error.render()
				})));
			}
		};

		Ok(json_result(json!({
			"ok": true,
			"action": "release_preview",
			"summary": format!(
				"Prepared dry-run release preview with {} release target(s).",
				prepared_release.release_targets.len()
			),
			"release": prepared_release_value(&prepared_release)
		})))
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

		let prepared_release = match crate::prepare_release(&root, true) {
			Ok(release) => release,
			Err(error) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "release_manifest",
					"root": root,
					"summary": error.render(),
					"error": error.render()
				})));
			}
		};

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

	#[tool(
		name = "monochange_affected_packages",
		description = "Evaluate changeset policy from changed paths and optional labels."
	)]
	async fn affected_packages(
		&self,
		Parameters(params): Parameters<AffectedParam>,
	) -> Result<CallToolResult, McpError> {
		let root = resolve_root(params.path.as_deref());

		let evaluation =
			match crate::affected_packages(&root, &params.changed_paths, &params.labels) {
				Ok(eval) => eval,
				Err(error) => {
					return Ok(json_error_result(json!({
						"ok": false,
						"action": "affected_packages",
						"root": root,
						"summary": error.render(),
						"error": error.render()
					})));
				}
			};

		Ok(json_result(json!({
			"ok": evaluation.status != monochange_core::ChangesetPolicyStatus::Failed,
			"action": "affected_packages",
			"summary": evaluation.summary,
			"evaluation": evaluation,
		})))
	}

	#[tool(
		name = "monochange_lint_catalog",
		description = "List registered manifest lint rules and presets as structured JSON."
	)]
	async fn lint_catalog(
		&self,
		Parameters(_params): Parameters<EmptyParam>,
	) -> Result<CallToolResult, McpError> {
		let rules = crate::lint::available_lint_rules();
		let presets = crate::lint::available_lint_presets();

		Ok(json_result(json!({
			"ok": true,
			"action": "lint_catalog",
			"summary": format!(
				"Loaded {} lint rule(s) and {} preset(s).",
				rules.len(),
				presets.len()
			),
			"rules": rules,
			"presets": presets,
		})))
	}

	#[tool(
		name = "monochange_lint_explain",
		description = "Explain one manifest lint rule or preset as structured JSON."
	)]
	async fn lint_explain(
		&self,
		Parameters(params): Parameters<LintExplainParam>,
	) -> Result<CallToolResult, McpError> {
		if let Some(rule) = crate::lint::explain_lint_rule(&params.id) {
			return Ok(json_result(json!({
				"ok": true,
				"action": "lint_explain",
				"kind": "rule",
				"summary": format!("Loaded lint rule `{}`.", rule.id),
				"entry": rule,
			})));
		}

		if let Some(preset) = crate::lint::explain_lint_preset(&params.id) {
			return Ok(json_result(json!({
				"ok": true,
				"action": "lint_explain",
				"kind": "preset",
				"summary": format!("Loaded lint preset `{}`.", preset.id),
				"entry": preset,
			})));
		}

		Ok(json_error_result(json!({
			"ok": false,
			"action": "lint_explain",
			"summary": format!("Unknown lint rule or preset `{}`.", params.id),
			"error": format!("unknown lint rule or preset `{}`", params.id)
		})))
	}

	#[tool(
		name = "monochange_analyze_changes",
		description = "Analyze git diff and return ecosystem-specific semantic diffs for changed packages."
	)]
	async fn analyze_changes(
		&self,
		Parameters(params): Parameters<AnalyzeChangesParam>,
	) -> Result<CallToolResult, McpError> {
		use monochange_analysis::AnalysisConfig;
		use monochange_analysis::ChangeFrame;

		let root = resolve_root(params.path.as_deref());

		// Determine the change frame
		let frame = match params.frame {
			Some(frame_str) => parse_frame(&frame_str),
			None => {
				match ChangeFrame::detect(&root) {
					Ok(f) => f,
					Err(e) => {
						return Ok(json_error_result(json!({
							"ok": false,
							"action": "analyze_changes",
							"root": root,
							"summary": format!("Failed to detect change frame: {}", e),
							"error": e.to_string()
						})));
					}
				}
			}
		};

		// Configure analysis
		// Configure analysis
		let detection_level = params.detection_level.as_deref().map(parse_detection_level);
		let config = AnalysisConfig {
			detection_level: detection_level.unwrap_or_else(|| {
				use monochange_analysis::DetectionLevel;
				DetectionLevel::Signature
			}),
			thresholds: monochange_analysis::GroupingThresholds::default(),
			max_suggestions: params.max_suggestions.unwrap_or(10),
		};

		// Run analysis
		let analysis = match monochange_analysis::analyze_changes(&root, &frame, &config) {
			Ok(a) => a,
			Err(e) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "analyze_changes",
					"root": root,
					"summary": format!("Analysis failed: {}", e.render()),
					"error": e.render()
				})));
			}
		};

		// Convert to JSON
		let analysis_json = serde_json::to_value(&analysis)
			.map_err(|e| McpError::internal_error(e.to_string(), None))?;

		let package_count = analysis.package_analyses.len();
		let semantic_change_count = analysis
			.package_analyses
			.values()
			.map(|package| package.semantic_changes.len())
			.sum::<usize>();

		Ok(json_result(json!({
			"ok": true,
			"action": "analyze_changes",
			"frame": frame.to_string(),
			"analysis": analysis_json,
			"summary": format!(
				"Analyzed {} package(s) and found {} semantic change(s)",
				package_count,
				semantic_change_count,
			)
		})))
	}

	#[tool(
		name = "monochange_validate_changeset",
		description = "Validate that a changeset matches the current semantic diff for its targeted packages."
	)]
	async fn validate_changeset(
		&self,
		Parameters(params): Parameters<ValidateChangesetParam>,
	) -> Result<CallToolResult, McpError> {
		let root = resolve_root(params.path.as_deref());
		let changeset_path = root.join(&params.changeset_path);

		// Load the changeset
		let configuration = match monochange_config::load_workspace_configuration(&root) {
			Ok(c) => c,
			Err(e) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "validate_changeset",
					"root": root,
					"summary": format!("Failed to load workspace: {}", e.render()),
					"error": e.render()
				})));
			}
		};

		let discovery = match crate::discover_workspace(&root) {
			Ok(d) => d,
			Err(e) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "validate_changeset",
					"root": root,
					"summary": format!("Failed to discover workspace: {}", e.render()),
					"error": e.render()
				})));
			}
		};

		let loaded = match monochange_config::load_changeset_file(
			&changeset_path,
			&configuration,
			&discovery.packages,
		) {
			Ok(l) => l,
			Err(e) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "validate_changeset",
					"root": root,
					"changeset_path": changeset_path,
					"summary": format!("Failed to load changeset: {}", e.render()),
					"error": e.render()
				})));
			}
		};

		let frame = match monochange_analysis::ChangeFrame::detect(&root) {
			Ok(frame) => frame,
			Err(error) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "validate_changeset",
					"root": root,
					"changeset_path": changeset_path,
					"summary": format!("Failed to detect change frame: {error}"),
					"error": error.to_string()
				})));
			}
		};

		let analysis = match monochange_analysis::analyze_changes(
			&root,
			&frame,
			&monochange_analysis::AnalysisConfig::default(),
		) {
			Ok(analysis) => analysis,
			Err(error) => {
				return Ok(json_error_result(json!({
					"ok": false,
					"action": "validate_changeset",
					"root": root,
					"changeset_path": changeset_path,
					"summary": format!("Semantic analysis failed: {}", error.render()),
					"error": error.render()
				})));
			}
		};

		let issues = validate_changeset_content(&loaded, &analysis);
		let valid = issues.is_empty();
		let lifecycle_status = if issues.iter().any(|issue| issue.severity == "error") {
			"stale"
		} else if issues.is_empty() {
			"current"
		} else {
			"incomplete"
		};

		Ok(json_result(json!({
			"ok": valid,
			"action": "validate_changeset",
			"frame": frame.to_string(),
			"changeset_path": params.changeset_path,
			"valid": valid,
			"issues": issues,
			"lifecycle_status": lifecycle_status,
			"summary": if valid {
				"Changeset validation passed".to_string()
			} else {
				format!("Found {} validation issue(s)", issues.len())
			}
		})))
	}
}

/// Run the stdio MCP server used by `mc mcp`.
pub async fn run_server() {
	let server = MonochangeMcpServer::new();
	let transport = rmcp::transport::io::stdio();

	let running = match server.serve(transport).await {
		Ok(running) => running,
		Err(error) => {
			eprintln!("monochange-mcp: failed to start server: {error}");
			return;
		}
	};

	let _ = running.waiting().await;
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod __tests {
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
	use super::resolve_root;

	fn setup_fixture(relative: &str) -> tempfile::TempDir {
		monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
	}

	fn setup_scenario_workspace(relative: &str) -> tempfile::TempDir {
		monochange_test_helpers::fs::setup_scenario_workspace_from(
			env!("CARGO_MANIFEST_DIR"),
			relative,
		)
	}

	fn fixture_path(relative: &str) -> PathBuf {
		monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
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

	#[tokio::test]
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
				caused_by: Vec::new(),
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

	#[tokio::test]
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

	#[tokio::test]
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

		assert_snapshot!(
			serde_json::to_string_pretty(&combined).unwrap_or_else(|error| {
				panic!("serialize combined lint explanation snapshot: {error}")
			})
		);
	}

	#[tokio::test]
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

	#[tokio::test]
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

		assert_snapshot!(
			serde_json::to_string_pretty(&eval)
				.unwrap_or_else(|error| { panic!("serialize agent eval snapshot: {error}") })
		);
	}

	#[tokio::test]
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

	#[tokio::test]
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

	#[tokio::test]
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

	#[tokio::test]
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

	#[tokio::test]
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

	#[tokio::test]
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
}
