---
name: monochange
description: Use the monochange CLI and MCP tooling to configure monorepo package versioning, create changesets, preview releases, generate versioned package files, and run configured publish workflows. Use when working with monochange.toml, .changeset/*.md, [cli.*] workflow commands, package/group version plans, manifest linting, release records, or the monochange MCP server.
---

# monochange

Use this skill when the user wants to operate monochange in a repository or author a `monochange.toml` configuration for versioned package releases.

monochange is a release-planning harness rather than a single fixed workflow. It discovers package manifests, maps them to configured package and group ids, reads `.changeset/*.md` release intent, computes versions, updates native manifests and extra versioned files, and then exposes release, source-provider, and package-publishing actions through built-in steps or repository-defined workflows.

Agents should optimize for safety and traceability: inspect config first, prefer JSON/dry-run output while planning, preserve changeset intent in files, and only run mutating release or publish flows after the user has approved the exact command path.

## Source-of-truth rules

- Read `monochange.toml` before recommending commands. Top-level `mc <name>` workflow commands can be user-defined by `[cli.<name>]` and vary per repository.
- Do not assume `mc discover`, `mc change`, `mc release`, `mc publish`, or similar workflow names exist in every repo. They are user-defined unless they appear in `mc help` for that workspace.
- Binary commands are wired by the CLI. Step commands are always exposed as `mc step:<step-name>` for built-in step variants, except the generic `Command` step.
- When authoring `[cli.*]` workflows, command inputs are explicit per step. Add `inputs = ["name"]` on a step to inherit a command input unchanged, or use the map form for overrides and renamed values.
- Prefer package or group ids from `monochange.toml` over manifest names.
- Use dry-run or preview commands before mutating versions, committing, tagging, releasing, or publishing.
- Never publish with local credentials on behalf of a user unless they explicitly own that operation and the project rules allow it.

## Fast workflow

1. Inspect configuration: `mc step:validate`, `mc step:config --format json`, or `mc help`. Use this to learn package ids, enabled ecosystems, groups, and which top-level workflow commands actually exist.
2. Inspect packages: use the configured workflow command (often `mc discover --format json`) or `mc step:discover --format json`. Prefer JSON when another tool or agent will consume the package graph.
3. Create release intent: use a configured workflow command (often `mc change ...`) or write `.changeset/*.md` manually. Read existing changesets first so you can update or merge related intent instead of creating duplicates.
4. Preview versioned files: use the configured workflow command (often `mc release --dry-run --format json` or `--diff`) or `mc step:prepare-release --dry-run`. The preview is where you verify versions, changelog entries, generated manifests, and lockfile work before mutating the tree.
5. Run validation and linting: `mc check` and `mc step:validate`. `validate` catches monochange configuration and target issues; `check` also runs manifest lint rules.
6. Only after review, run configured commit/release/publish workflows. Keep release-record, readiness, bootstrap, plan, and publish artifacts when the workflow emits them.

## What to open next

- [skills/readme.md](skills/readme.md) — index of all focused skill modules.
- [skills/commands.md](skills/commands.md) — verified built-in commands, step commands, user-defined command behavior, and all CLI step types.
- [skills/configuration.md](skills/configuration.md) — current `monochange.toml` structure and examples.
- [skills/changesets.md](skills/changesets.md) — changeset file shape, CLI creation, and lifecycle rules.
- [skills/linting.md](skills/linting.md) — `mc check`, lint presets, and manifest policy.
- [skills/multi-package-publishing.md](skills/multi-package-publishing.md) — readiness, bootstrap, and package publishing flows.
- [skills/trusted-publishing.md](skills/trusted-publishing.md) — registry trust/OIDC notes for publishing.
- [skills/reference.md](skills/reference.md) — full operating guide.
- [examples/readme.md](examples/readme.md) — copyable example scenarios.

## Verified command inventory

The command inventory in this skill is based on `crates/monochange/src/cli.rs`, `crates/monochange_core/src/lib.rs`, and the CLI help snapshot `crates/monochange/tests/snapshots/cli_help__help_overview_lists_all_commands@help_overview_lists_all_commands.snap`.

Built-in binary and step commands worth remembering:

- `mc init` — create a starter `monochange.toml` from discovered manifests.
- `mc populate` — add missing configurable workflow definitions to an existing config.
- `mc skill` — install or update the monochange skill bundle.
- `mc subagents` — generate repository-local agent/subagent guidance for monochange work.
- `mc analyze` — inspect semantic changes for a package.
- `mc step:tag-release` — create release tags from an embedded release record.
- `mc step:release-record` — inspect the release record reachable from a tag or commit.
- `mc check` — validate configuration, changesets, and manifest lint rules.
- `mc lint` — list or explain lint rules and presets.
- `mc mcp` — run the stdio MCP server.
- `mc step:validate` — validate `monochange.toml` and changeset targets.
- `mc step:publish-readiness` — verify publishability from a release record without publishing.
- `mc step:placeholder-publish` — publish first-time placeholder versions for packages in a release record.

Built-in step commands:

- `mc step:config`
- `mc step:validate`
- `mc step:discover`
- `mc step:display-versions`
- `mc step:create-change-file`
- `mc step:prepare-release`
- `mc step:commit-release`
- `mc step:verify-release-branch`
- `mc step:publish-release`
- `mc step:placeholder-publish`
- `mc step:release-record`
- `mc step:publish-readiness`
- `mc step:publish-packages`
- `mc step:plan-publish-rate-limits`
- `mc step:open-release-request`
- `mc step:tag-release`
- `mc step:comment-released-issues`
- `mc step:affected-packages`
- `mc step:diagnose-changesets`
- `mc step:retarget-release`

`Command` is a valid `[cli.*].steps[].type` for running shell commands, but it is not exposed as `mc step:command`.

## Current MCP tools

The `mc mcp` server exposes these tools:

- `monochange_validate` — validate config and changeset targets.
- `monochange_discover` — return structured packages, groups, dependencies, and ecosystems.
- `monochange_diagnostics` — inspect pending changesets with git and review context.
- `monochange_change` — create a changeset through structured tool input.
- `monochange_release_preview` — run a dry-run release preview.
- `monochange_release_manifest` — produce a release manifest payload for downstream automation.
- `monochange_affected_packages` — evaluate changed paths and changeset coverage.
- `monochange_lint_catalog` — list lint rules and presets.
- `monochange_lint_explain` — explain one lint rule or preset.
- `monochange_analyze_changes` — inspect semantic diffs for package-aware changes.
- `monochange_validate_changeset` — check one changeset against the current semantic diff.

Prefer MCP tools when the caller needs structured data and the shell when you need to run the exact repository workflow that maintainers use locally or in CI.
