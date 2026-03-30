# CLI Commands Config Design

## Decision

Replace the `[[workflows]]` configuration namespace with a command-keyed `[cli.<command>]` namespace.

## Motivation

The current API models a sequence of steps and exposes that sequence as a top-level CLI command. In practice, users experience this as command configuration, not general workflow orchestration.

A command-keyed map makes the config read like the actual product surface:

```toml
[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"
```

This is simpler than:

```toml
[[workflows]]
name = "release"

[[workflows.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[workflows.steps]]
type = "PrepareRelease"
```

## Chosen shape

### Top-level commands

```toml
[cli.validate]
help_text = "Validate monochange configuration and changesets"

[[cli.validate.steps]]
type = "Validate"
```

### Command steps

```toml
[[cli.release.steps]]
type = "Command"
command = "cargo test --workspace --all-features"
dry_run_command = "cargo test --workspace --all-features"
shell = true
```

## Why a map

The command-keyed map fits the current product well because:

- command names are unique
- nested commands are not planned
- command identity is structural instead of duplicated in a `name` field
- TOML paths like `cli.release.inputs` read naturally
- command lookup is efficient and deterministic

## Non-goals

- nested subcommand trees
- generic workflow orchestration beyond CLI commands
- command layering or imports in this change

Future non-command automation can live under separate namespaces such as:

- `[hooks.*]`
- `[automation.*]`

## Behavioral rules

- if no `[cli.<command>]` entries are declared, MonoChange synthesizes `validate`, `discover`, `change`, and `release`
- built-in reserved names remain `init`, `help`, and `version`
- command inputs become CLI flags
- all configured commands implicitly support `--help` and `--dry-run`
- `dry_run_command` replaces `command` only when the command is invoked with `--dry-run`

## Migration policy

The old `[[workflows]]` namespace is rejected with a clear error directing users to `[cli.<command>]`.

## User-facing terminology

Use these terms consistently:

- **CLI command** for a top-level configured command
- **command step** for a typed step inside a command
- avoid calling this surface a workflow unless referring to external CI systems or historical docs
