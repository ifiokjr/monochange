# CLI Contract: Release Planning Foundation

## Purpose

Define the user-facing command contract for workspace discovery, validation, release planning, and workflow-driven release preparation.

## Command 1: Workspace Validation

```bash
mc check --root <path>
monochange check --root <path>
```

### Behavior

- validates `monochange.toml`
- validates `.changeset/*.md`
- reports configured package/group id mistakes with source-aware diagnostics
- does not modify repository files

## Command 2: Workspace Discovery

```bash
mc workspace discover --root <path> --format <text|json>
```

### Behavior

- discovers supported packages from native workspaces and standalone manifests
- produces a unified view of packages, dependency edges, configured groups, and warnings
- does not modify repository files

## Command 3: Release Plan Generation

```bash
mc plan release --root <path> --changes <path> --format <text|json>
```

### Behavior

- reads explicit markdown changeset input
- resolves configured package ids or group ids
- expands group-targeted changesets into package-level signals
- calculates release impact through direct and transitive dependency edges
- applies configured-group synchronization before finalizing output
- includes compatibility evidence when a provider escalates severity

## Command 4: Workflow-Driven Release Preparation

```bash
mc release --root <path> [--dry-run]
monochange release --root <path> [--dry-run]
```

### Behavior

- loads workflows from `monochange.toml` and dispatches them as top-level commands
- auto-discovers `.changeset/*.md` under the repository root
- updates native manifests plus configured changelogs and `versioned_files`
- applies group release identity precedence for `tag`, `release`, and `version_format`
- deletes consumed changesets only after a fully successful non-dry-run execution
- in `--dry-run`, performs planning and rendering only and does not mutate files

## Text Output Requirements

- identify the workflow name
- indicate whether execution was a dry-run
- report release targets with effective tag/release metadata
- list released packages and changed files when applicable
- show command-step execution summaries when workflow commands run
