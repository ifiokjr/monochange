# Assistant setup and MCP

MonoChange ships two assistant-facing surfaces:

- `mc assist <assistant>` prints install instructions, MCP configuration, and repo-local guidance
- `mc mcp` starts a stdio MCP server so assistants can call MonoChange tools directly

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
- repo-local guidance for how agents should use MonoChange
- assistant-specific notes

## MCP configuration

Typical client configuration:

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

Start the server manually with:

```bash
mc mcp
```

## Recommended repo-local guidance

Keep instructions like these close to your project guidance:

- Read `monochange.toml` before proposing release workflow changes.
- Run `mc validate` before and after release-affecting edits.
- Use `mc discover --format json` to inspect package ids, group ownership, and dependency edges.
- Use `mc diagnostics --format json` to get a structured view of all pending changesets with git and review provenance — useful for auditing what has changed before a release or PR review.
- Prefer `mc change` plus `.changeset/*.md` files over ad hoc release notes.
- Use `mc release --dry-run --format json` before mutating release state.

## Current MCP tools

The first MCP slice is JSON-first and focuses on reviewable operations:

- `monochange_validate`
- `monochange_discover`
- `monochange_change`
- `monochange_release_preview`
- `monochange_release_manifest`
- `monochange_verify_changesets`

These tools are designed to help assistants inspect the workspace, write explicit release intent, and preview release effects before a human or CI system performs mutating follow-up commands.

When you need full changeset provenance — introduced commit, linked PR, related issues — use `mc diagnostics --format json` directly. It returns stable workspace-relative paths and structured records that agents can parse without reading raw markdown files.
