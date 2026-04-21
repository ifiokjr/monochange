use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use serde::Serialize;
use serde_json::json;

use crate::SubagentOutputFormat;
use crate::SubagentTarget;

#[derive(Debug, Clone)]
pub(crate) struct SubagentOptions {
	pub targets: Vec<SubagentTarget>,
	pub force: bool,
	pub dry_run: bool,
	pub format: SubagentOutputFormat,
	pub generate_mcp: bool,
}

#[derive(Debug, Clone)]
struct GeneratedFileDraft {
	path: PathBuf,
	description: String,
	contents: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GeneratedFileOperation {
	Create,
	Overwrite,
	Skip,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct GeneratedFile {
	pub path: PathBuf,
	pub description: String,
	pub operation: GeneratedFileOperation,
	pub contents: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct SubagentPlan {
	pub targets: Vec<SubagentTarget>,
	pub files: Vec<GeneratedFile>,
	pub notes: Vec<String>,
	pub dry_run: bool,
}

pub(crate) fn run_subagents(root: &Path, options: &SubagentOptions) -> MonochangeResult<String> {
	let plan = build_subagent_plan(root, options)?;

	if !options.dry_run {
		write_subagent_plan(root, &plan, options.force)?;
	}

	match options.format {
		SubagentOutputFormat::Json => {
			serde_json::to_string_pretty(&plan)
				.map_err(|error| MonochangeError::Config(error.to_string()))
		}
		SubagentOutputFormat::Markdown => {
			Ok(crate::maybe_render_markdown_for_terminal(
				&render_subagent_plan_text(&plan),
			))
		}
		SubagentOutputFormat::Text => Ok(render_subagent_plan_text(&plan)),
	}
}

fn build_subagent_plan(root: &Path, options: &SubagentOptions) -> MonochangeResult<SubagentPlan> {
	let mut drafts = Vec::new();
	let mut notes = Vec::new();

	for target in &options.targets {
		for draft in generate_target_files(*target, options.generate_mcp, &mut notes)? {
			push_generated_file(&mut drafts, draft)?;
		}
	}

	let files = drafts
		.into_iter()
		.map(|draft| {
			let absolute_path = root.join(&draft.path);
			let operation = classify_generated_file(&absolute_path, &draft.contents)?;

			Ok(GeneratedFile {
				path: draft.path,
				description: draft.description,
				operation,
				contents: draft.contents,
			})
		})
		.collect::<MonochangeResult<Vec<_>>>()?;

	Ok(SubagentPlan {
		targets: options.targets.clone(),
		files,
		notes,
		dry_run: options.dry_run,
	})
}

fn push_generated_file(
	drafts: &mut Vec<GeneratedFileDraft>,
	draft: GeneratedFileDraft,
) -> MonochangeResult<()> {
	let Some(existing) = drafts.iter().find(|existing| existing.path == draft.path) else {
		drafts.push(draft);
		return Ok(());
	};

	if existing.contents == draft.contents {
		return Ok(());
	}

	Err(MonochangeError::Config(format!(
		"multiple subagent targets attempted to generate different contents for {}",
		draft.path.display()
	)))
}

fn classify_generated_file(
	path: &Path,
	expected: &str,
) -> MonochangeResult<GeneratedFileOperation> {
	if !path.exists() {
		return Ok(GeneratedFileOperation::Create);
	}

	let current = fs::read_to_string(path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;

	if current == expected {
		return Ok(GeneratedFileOperation::Skip);
	}

	Ok(GeneratedFileOperation::Overwrite)
}

fn write_subagent_plan(root: &Path, plan: &SubagentPlan, force: bool) -> MonochangeResult<()> {
	let conflicts = plan
		.files
		.iter()
		.filter(|file| !force && file.operation == GeneratedFileOperation::Overwrite)
		.map(|file| file.path.display().to_string())
		.collect::<Vec<_>>();

	if !conflicts.is_empty() {
		return Err(MonochangeError::Config(format!(
			"refusing to overwrite existing subagent files without --force: {}",
			conflicts.join(", ")
		)));
	}

	for file in &plan.files {
		if file.operation == GeneratedFileOperation::Skip {
			continue;
		}

		let absolute_path = root.join(&file.path);
		let Some(parent) = absolute_path.parent() else {
			continue;
		};

		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
		fs::write(&absolute_path, &file.contents).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to write {}: {error}",
				absolute_path.display()
			))
		})?;
	}

