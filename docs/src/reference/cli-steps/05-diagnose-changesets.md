# `DiagnoseChangesets`

## What it does

`DiagnoseChangesets` inspects discovered changesets and reports how monochange interpreted them.

That includes parsed targets, notes, bump or version intent, provenance, and linked review metadata.

It is the inspection step you reach for when a changeset exists but you want to understand **why monochange is treating it a certain way**.

## Why use it

Use `DiagnoseChangesets` when you need visibility into:

- which package or group targets a changeset resolved to
- what bump or explicit version monochange inferred
- which commit introduced or last updated the changeset
- which review request or linked issues were attached to it
- why a release note, policy decision, or provider comment included that changeset

This makes it especially useful for debugging rich release-note context and CI policy behavior.

## Inputs

- `format` — `text` or `json`
- `changeset` — one or more explicit changeset paths; omit to inspect all discovered changesets

## Prerequisites

None. `DiagnoseChangesets` is standalone.

It does not require `PrepareRelease`, and it does not modify workspace state.

## Side effects and outputs

`DiagnoseChangesets` is read-only.

It:

- produces a diagnostics report
- does not prepare release state
- does not edit files
- is useful both for human debugging and for machine-readable CI inspection

In practice:

- use `format = "text"` for local debugging
- use `format = "json"` when another tool should consume the results

## Example

<!-- {=cliStepDiagnoseChangesetsExample} -->

```toml
[cli.diagnostics]
help_text = "Inspect changeset context and provenance"

[[cli.diagnostics.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.diagnostics.inputs]]
name = "changeset"
type = "string_list"

[[cli.diagnostics.steps]]
type = "DiagnoseChangesets"
```

<!-- {/cliStepDiagnoseChangesetsExample} -->

## Composition ideas

### Diagnose a targeted changeset set in CI

```toml
[cli.diagnostics-json]
help_text = "Inspect selected changesets as JSON"

[[cli.diagnostics-json.inputs]]
name = "changeset"
type = "string_list"

[[cli.diagnostics-json.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "json"

[[cli.diagnostics-json.steps]]
type = "DiagnoseChangesets"
```

### Use it as a maintainer support command

Many teams expose `DiagnoseChangesets` for maintainers only, because it shortens the time needed to explain:

- why a changeset rendered a certain note
- why it resolved to a package or group id
- why it linked to a certain review request or issue

### Pair it with external tooling through `Command`

If you need custom summarization or uploads, run `DiagnoseChangesets` as JSON and keep that command separate from release planning. It is usually clearer to treat diagnosis as its own workflow instead of trying to hide it inside a release command.

## Good fit / bad fit

**Good fit:**

- maintainer debugging commands
- CI jobs that inspect authored changesets without publishing anything
- support workflows where you need interpreted changeset context, not just raw markdown

**Bad fit:**

- commands that should mutate manifests, changelogs, or releases
- workflows that need `release.*` state
- cases where simply reading the markdown file is enough

## Why choose it over opening the markdown file directly?

Because the raw file is only part of the picture.

`DiagnoseChangesets` shows the interpreted result after monochange resolves package ids, provenance, linked review metadata, and related issue context. That is usually the information you actually need when debugging release behavior.

## Common mistakes

- expecting `DiagnoseChangesets` to modify anything
- assuming it prepares release state for later publication steps
- using it when a simpler `mc validate` failure would already answer the question
- forgetting to switch to `json` output when another tool should consume the results
