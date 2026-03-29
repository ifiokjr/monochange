# CLI Contract: Release Planning Foundation

## Purpose

Define the user-facing command contract for workflow-defined top-level commands covering workspace validation, discovery, change capture, and release preparation.

## Command 1: Workspace Validation

```bash
mc validate
monochange validate
```

### Behavior

- validates `monochange.toml`
- validates `.changeset/*.md`
- reports configured package/group id mistakes with source-aware diagnostics
- does not modify repository files

## Command 2: Workspace Discovery

```bash
mc discover --format <text|json>
monochange discover --format <text|json>
```

### Behavior

- discovers supported packages from native workspaces and standalone manifests
- produces a unified view of packages, dependency edges, configured groups, and warnings
- does not modify repository files

## Command 3: Change File Creation

```bash
mc change --package <id>... --reason <text> [--bump <patch|minor|major>] [--evidence <value>...] [--output <path>]
monochange change --package <id>... --reason <text> [--bump <patch|minor|major>] [--evidence <value>...] [--output <path>]
```

### Behavior

- requires one or more configured package ids or group ids
- records a markdown changeset file under `.changeset/` by default
- defaults `--bump` to `patch`
- supports optional compatibility evidence strings and explicit output paths

## Command 4: Release Planning and Preparation

```bash
mc release [--dry-run] [--format <text|json>]
monochange release [--dry-run] [--format <text|json>]
```

### Behavior

- loads workflows from `monochange.toml` and dispatches them as top-level commands
- auto-discovers `.changeset/*.md` under the repository root
- `--dry-run` performs planning and rendering only without mutating files
- updates native manifests plus configured changelogs and `versioned_files` during non-dry-run execution
- applies group release identity precedence for `tag`, `release`, and `version_format`
- deletes consumed changesets only after a fully successful non-dry-run execution

## Workflow Surface Rules

- repositories may define custom top-level commands through `[[workflows]]`
- when `[[workflows]]` is omitted, MonoChange synthesizes `validate`, `discover`, `change`, and `release`
- workflow-declared inputs become CLI flags
- all workflow commands implicitly support `--help` and `--dry-run`
- `init`, `help`, and `version` remain reserved built-ins

## Text Output Requirements

- identify the workflow name
- indicate whether execution was a dry-run
- report release targets with effective tag/release metadata when applicable
- list released packages and changed files when applicable
- show command-step execution summaries when workflow commands run