	Ok(())
}

fn render_subagent_plan_text(plan: &SubagentPlan) -> String {
	let mut output = String::new();
	let _ = writeln!(output, "monochange subagents");
	let _ = writeln!(output);
	let _ = writeln!(output, "Targets:");

	for target in &plan.targets {
		let _ = writeln!(output, "- {}", subagent_target_name(*target));
	}

	let _ = writeln!(output);
	let _ = writeln!(output, "Files:");

	for file in &plan.files {
		let _ = writeln!(
			output,
			"- {} {}",
			generated_file_operation_name(&file.operation),
			file.path.display()
		);
	}

	if !plan.notes.is_empty() {
		let _ = writeln!(output);
		let _ = writeln!(output, "Notes:");

		for note in &plan.notes {
			let _ = writeln!(output, "- {note}");
		}
	}

	if plan.dry_run {
		let _ = writeln!(output);
		let _ = writeln!(output, "Dry run only. No files were written.");
	}

	output.trim_end().to_string()
}

fn generated_file_operation_name(operation: &GeneratedFileOperation) -> &'static str {
	match operation {
		GeneratedFileOperation::Create => "create",
		GeneratedFileOperation::Overwrite => "overwrite",
		GeneratedFileOperation::Skip => "skip",
	}
}

fn generate_target_files(
	target: SubagentTarget,
	generate_mcp: bool,
	notes: &mut Vec<String>,
) -> MonochangeResult<Vec<GeneratedFileDraft>> {
	let mut files = Vec::new();

	match target {
		SubagentTarget::Claude => {
			files.push(GeneratedFileDraft {
				path: PathBuf::from(".claude/agents/monochange-release-agent.md"),
				description: "Claude subagent definition".to_string(),
				contents: render_claude_agent(),
			});

			if generate_mcp {
				files.push(GeneratedFileDraft {
					path: PathBuf::from(".mcp.json"),
					description: "Claude MCP server configuration".to_string(),
					contents: render_claude_mcp_config()?,
				});
			}
		}

		SubagentTarget::Vscode | SubagentTarget::Copilot => {
			let description = if target == SubagentTarget::Vscode {
				"VS Code"
			} else {
				"GitHub Copilot"
			};
			files.push(GeneratedFileDraft {
				path: PathBuf::from(".github/agents/monochange-release-agent.agent.md"),
				description: format!("{description} agent definition"),
				contents: render_vscode_agent(),
			});

			if generate_mcp {
				files.push(GeneratedFileDraft {
					path: PathBuf::from(".vscode/mcp.json"),
					description: "VS Code MCP server configuration".to_string(),
					contents: render_vscode_mcp_config()?,
				});
			}
		}

		SubagentTarget::Pi => {
			files.push(GeneratedFileDraft {
				path: PathBuf::from(".pi/agents/monochange-release-agent.md"),
				description: "Pi project agent definition".to_string(),
				contents: render_pi_agent(),
			});
		}

		SubagentTarget::Codex => {
			files.push(GeneratedFileDraft {
				path: PathBuf::from(".codex/agents/monochange-release-agent.toml"),
				description: "Codex custom agent definition".to_string(),
				contents: render_codex_agent(),
			});
		}

		SubagentTarget::Cursor => {
			files.push(GeneratedFileDraft {
				path: PathBuf::from(".cursor/rules/monochange.mdc"),
				description: "Cursor workspace rule".to_string(),
				contents: render_cursor_rule(),
			});
			notes.push(
				"Cursor generation currently emits a repo-local workspace rule instead of a native custom subagent manifest.".to_string(),
			);
		}
	}

	Ok(files)
}

fn render_claude_agent() -> String {
	format!(
		"---\nname: monochange-release-agent\ndescription: Use this agent for monochange configuration, changesets, diagnostics, and release planning.\ntools: Bash, Read, Grep, Glob, LS\ncolor: blue\n---\n\n{}\n",
		shared_subagent_instructions(),
	)
}

fn render_vscode_agent() -> String {
	format!(
		"---\nname: monochange-release-agent\ndescription: Use this agent for monochange configuration, changesets, diagnostics, and release planning.\n---\n\n{}\n",
		shared_subagent_instructions(),
	)
}

