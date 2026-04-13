# `Command`

## What it does

`Command` runs an arbitrary program or shell command from a monochange workflow.

This is the escape hatch step that lets you combine monochange's structured state with the rest of your toolchain.

## Why use it

Use `Command` when you need to:

- run project-specific tooling that monochange does not own
- upload artifacts
- call deployment, chat, or notification tools
- bridge monochange release context into custom scripts
- compose outputs from earlier steps into external automation

The important design rule is this:

> prefer a built-in step whenever monochange already has a first-class semantic for the work.

Use `Command` for what is truly custom.

## Core fields

- `command` — the command to run in normal mode
- `when` — optional boolean condition controlling whether the step runs
- `dry_run_command` — optional replacement command used only when the command runs with `--dry-run`
- `shell` — whether to run through a shell (`true`, `false`, or a custom shell binary name)
- `id` — optional identifier that exposes `steps.<id>.stdout` and `steps.<id>.stderr` to later steps
- `variables` — optional custom variable mapping for command substitution
- `inputs` — optional step-local input overrides
- `show_progress` — optional boolean; set to `false` when the command itself is interactive and spinner output would get in the way

## Prerequisites

`Command` itself has no built-in prerequisite.

What it can see depends on where you place it:

- after `PrepareRelease`, it can consume `release.*` and `manifest.path`
- after `AffectedPackages`, it can consume `affected.*`
- after `RetargetRelease`, it can consume `retarget.*`
- after `CommitRelease`, it can consume `release_commit.*`
- after another named `Command`, it can consume `steps.<id>.*`

## Side effects and outputs

- runs an external command
- records stdout/stderr when `id` is present
- can act as a consumer or producer step in a workflow chain

## Example

<!-- {=cliStepCommandExample} -->

```toml
[cli.test]
help_text = "Run project tests"

[[cli.test.steps]]
type = "Command"
command = "cargo test --workspace --all-features"
dry_run_command = "cargo test --workspace --all-features --no-run"
shell = true
```

<!-- {/cliStepCommandExample} -->

## Composition ideas

### Consume prepared release context

<!-- {=cliStepPrepareReleaseCommandCompositionExample} -->

```toml
[cli.release-with-notes]
help_text = "Prepare a release and print a custom summary"

[[cli.release-with-notes.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-with-notes.steps]]
type = "PrepareRelease"

[[cli.release-with-notes.steps]]
type = "Command"
command = "echo Releasing {{ release.version }} for {{ released_packages }}"
shell = true
```

<!-- {/cliStepPrepareReleaseCommandCompositionExample} -->

### Reuse earlier command output

<!-- {=cliStepCommandStepOutputExample} -->

```toml
[cli.release-with-generated-notes]
help_text = "Prepare a release, generate notes, and upload them"

[[cli.release-with-generated-notes.steps]]
type = "PrepareRelease"

[[cli.release-with-generated-notes.steps]]
type = "Command"
id = "notes"
command = "printf 'version=%s\n' '{{ release.version }}'"
shell = true

[[cli.release-with-generated-notes.steps]]
type = "Command"
command = "printf '%s\n' '{{ steps.notes.stdout }}'"
shell = true
```

<!-- {/cliStepCommandStepOutputExample} -->

### Consume repair state

<!-- {=cliStepRetargetCommandCompositionExample} -->

```toml
[cli.repair-and-notify]
help_text = "Repair a release and print the retarget result"

[[cli.repair-and-notify.inputs]]
name = "from"
type = "string"
required = true

[[cli.repair-and-notify.inputs]]
name = "target"
type = "string"
default = "HEAD"

[[cli.repair-and-notify.steps]]
type = "RetargetRelease"

[[cli.repair-and-notify.steps]]
type = "Command"
command = "echo moved {{ retarget.tags }} to {{ retarget.target }} with status {{ retarget.status }}"
shell = true
```

<!-- {/cliStepRetargetCommandCompositionExample} -->

## Why choose `Command` carefully?

Because it is powerful enough to bypass monochange's typed guarantees.

That is useful, but it also means:

- validation cannot reason deeply about your command string
- provider-aware dry-run semantics are now partly your responsibility
- shell quoting and portability become part of the workflow design

## Recommended usage pattern

A good workflow usually looks like this:

1. use built-in steps to create stable state
2. use `Command` only for the final custom integration points
3. give important custom steps an `id` so later steps can consume structured stdout

## Common mistakes

- using `Command` to reimplement `PublishRelease` or `OpenReleaseRequest`
- forgetting `dry_run_command` when the real command would mutate external systems
- omitting `id` and then having no clean way to reuse the command's output later
- relying on shell features without setting `shell = true` or a custom shell name
