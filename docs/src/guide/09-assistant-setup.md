# Assistant setup and MCP

monochange ships two assistant-facing surfaces:

- `mc assist <assistant>` prints install instructions, MCP configuration, and repo-local guidance
- `mc mcp` starts a stdio MCP server so assistants can call monochange tools directly

## Install the CLI and skill

Install the CLI:

```bash
npm install -g @monochange/cli
monochange --help
mc --help
```

Install the bundled skill:

```bash
npm install -g @monochange/skill
monochange-skill --print-install
monochange-skill --copy ~/.pi/agent/skills/monochange
```

<!-- {=assistantSkillBundleContents} -->

After copying the bundled skill, you get a small documentation set that is designed to load in layers:

- `SKILL.md` — concise entrypoint for agents
- `REFERENCE.md` — broader high-context reference with more examples
- `skills/README.md` — index of focused deep dives
- `skills/adoption.md` — setup-depth questions, migration guidance, and recommendation patterns
- `skills/changesets.md` — changeset authoring and lifecycle guidance
- `skills/commands.md` — built-in command catalog and workflow selection
- `skills/configuration.md` — `monochange.toml` setup and editing guidance
- `skills/linting.md` — `[lints]` presets, `mc check`, and manifest-focused examples
- `examples/README.md` — condensed scenario examples for quick recommendations

This layout keeps the top-level skill small while still making the richer guidance available when an assistant needs more context.

<!-- {/assistantSkillBundleContents} -->

## Print an assistant profile

Examples:

```bash
mc assist generic
mc assist pi
mc assist claude --format json
```

The profile includes:

- install commands for `@monochange/cli` and `@monochange/skill`
- an MCP server config snippet that runs `monochange mcp`
- repo-local guidance for how agents should use monochange
- assistant-specific notes

## MCP configuration

Typical client configuration:

<!-- {=mcpConfigSnippet} -->

```json
{
	"mcpServers": {
		"monochange": {
			"command": "monochange",
			"args": ["mcp"]
		}
	}
}
```

<!-- {/mcpConfigSnippet} -->

Start the server manually with:

```bash
mc mcp
```

## Recommended repo-local guidance

Keep instructions like these close to your project guidance:

<!-- {=assistantRepoGuidance} -->

- Read `monochange.toml` before proposing release workflow changes.
- Run `mc validate` before and after release-affecting edits.
- Use `mc discover --format json` to inspect package ids, group ownership, and dependency edges.
- Use `mc diagnostics --format json` or `monochange_diagnostics` for a structured view of all pending changesets with git and review context.
- Use `monochange_lint_catalog` and `monochange_lint_explain` when you need lint metadata without shelling out.
- Prefer `mc change` plus `.changeset/*.md` files over ad hoc release notes.
- Use `mc release --dry-run --format json` before mutating release state.

<!-- {/assistantRepoGuidance} -->

## Current MCP tools

The MCP server is JSON-first and focuses on reviewable operations:

<!-- {=mcpToolsList} -->

- `monochange_validate` — validate `monochange.toml` and `.changeset` targets
- `monochange_discover` — discover packages, dependencies, and groups across the repository
- `monochange_diagnostics` — inspect pending changesets with git and review context as structured JSON
- `monochange_change` — write a `.changeset` markdown file for one or more package or group ids
- `monochange_release_preview` — prepare a dry-run release preview from discovered `.changeset` files
- `monochange_release_manifest` — generate a dry-run release manifest JSON document for downstream automation
- `monochange_affected_packages` — evaluate changeset policy from changed paths and optional labels
- `monochange_lint_catalog` — list registered manifest lint rules and presets
- `monochange_lint_explain` — explain one manifest lint rule or preset
- `monochange_analyze_changes` — analyze git diff state and return ecosystem-specific semantic changes
- `monochange_validate_changeset` — validate one changeset against the current semantic diff

<!-- {/mcpToolsList} -->

These tools are designed to help assistants inspect the workspace, write explicit release intent, and preview release effects before a human or CI system performs mutating follow-up commands.

`monochange_analyze_changes` and `monochange_validate_changeset` now provide semantic analysis across **Cargo, npm, Deno, and Dart/Flutter** packages. They surface ecosystem-specific evidence such as Rust public API diffs, JS/TS export changes, `package.json` and `deno.json` export metadata, and `pubspec.yaml` dependency or plugin-platform changes, then validate authored changesets against that semantic model.

When you need full changeset context — introduced commit, linked PR, related issues — use `mc diagnostics --format json` directly. It returns stable workspace-relative paths and structured records that agents can parse without reading raw markdown files.
