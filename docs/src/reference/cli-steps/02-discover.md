# `Discover`

## What it does

`Discover` runs monochange package discovery and renders the result in `text` or `json` form.

It is the step to use when you want to inspect how monochange sees the repository before you involve changesets or release logic.

## Why use it

Use `Discover` when you need visibility into:

- which packages monochange found
- which ids were assigned
- which manifest paths were normalized
- whether a repository layout is discoverable the way you expect

This is particularly valuable in mixed-ecosystem monorepos where discovery rules are part of the product contract.

## Inputs

- `format` — `text` or `json`

## Step-level `when` condition

All CLI steps support an optional `when = "..."` condition.

If the expression resolves to false at runtime, monochange skips the step and continues with the next step.

```toml
when = "{{ inputs.enabled }}"
```

## Prerequisites

None. `Discover` is standalone.

## Side effects and outputs

- discovers packages across supported ecosystems
- emits a report for the overall CLI command output
- does not prepare release state for later steps

## Example

<!-- {=cliStepDiscoverExample} -->

```toml
[cli.discover]
help_text = "Discover packages across supported ecosystems"

[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.discover.steps]]
type = "Discover"
```

<!-- {/cliStepDiscoverExample} -->

## Composition ideas

### Discovery-focused debug command

```toml
[cli.discover-debug]
help_text = "Show package discovery and then print a custom notice"

[[cli.discover-debug.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "json"

[[cli.discover-debug.steps]]
type = "Discover"
```

`Discover` is usually best as the only step in a command, because its value is the rendered report itself.

### Use it during repository setup

During initial adoption, teams often expose a `discover` command next to `validate` so contributors can see the exact package ids they should use in `.changeset/*.md` files and command inputs.

## Why not just shell out?

A `Command` step that runs `mc discover` works, but the built-in step is easier to validate and easier to understand when reading `monochange.toml`. It makes the intent obvious: the command exists to inspect discovery, not to run an arbitrary shell pipeline.

## Common mistake

Do not treat `Discover` as release planning. It does not read changesets into a release decision. For that, use `PrepareRelease`.