fn render_pi_agent() -> String {
	format!(
		"---\nname: monochange-release-agent\ndescription: Use this agent for monochange configuration, changesets, diagnostics, and release planning.\ntools: read, grep, find, bash\n---\n\n{}\n",
		shared_subagent_instructions(),
	)
}

fn render_codex_agent() -> String {
	format!(
		"name = \"monochange-release-agent\"\ndescription = \"Use this agent for monochange configuration, changesets, diagnostics, and release planning.\"\ndeveloper_instructions = \"\"\"\n{}\n\"\"\"\n",
		shared_subagent_instructions(),
	)
}

fn render_cursor_rule() -> String {
	format!(
		"---\ndescription: monochange workflow guidance for release planning, changesets, diagnostics, and changelog updates\nglobs:\n  - \"**/*\"\nalwaysApply: false\n---\n\n{}\n",
		shared_cursor_instructions(),
	)
}

fn render_claude_mcp_config() -> MonochangeResult<String> {
	serde_json::to_string_pretty(&json!({
		"mcpServers": {
			"monochange": {
				"command": "npx",
				"args": ["-y", "@monochange/cli", "mcp"]
			}
		}
	}))
	.map_err(|error| MonochangeError::Config(error.to_string()))
}

fn render_vscode_mcp_config() -> MonochangeResult<String> {
	serde_json::to_string_pretty(&json!({
		"servers": {
			"monochange": {
				"type": "stdio",
				"command": "npx",
				"args": ["-y", "@monochange/cli", "mcp"]
			}
		},
		"inputs": []
	}))
	.map_err(|error| MonochangeError::Config(error.to_string()))
}

fn shared_subagent_instructions() -> &'static str {
	"You are the monochange release agent for this repository.

When working on release planning, versioning, changesets, changelogs, or monochange configuration:

1. Read `monochange.toml` before suggesting workflow or release changes.
2. Prefer the monochange CLI over MCP tools.
3. Choose the CLI executable in this order:
   - `mc`
   - `monochange`
   - `npx -y @monochange/cli`
4. Use structured JSON output when inspecting workspace state:
   - `<cli> validate`
   - `<cli> discover --format json`
   - `<cli> diagnostics --format json`
   - `<cli> release --dry-run --format json`
5. Prefer `mc change` and `.changeset/*.md` files over ad hoc release notes.
6. Run validation before and after release-affecting edits.
7. Do not run mutating release or publish commands unless the user explicitly asks.
8. If monochange MCP tools are available, use them as a secondary structured fallback when they are more useful than shelling out.

Recommended workflow:
- validate
- discover
- inspect diagnostics
- edit config or changesets
- run a dry-run release preview
- summarize the impact and next steps"
}

fn shared_cursor_instructions() -> &'static str {
	"When working in this repository on release planning, versioning, changesets, changelogs, or `monochange.toml`:

1. Read `monochange.toml` first.
2. Prefer the monochange CLI over MCP tools.
3. Choose the CLI executable in this order:
   - `mc`
   - `monochange`
   - `npx -y @monochange/cli`
4. Use JSON output when inspecting repository state:
   - `<cli> validate`
   - `<cli> discover --format json`
   - `<cli> diagnostics --format json`
   - `<cli> release --dry-run --format json`
5. Prefer `mc change` and `.changeset/*.md` files over ad hoc release notes.
6. Run validation before and after release-affecting edits.
7. Do not run mutating release or publish commands unless the user explicitly asks.
8. If monochange MCP tools are configured, use them only as a secondary structured fallback."
}

