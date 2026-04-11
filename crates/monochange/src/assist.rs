use std::fmt::Write as _;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use serde_json::json;

use crate::AssistOutputFormat;
use crate::Assistant;

pub(crate) fn assistant_display_name(assistant: Assistant) -> &'static str {
	match assistant {
		Assistant::Generic => "Generic MCP client",
		Assistant::Claude => "Claude",
		Assistant::Cursor => "Cursor",
		Assistant::Copilot => "GitHub Copilot",
		Assistant::Pi => "Pi",
	}
}

pub(crate) fn assistant_setup_payload(assistant: Assistant) -> serde_json::Value {
	let mcp_config = json!({
		"mcpServers": {
			"monochange": {
				"command": "monochange",
				"args": ["mcp"]
			}
		}
	});
	let guidance = vec![
		"Read `monochange.toml` before proposing release workflow changes.".to_string(),
		"Run `mc validate` before and after release-affecting edits.".to_string(),
		"Use `mc discover --format json` to inspect package ids, group ownership, and dependency edges.".to_string(),
		"Prefer `mc change` plus `.changeset/*.md` files over ad hoc release notes when encoding release intent.".to_string(),
		"Use `mc release --dry-run --format json` before any mutating release command or source-provider publish flow.".to_string(),
	];
	let notes = match assistant {
		Assistant::Generic => vec![
			"Add the MCP snippet to any client that supports stdio MCP servers.".to_string(),
			"Install `@monochange/skill` when you want a reusable local skill bundle with the same repo guidance.".to_string(),
		],
		Assistant::Claude => vec![
			"Add the MCP snippet to Claude's MCP configuration and keep the repo-local guidance in project instructions.".to_string(),
			"Use the skill package as a reviewable source of guidance rather than embedding one-off release instructions in each chat.".to_string(),
		],
		Assistant::Cursor => vec![
			"Configure the MCP server in Cursor at the workspace or user level.".to_string(),
			"Pair MCP with repo instructions so Cursor agents validate and dry-run release changes before editing manifests or changelogs.".to_string(),
		],
		Assistant::Copilot => vec![
			"Use this MCP snippet in Copilot or VS Code environments that support MCP-compatible server definitions.".to_string(),
			"Keep the guidance close to workspace instructions so Copilot follows monochange's explicit changeset and dry-run workflow.".to_string(),
		],
		Assistant::Pi => vec![
			"Install `@monochange/skill` and copy the bundled files into your Pi skills directory when you want reusable monochange-specific instructions.".to_string(),
			"Configure Pi to run `monochange mcp` so agents can inspect the workspace model, create changesets, and preview releases through MCP tools.".to_string(),
		],
	};

	json!({
		"assistant": assistant_display_name(assistant),
		"strategy": {
			"type": "official-profile",
			"scope": "config-snippets-guidance-install",
			"summary": "monochange ships official assistant setup guidance with install steps, MCP config, and repo-local workflow rules."
		},
		"install": {
			"cli": {
				"npm": "npm install -g @monochange/cli",
				"cargo": "cargo install monochange"
			},
			"skill": {
				"npm": "npm install -g @monochange/skill",
				"copy": "monochange-skill --copy ~/.pi/agent/skills/monochange"
			}
		},
		"mcp_config": mcp_config,
		"repo_guidance": guidance,
		"notes": notes,
	})
}

pub(crate) fn run_assist(
	assistant: Assistant,
	format: AssistOutputFormat,
) -> MonochangeResult<String> {
	let payload = assistant_setup_payload(assistant);
	match format {
		AssistOutputFormat::Json => {
			serde_json::to_string_pretty(&payload)
				.map_err(|error| MonochangeError::Config(error.to_string()))
		}
		AssistOutputFormat::Text => {
			let mcp_config = serde_json::to_string_pretty(&payload["mcp_config"])
				.map_err(|error| MonochangeError::Config(error.to_string()))?;
			let install = serde_json::to_string_pretty(&payload["install"])
				.map_err(|error| MonochangeError::Config(error.to_string()))?;
			let mut output = String::new();
			let _ = writeln!(output, "monochange assist");
			let _ = writeln!(output);
			let _ = writeln!(
				output,
				"Assistant                 {}",
				payload["assistant"].as_str().unwrap_or_default()
			);
			let _ = writeln!(
				output,
				"Strategy                  {}",
				payload["strategy"]["summary"].as_str().unwrap_or_default()
			);
			let _ = writeln!(output);
			let _ = writeln!(output, "Install:");
			let _ = writeln!(output, "{install}");
			let _ = writeln!(output);
			let _ = writeln!(output, "MCP config snippet:");
			let _ = writeln!(output, "{mcp_config}");
			let _ = writeln!(output);
			let _ = writeln!(output, "Suggested repo-local guidance:");
			for item in payload["repo_guidance"].as_array().into_iter().flatten() {
				if let Some(text) = item.as_str() {
					let _ = writeln!(output, "- {text}");
				}
			}
			let _ = writeln!(output);
			let _ = writeln!(output, "Notes for {}:", assistant_display_name(assistant));
			for item in payload["notes"].as_array().into_iter().flatten() {
				if let Some(text) = item.as_str() {
					let _ = writeln!(output, "- {text}");
				}
			}
			Ok(output.trim_end().to_string())
		}
	}
}
