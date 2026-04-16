# `DisplayVersions`

## What it does

`DisplayVersions` computes monochange's planned package and group versions and renders only that summary.

Use it when you want the release-version answer without the rest of the release preview.

## Why use it

Use `DisplayVersions` when you want a dedicated read-only command such as `mc versions`.

It is the best fit for:

- CI or local scripts that only need the planned version map
- release dashboards or follow-up tooling that want compact JSON
- human-readable summaries without release targets, changed files, or changelog previews

## Inputs

- `format` — `text`, `markdown`, or `json`

## Step-level `when` condition

All CLI steps support an optional `when = "..."` condition.

If the expression resolves to false at runtime, monochange skips the step and continues with the next step.

```toml
when = "{{ inputs.enabled }}"
```

## Prerequisites

None. `DisplayVersions` is standalone.

## Side effects and outputs

`DisplayVersions` is read-only.

It:

- computes the same planned package and group versions used by monochange release workflows
- renders a compact summary in `text`, `markdown`, or `json`
- does not update manifests, changelogs, or consumed changesets
- does not require a previous `PrepareRelease` step

## Example

```toml
[cli.versions]
help_text = "Display planned package and group versions"

[[cli.versions.inputs]]
name = "format"
type = "choice"
choices = ["text", "markdown", "json"]
default = "text"

[[cli.versions.steps]]
name = "display versions"
type = "DisplayVersions"
```

## Composition ideas

### Use it as the built-in summary command

```bash
mc versions
mc versions --format markdown
mc versions --format json
```

### Keep release preparation and version display separate

Use `DisplayVersions` when you only need the version summary. Use [`PrepareRelease`](07-prepare-release.md) when you also need release file updates, release targets, manifest artifacts, or later release-oriented steps.

## Common mistakes

- expecting it to update release files
- treating it as a replacement for `PrepareRelease` in publish or release-request workflows
- bundling it into long multi-step commands when a dedicated `versions` command is clearer
