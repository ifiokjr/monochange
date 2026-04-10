# `CreateChangeFile`

## What it does

`CreateChangeFile` writes a `.changeset/*.md` file from typed CLI inputs.

It supports both:

- explicit non-interactive authoring from inputs such as `package`, `bump`, `reason`, and `details`
- interactive authoring when `interactive = true`

## Why use it

Use `CreateChangeFile` when you want monochange itself to remain the source of truth for authored change files.

That gives you a few advantages over rolling your own shell template generator:

- package and group references resolve through the same config model used for release planning
- default bump/type behavior stays aligned with monochange parsing rules
- interactive mode can guide authors instead of forcing them to remember frontmatter details
- the generated file shape stays compatible with `mc validate`, `PrepareRelease`, and diagnostics tooling

## Inputs

- `interactive` — boolean; use interactive prompting instead of explicit package arguments
- `package` — list of package or group ids to target
- `bump` — `none`, `patch`, `minor`, or `major`
- `version` — explicit version pin for the change
- `reason` — summary line
- `type` — optional release-note type
- `details` — optional long-form body
- `output` — optional explicit file path

## Step-level `when` condition

All CLI steps support an optional `when = "..."` condition.

If the expression resolves to false at runtime, monochange skips the step and continues with the next step.

```toml
when = "{{ inputs.enabled }}"
```

## Prerequisites

None. `CreateChangeFile` is standalone.

## Side effects and outputs

- writes a new changeset file
- reports the written path
- does not prepare release state for later steps

## Example

<!-- {=cliStepCreateChangeFileExample} -->

```toml
[cli.change]
help_text = "Create a change file for one or more packages"

[[cli.change.inputs]]
name = "interactive"
type = "boolean"
short = "i"

[[cli.change.inputs]]
name = "package"
type = "string_list"

[[cli.change.inputs]]
name = "bump"
type = "choice"
choices = ["none", "patch", "minor", "major"]
default = "patch"

[[cli.change.inputs]]
name = "reason"
type = "string"

[[cli.change.inputs]]
name = "details"
type = "string"

[[cli.change.steps]]
type = "CreateChangeFile"
```

<!-- {/cliStepCreateChangeFileExample} -->

## Composition ideas

### Non-interactive wrapper for contributors

```toml
[cli.change-fix]
help_text = "Create a patch changeset for one package"

[[cli.change-fix.inputs]]
name = "package"
type = "string_list"
required = true

[[cli.change-fix.inputs]]
name = "reason"
type = "string"
required = true

[[cli.change-fix.steps]]
type = "CreateChangeFile"
inputs = { bump = "patch", package = "{{ inputs.package }}", reason = "{{ inputs.reason }}" }
```

This is a good example of why built-in step inputs matter: the wrapper command is still using `CreateChangeFile` semantics rather than generating markdown manually.

### Interactive authoring command

You can also create a dedicated interactive authoring command that always opts in to prompts.

```toml
[cli.change-interactive]
help_text = "Create a change file interactively"

[[cli.change-interactive.steps]]
type = "CreateChangeFile"
inputs = { interactive = true }
```

## Good fit / bad fit

**Good fit:**

- contributor-facing commands
- wrappers that standardize bump policies
- interactive authoring helpers

**Bad fit:**

- release execution
- commands that need `release.*` context

## Common mistakes

- omitting `package` in non-interactive mode
- expecting `CreateChangeFile` to release anything immediately
- using raw manifest paths when configured package ids are the stable interface
