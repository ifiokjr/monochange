# monochange reference

## What monochange is for

monochange manages versions and releases for monorepos that span more than one package ecosystem.

Use it when a repository needs one release-planning model across:

- Cargo
- npm / pnpm / Bun
- Deno
- Dart / Flutter

It discovers packages, normalizes dependency relationships, applies package and group rules from `monochange.toml`, reads explicit `.changeset/*.md` files, and turns those inputs into deterministic release plans.

## Installation

### CLI via npm

```bash
npm install -g @monochange/cli
monochange --help
mc --help
```

### CLI via Cargo

```bash
cargo install monochange
monochange --help
mc --help
```

### Skill package

```bash
npm install -g @monochange/skill
monochange-skill --print-install
```

The `@monochange/skill` package should provide a helper that can print or copy the bundled skill files for agent setups.

## Recommended command flow

### Validate first

```bash
mc validate
```

Run this before changing config, changesets, or release workflows.

### Discover the workspace model

```bash
mc discover --format json
```

Use this to inspect configured package ids, inferred packages, paths, dependency edges, and group ownership.

### Create explicit release intent

```bash
mc change --package monochange --bump minor --reason "describe the change"
```

Prefer explicit change files over ad hoc notes. Keep `.changeset/*.md` aligned with configured package or group ids.

### Preview release effects

```bash
mc release --dry-run --format json
```

Use dry-run output before real release preparation. Review:

- release targets
- propagated bumps
- changelog output
- deleted changesets
- changed files

### Generate downstream automation input

```bash
mc release --dry-run --format json
```

Use this when downstream CI or source-provider automation needs stable machine-readable release data.

## Important modeling rules

- `monochange.toml` is the source of truth.
- Groups own outward release identity for their member packages.
- Package changelogs and package versioned files may still apply even when a group owns versioning.
- Changesets should reference configured package ids or group ids.
- Source-provider release publishing is downstream from prepared release data, not a substitute for planning.

## Assistant setup guidance

Recommended repo-local guidance for agents:

- Read `monochange.toml` before proposing release changes.
- Prefer `mc validate` before and after config edits.
- Use `mc discover --format json` to verify package ids and group ownership.
- Use `mc release --dry-run --format json` before mutating release state.
- Keep docs, templates, changelog behavior, and CLI config examples synchronized with code changes.

## MCP setup target

The planned MCP setup should look like:

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

The initial MCP server should focus on safe, reviewable tools such as validation, discovery, changeset verification, release preview, and release-manifest generation.
