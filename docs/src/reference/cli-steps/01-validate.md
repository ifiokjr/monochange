# `Validate`

## What it does

`Validate` runs monochange's repository validation without preparing a release.

It checks the current workspace configuration, package and group rules, and authored changesets. The goal is to fail early when the repository is in a state that would make later commands unreliable.

## Why use it

Use `Validate` when you want a cheap, deterministic gate before any workflow that depends on a healthy monochange model.

It is especially useful for:

- local preflight checks before authoring or releasing
- CI jobs that should fail before spending time on planning or publication
- custom commands that should refuse to continue when config or changesets are invalid

Compared with a shell-only `Command` step that runs `mc validate`, the built-in `Validate` step is preferable when you want the command definition to stay provider-neutral and semantically typed.

## Inputs

`Validate` does not accept any built-in step inputs.

## Step-level `when` condition

All CLI steps support an optional `when = "..."` condition.

If the expression resolves to false at runtime, monochange skips the step and continues with the next step.

```toml
when = "{{ inputs.enabled }}"
```

## Prerequisites

None. `Validate` is standalone.

## Side effects and outputs

**Structural checks** (always run):

- validates workspace config syntax and required fields
- validates package and group declarations, membership rules, and namespace collisions
- validates CLI command definitions, step types, and input schemas
- validates changeset files reference declared packages and groups
- validates Cargo workspace version-group constraints

**Content checks** (verify files on disk):

- versioned file paths that are not globs must resolve to an existing file
- ecosystem-typed versioned files (Cargo.toml, package.json, deno.json, pubspec.yaml) must contain a readable version field
- regex versioned file patterns must match at least once in the target file
- glob patterns that match zero files produce a non-fatal warning printed to stderr

**Security checks:**

- `[source].api_url` and `[source].host` must use `https://`; insecure `http://` schemes are rejected to prevent cleartext token transmission

`Validate` returns a normal success/failure result for the command and does not prepare release state for later steps.

When validation fails, monochange renders the offending file path and line/column first, then shows a focused source snippet plus a fix hint when one is available. That makes malformed changesets and config entries much faster to correct from CI logs or local terminal output.

That last point matters: `Validate` is a gate, not a state-producing step.

## When to place it in a workflow

Put `Validate` first when a later `Command` step would otherwise run expensive tooling or provider calls.

Typical pattern:

1. `Validate`
2. `Command` for extra project-specific checks
3. maybe another standalone step such as `AffectedPackages`

## Example

<!-- {=cliStepValidateExample} -->

```toml
[cli.validate]
help_text = "Validate monochange configuration and changesets"

[[cli.validate.steps]]
type = "Validate"
```

<!-- {/cliStepValidateExample} -->

## Composition ideas

### Validate before custom project checks

```toml
[cli.preflight]
help_text = "Validate monochange state and then run project checks"

[[cli.preflight.steps]]
type = "Validate"

[[cli.preflight.steps]]
type = "Command"
command = "cargo test --workspace --all-features"
shell = true
```

### Validate before authoring workflows

If your team uses a custom `change` wrapper command, put `Validate` before any custom `Command` step that derives package lists or reads repo metadata. That keeps the repository model stable before you generate new artifacts.

## Good fit / bad fit

**Good fit:**

- fast CI gates
- local `pre-release` checks
- repo health checks before other steps

**Bad fit:**

- anything that needs release outputs such as `release.*`
- anything that should mutate files or provider state

## Common mistake

Do not expect `Validate` to make `PrepareRelease` unnecessary. It only checks whether the repository is valid; it does not compute the release state that publication-oriented steps need.
