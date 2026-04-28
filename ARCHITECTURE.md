# Architecture

`monochange` is optimized around a small set of repo-local source-of-truth documents:

- `AGENTS.md` is the table of contents for agent guidance.
- this file is the top-level architecture map.
- `docs/agents/` holds focused workflow, testing, and boundary rules.
- `docs/plans/README.md` explains how multi-step work is planned, tracked, and archived.

## Crate map

### Orchestration layer

- `crates/monochange` is the top-level CLI and library entrypoint.
- It wires commands, MCP tools, release planning, and adapter dispatch together.
- It may orchestrate ecosystem and provider adapters, but it should not absorb adapter-specific parsing, payload shaping, or capability matrices.

### Shared domain layer

- `crates/monochange_core` defines shared domain types, command definitions, release artifacts, and lint contracts.
- `crates/monochange_graph` builds release plans from normalized package and dependency data.
- `crates/monochange_semver` and `crates/monochange_analysis` provide compatibility and semantic-diff evidence.

### Adapter layer

- Ecosystem crates own package-manager-specific behavior:
  - `crates/monochange_cargo`
  - `crates/monochange_npm`
  - `crates/monochange_deno`
  - `crates/monochange_dart`
- Source-provider crates own hosted automation behavior:
  - `crates/monochange_github`
  - `crates/monochange_gitlab`
  - `crates/monochange_gitea`

### Configuration and linting support

- `crates/monochange_config` parses and normalizes `monochange.toml`, then delegates adapter-specific validation when behavior depends on a concrete ecosystem or provider.
- `crates/monochange_lint`, `crates/monochange_linting`, and `crates/monochange_lint_testing` provide the generic lint engine, authoring helpers, and test helpers.

### Packages and assistant surfaces

- `packages/monochange__cli` ships the npm CLI wrapper.
- `packages/monochange__skill` ships the layered assistant skill bundle.

## Placement rules

When adding a feature, decide in this order:

1. Is this a shared concept or contract? Put it in `monochange_core`.
2. Is it a planning or graph concern? Put it in `monochange_graph` or `monochange_analysis`.
3. Is it ecosystem-specific? Put it in the relevant ecosystem crate.
4. Is it provider-specific? Put it in the relevant source crate.
5. Is it only command orchestration? Keep it in `crates/monochange`.

## Explicit dispatch points

Direct `SourceProvider` and `EcosystemType` branching is intentionally concentrated in a small number of files.

### Provider dispatch

These files are the current reviewed exceptions inside the orchestration/config layers:

- `crates/monochange/src/hosted_sources.rs`
- `crates/monochange/src/release_artifacts.rs`
- `crates/monochange/src/release_branch_policy.rs`
- `crates/monochange/src/release_record.rs`
- `crates/monochange/src/workspace_ops.rs`
- `crates/monochange/src/package_publish.rs`
- `crates/monochange_config/src/lib.rs`

### Ecosystem dispatch

These files are the current reviewed exceptions inside the orchestration/config layers:

- `crates/monochange/src/versioned_files.rs`
- `crates/monochange/src/workspace_ops.rs`
- `crates/monochange_config/src/lib.rs`

New exceptions should be rare. If one is necessary, document it here and update the architecture check in the same change.

## Feedback loops and legibility

`monochange` treats machine-readable outputs as first-class APIs for agents and CI.

- prefer `--format json` surfaces for automation
- prefer MCP tools over shell scraping when a stable tool exists
- keep docs aligned with the MCP surface and command surface
- keep fixture-first tests and snapshots representative of real workflows

## Plans and long-running work

Complex work should live under `docs/plans/`:

- `docs/plans/active/` for current execution plans
- `docs/plans/completed/` for archived plans
- `docs/plans/tech-debt.md` for recurring cleanup targets and follow-up work

See [docs/plans/README.md](docs/plans/README.md) for the workflow.