fn subagent_target_name(target: SubagentTarget) -> &'static str {
	match target {
		SubagentTarget::Claude => "claude",
		SubagentTarget::Vscode => "vscode",
		SubagentTarget::Copilot => "copilot",
		SubagentTarget::Pi => "pi",
		SubagentTarget::Codex => "codex",
		SubagentTarget::Cursor => "cursor",
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;

	use monochange_test_helpers::fs::fixture_path_from;
	use monochange_test_helpers::fs::setup_scenario_workspace_from;

	use super::*;

	fn fixture_readme_contents() -> String {
		fs::read_to_string(fixture_path_from(
			env!("CARGO_MANIFEST_DIR"),
			"subagents/basic/workspace/README.md",
		))
		.unwrap_or_else(|error| panic!("read fixture README: {error}"))
	}

	#[test]
	fn push_generated_file_handles_duplicate_and_conflicting_paths() {
		let contents = fixture_readme_contents();
		let mut drafts = Vec::new();
		let shared_path = PathBuf::from(".github/agents/monochange-release-agent.agent.md");

		push_generated_file(
			&mut drafts,
			GeneratedFileDraft {
				path: shared_path.clone(),
				description: "VS Code agent definition".to_string(),
				contents: contents.clone(),
			},
		)
		.unwrap_or_else(|error| panic!("push first draft: {error}"));
		push_generated_file(
			&mut drafts,
			GeneratedFileDraft {
				path: shared_path.clone(),
				description: "GitHub Copilot agent definition".to_string(),
				contents: contents.clone(),
			},
		)
		.unwrap_or_else(|error| panic!("push matching draft: {error}"));
		assert_eq!(drafts.len(), 1);

		let error = push_generated_file(
			&mut drafts,
			GeneratedFileDraft {
				path: shared_path,
				description: "conflicting agent definition".to_string(),
				contents: format!("{contents}\nconflict\n"),
			},
		)
		.err()
		.unwrap_or_else(|| panic!("expected conflicting draft error"));
		assert!(
			error
				.to_string()
				.contains("multiple subagent targets attempted to generate different contents")
		);
	}

	#[test]
	fn classify_generated_file_reports_create_skip_overwrite_and_read_errors() {
		let workspace =
			setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), "subagents/basic");
		let readme_path = workspace.path().join("README.md");
		let contents = fixture_readme_contents();

		assert_eq!(
			classify_generated_file(&workspace.path().join("missing.md"), &contents)
				.unwrap_or_else(|error| panic!("classify missing file: {error}")),
			GeneratedFileOperation::Create
		);
		assert_eq!(
			classify_generated_file(&readme_path, &contents)
				.unwrap_or_else(|error| panic!("classify matching file: {error}")),
			GeneratedFileOperation::Skip
		);
		assert_eq!(
			classify_generated_file(&readme_path, "different contents")
				.unwrap_or_else(|error| panic!("classify overwritten file: {error}")),
			GeneratedFileOperation::Overwrite
		);

		let error = classify_generated_file(workspace.path(), &contents)
			.err()
			.unwrap_or_else(|| panic!("expected classify read error"));
		assert!(
			error
				.to_string()
				.contains(&format!("failed to read {}", workspace.path().display()))
		);
	}

	#[test]
	fn write_subagent_plan_reports_conflicts_and_io_failures() {
		let workspace =
			setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), "subagents/basic");
		let contents = fixture_readme_contents();
		let plan = SubagentPlan {
			targets: vec![SubagentTarget::Claude],
			files: vec![GeneratedFile {
				path: PathBuf::from("README.md"),
				description: "README".to_string(),
				operation: GeneratedFileOperation::Overwrite,
				contents: contents.clone(),
			}],
			notes: Vec::new(),
			dry_run: false,
		};
		let error = write_subagent_plan(workspace.path(), &plan, false)
			.err()
			.unwrap_or_else(|| panic!("expected overwrite conflict"));
		assert!(
			error
				.to_string()
				.contains("refusing to overwrite existing subagent files without --force")
		);

		let parent_file_path = workspace.path().join("parent-file");
		fs::write(&parent_file_path, &contents)
			.unwrap_or_else(|error| panic!("write parent-file fixture: {error}"));
		let parent_error_plan = SubagentPlan {
			targets: vec![SubagentTarget::Pi],
			files: vec![GeneratedFile {
				path: PathBuf::from("parent-file/child.md"),
				description: "nested child".to_string(),
				operation: GeneratedFileOperation::Create,
				contents: contents.clone(),
			}],
			notes: Vec::new(),
			dry_run: false,
		};
		let parent_error = write_subagent_plan(workspace.path(), &parent_error_plan, true)
			.err()
			.unwrap_or_else(|| panic!("expected create_dir_all failure"));
		assert!(
			parent_error
				.to_string()
				.contains(&format!("failed to create {}", parent_file_path.display()))
		);

		let directory_path = workspace.path().join("existing-directory");
		fs::create_dir_all(&directory_path)
			.unwrap_or_else(|error| panic!("create existing-directory fixture: {error}"));
		let write_error_plan = SubagentPlan {
			targets: vec![SubagentTarget::Pi],
			files: vec![GeneratedFile {
				path: PathBuf::from("existing-directory"),
				description: "directory write error".to_string(),
				operation: GeneratedFileOperation::Create,
				contents,
			}],
			notes: Vec::new(),
			dry_run: false,
		};
		let write_error = write_subagent_plan(workspace.path(), &write_error_plan, true)
			.err()
			.unwrap_or_else(|| panic!("expected file write failure"));
		assert!(
			write_error
				.to_string()
				.contains(&format!("failed to write {}", directory_path.display()))
		);

		let no_parent_plan = SubagentPlan {
			targets: vec![SubagentTarget::Cursor],
			files: vec![GeneratedFile {
				path: PathBuf::new(),
				description: "no parent path".to_string(),
				operation: GeneratedFileOperation::Create,
				contents: String::new(),
			}],
			notes: Vec::new(),
			dry_run: false,
		};
		write_subagent_plan(Path::new(""), &no_parent_plan, true)
			.unwrap_or_else(|error| panic!("write empty path plan: {error}"));
	}

	#[test]
	fn render_and_generate_helpers_cover_notes_and_target_variants() {
		let mut notes = Vec::new();
		let claude_files = generate_target_files(SubagentTarget::Claude, true, &mut notes)
			.unwrap_or_else(|error| panic!("generate claude files: {error}"));
		assert_eq!(claude_files.len(), 2);
		assert!(
			claude_files
				.iter()
				.any(|file| file.path == Path::new(".mcp.json"))
		);
		let claude_files_without_mcp =
			generate_target_files(SubagentTarget::Claude, false, &mut notes)
				.unwrap_or_else(|error| panic!("generate claude files without mcp: {error}"));
		assert_eq!(claude_files_without_mcp.len(), 1);

		let vscode_files = generate_target_files(SubagentTarget::Vscode, true, &mut notes)
			.unwrap_or_else(|error| panic!("generate vscode files: {error}"));
		assert!(
			vscode_files
				.iter()
				.any(|file| file.path == Path::new(".vscode/mcp.json"))
		);
		assert_eq!(vscode_files[0].description, "VS Code agent definition");

		let copilot_files = generate_target_files(SubagentTarget::Copilot, true, &mut notes)
			.unwrap_or_else(|error| panic!("generate copilot files: {error}"));
		assert_eq!(
			copilot_files[0].description,
			"GitHub Copilot agent definition"
		);

		let cursor_files = generate_target_files(SubagentTarget::Cursor, true, &mut notes)
			.unwrap_or_else(|error| panic!("generate cursor files: {error}"));
		assert_eq!(
			cursor_files[0].path,
			PathBuf::from(".cursor/rules/monochange.mdc")
		);
		assert_eq!(notes.len(), 1);
		assert!(notes[0].contains("repo-local workspace rule"));

		let plan = SubagentPlan {
			targets: vec![
				SubagentTarget::Claude,
				SubagentTarget::Vscode,
				SubagentTarget::Copilot,
				SubagentTarget::Pi,
				SubagentTarget::Codex,
				SubagentTarget::Cursor,
			],
			files: vec![GeneratedFile {
				path: PathBuf::from(".cursor/rules/monochange.mdc"),
				description: "Cursor workspace rule".to_string(),
				operation: GeneratedFileOperation::Create,
				contents: render_cursor_rule(),
			}],
			notes,
			dry_run: true,
		};
		let output = render_subagent_plan_text(&plan);
		assert!(output.contains("- claude"));
		assert!(output.contains("- vscode"));
		assert!(output.contains("- copilot"));
		assert!(output.contains("- pi"));
		assert!(output.contains("- codex"));
		assert!(output.contains("- cursor"));
		assert!(output.contains("Notes:"));
		assert!(output.contains("Dry run only. No files were written."));

		assert_eq!(subagent_target_name(SubagentTarget::Claude), "claude");
		assert_eq!(subagent_target_name(SubagentTarget::Vscode), "vscode");
		assert_eq!(subagent_target_name(SubagentTarget::Copilot), "copilot");
		assert_eq!(subagent_target_name(SubagentTarget::Pi), "pi");
		assert_eq!(subagent_target_name(SubagentTarget::Codex), "codex");
		assert_eq!(subagent_target_name(SubagentTarget::Cursor), "cursor");
		assert!(
			render_vscode_mcp_config()
				.unwrap_or_else(|error| panic!("render vscode mcp config: {error}"))
				.contains("\"servers\"")
		);
	}
}
