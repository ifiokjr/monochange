# monochange assistant distribution design

## Goal

Explore a monochange assistant surface modeled after the latest `mdt` patterns:

1. ship a portable MCP server entrypoint via `monochange mcp`
2. ship a lightweight assistant setup command similar to `mdt assist`
3. publish the CLI to npm as `@monochange/cli`
4. publish a reusable agent skill to npm as `@monochange/skill`
5. document how agents should use monochange and how users should install it

## Reference points from `mdt`

### MCP and assistant setup

The latest `mdt` implementation uses two complementary entrypoints:

- `mdt mcp` starts a stdio MCP server
- `mdt assist <assistant>` prints:
  - an MCP config snippet
  - repo-local guidance
  - assistant-specific notes

Key files reviewed:

- `/Users/ifiokjr/Developer/projects/mdt/mdt_cli/src/lib.rs`
- `/Users/ifiokjr/Developer/projects/mdt/mdt_cli/src/main.rs`
- `/Users/ifiokjr/Developer/projects/mdt/mdt_mcp/src/lib.rs`
- `/Users/ifiokjr/Developer/projects/mdt/docs/src/getting-started/assistant-setup.md`
- `/Users/ifiokjr/Developer/projects/mdt/docs/src/reference/cli.md`

### npm packaging

`mdt` publishes prebuilt binaries to npm by:

- building GitHub release assets per target in `release.yml`
- emitting a small metadata artifact containing the release tag
- running `npm-publish.yml` after `release.yml`
- repackaging the exact GitHub release binaries into:
  - one root npm package
  - several platform packages used through `optionalDependencies`

Key files reviewed:

- `/Users/ifiokjr/Developer/projects/mdt/.github/workflows/npm-publish.yml`
- `/Users/ifiokjr/Developer/projects/mdt/.github/workflows/release.yml`
- `/Users/ifiokjr/Developer/projects/mdt/scripts/npm/build-packages.mjs`
- `/Users/ifiokjr/Developer/projects/mdt/scripts/npm/publish-packages.mjs`
- `/Users/ifiokjr/Developer/projects/mdt/npm/bin/mdt.js`

## Proposed monochange adaptation

### 1. Built-in assistant entrypoints

Add two first-class built-ins outside config-defined `[cli.<command>]` entries:

- `monochange assist <assistant>` / `mc assist <assistant>`
- `monochange mcp` / `mc mcp`

This mirrors `mdt`:

- `assist` lowers setup friction
- `mcp` provides the actual protocol surface

### 2. Initial `assist` output

`monochange assist` should print:

- MCP config that runs `monochange mcp`
- installation commands for `@monochange/cli` and `@monochange/skill`
- repo-local guidance describing how monochange should be used
- assistant-specific notes for `generic`, `claude`, `cursor`, `copilot`, and `pi`

Suggested JSON shape:

```json
{
  "assistant": "Pi",
  "strategy": {
    "type": "official-profile",
    "scope": "config-snippets-guidance-install",
    "summary": "monochange ships official assistant setup profiles with MCP config, install steps, and repo guidance."
  },
  "install": {
    "cli": "npm install -g @monochange/cli",
    "skill": "npm install -g @monochange/skill"
  },
  "mcp_config": {
    "mcpServers": {
      "monochange": {
        "command": "monochange",
        "args": ["mcp"]
      }
    }
  },
  "repo_guidance": [...],
  "notes": [...]
}
```

### 3. Initial MCP scope

The first MCP slice should stay narrow and reuse existing stable CLI/library behavior.

Recommended tools:

- `monochange_validate`
- `monochange_discover`
- `monochange_change`
- `monochange_release_preview`
- `monochange_release_manifest`
- `monochange_verify_changesets`

Guiding rule:

- expose deterministic, reviewable operations first
- prefer dry-run and preview tools before mutating tools
- avoid source-provider publish side effects in the first MCP slice unless explicitly invoked and clearly named

### 4. Skill package shape

Publish a separate package named `@monochange/skill`.

Recommended contents:

- `SKILL.md` — concise trigger-oriented agent instructions
- `REFERENCE.md` — deeper guidance and examples
- `README.md` — human install and usage docs
- `bin/monochange-skill.js` — helper that can:
  - print install instructions
  - print the bundled skill
  - copy the bundled skill files into a target directory

Recommended default install story:

```bash
npm install -g @monochange/skill
monochange-skill --print-install
monochange-skill --copy ~/.pi/agent/skills/monochange
```

### 5. CLI npm package shape

Publish a separate root package named `@monochange/cli` modeled on `mdt`.

Recommended root package behavior:

- platform-specific optional dependencies
- one JS launcher that resolves the correct prebuilt binary
- expose both `monochange` and `mc` bins from the root package

Recommended platform package naming:

- `@monochange/cli-linux-arm64-gnu`
- `@monochange/cli-linux-arm64-musl`
- `@monochange/cli-darwin-arm64`
- `@monochange/cli-linux-x64-gnu`
- `@monochange/cli-linux-x64-musl`
- `@monochange/cli-darwin-x64`
- `@monochange/cli-win32-x64-msvc`
- `@monochange/cli-win32-arm64-msvc`

### 6. Release flow

monochange does not use `knope`, but the distribution pattern from `mdt` still applies.

Recommended workflow split:

- `release.yml`
  - trigger on GitHub release creation or manual dispatch
  - build target binaries for the `monochange` binary
  - upload release archives and checksums
  - publish a metadata artifact containing the release tag
- `npm-publish.yml`
  - trigger from successful `release.yml`
  - download release assets
  - build `@monochange/cli` root and platform packages
  - publish npm packages in safe order
  - optionally also publish `@monochange/skill` if the package version matches the tag

## Proposed skill guidance

The skill should teach agents to:

- start from `monochange.toml` and configured `[cli.<command>]` entries
- prefer `mc validate` before making release-affecting changes
- use `mc discover --format json` to inspect the normalized workspace model
- use `mc change` to author `.changeset/*.md` files rather than hand-writing ad hoc formats when possible
- use `mc release --dry-run --format json` before any mutating release run
- understand that groups own outward version identity while package changelogs and versioned files may still apply
- treat source-provider publishing as a follow-up to prepared release data, not as the first step

## Suggested implementation order

1. add exploration docs and bundled skill content
2. add `assist` command
3. add `@monochange/skill` package and helper script
4. add npm packaging scripts for `@monochange/cli`
5. add release and npm-publish workflows
6. add `monochange_mcp` crate and `mcp` subcommand
7. expand docs and assistant-specific examples

## Open questions

1. Should `@monochange/skill` be versioned independently or always match the CLI version?
2. Should npm publishing for `@monochange/skill` happen in the same workflow as `@monochange/cli` or in a separate workflow?
3. Should the initial MCP toolset include mutating operations like `change` and `release`, or only dry-run/inspection tools?
4. Should `monochange assist` live as a built-in command, or should it also be expressible as a config-defined command later?
5. Should the npm launcher expose both `monochange` and `mc`, or only `monochange` with `mc` documented as cargo/homebrew-only?
