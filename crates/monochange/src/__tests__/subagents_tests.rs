#![allow(clippy::disallowed_methods)]
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
	let workspace = setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), "subagents/basic");
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
	let workspace = setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), "subagents/basic");
	let contents = fixture_readme_contents();
	let plan = SubagentPlan {
		targets: vec![SubagentTarget::Claude],
		files: vec![
			GeneratedFile {
				path: PathBuf::from("README.md"),
				description: "README".to_string(),
				operation: GeneratedFileOperation::Overwrite,
				contents: contents.clone(),
			},
			GeneratedFile {
				path: PathBuf::from(".mcp.json"),
				description: "MCP".to_string(),
				operation: GeneratedFileOperation::Overwrite,
				contents: contents.clone(),
			},
		],
		notes: Vec::new(),
		dry_run: false,
	};
	let error = write_subagent_plan(workspace.path(), &plan, false)
		.err()
		.unwrap_or_else(|| panic!("expected overwrite conflict"));
	assert_eq!(
		error.to_string(),
		"config error: refusing to overwrite existing subagent files without --force: README.md, .mcp.json"
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
	let claude_files_without_mcp = generate_target_files(SubagentTarget::Claude, false, &mut notes)
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
