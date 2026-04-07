# `RetargetRelease`

## What it does

`RetargetRelease` repairs an already-recorded release.

It finds the release's durable `ReleaseRecord`, plans a retarget operation, and then moves the release tag set to a later commit.

This is intentionally separate from `PrepareRelease`-driven steps. It works from git history and durable release metadata, not from newly prepared release state.

## Why use it

Use `RetargetRelease` when the release already happened but the tags or hosted release state need to move.

It is a repair step, not a planning step.

Typical use cases include:

- a release commit landed, but tags must move to a later fix commit
- the hosted release should stay aligned with the corrected tag position
- a recent release needs to be repaired without generating a brand-new release plan
- a previous `CommitRelease` left the durable release record you now want to reuse safely

## Inputs

- `from` — tag or commit-ish used to discover the release record
- `target` — commit-ish to move the release to; defaults to `HEAD`
- `force` — allow non-descendant retargets
- `sync_provider` — whether hosted provider state should be synchronized
- `format` — `text` or `json`

## Prerequisites

None.

Unlike publication-oriented steps, `RetargetRelease` does **not** require `PrepareRelease` first.

## Side effects and outputs

`RetargetRelease` is a stateful maintenance step.

It can:

- discover the release record from history
- plan and optionally execute tag movement
- optionally synchronize provider release state
- expose a rich `retarget.*` namespace to later `Command` steps

Commonly useful fields include:

- `retarget.from`
- `retarget.target`
- `retarget.record_commit`
- `retarget.resolved_from_commit`
- `retarget.distance`
- `retarget.tags`
- `retarget.provider_results`
- `retarget.status`

In `--dry-run` mode, it reports the planned repair without mutating tags or provider state.

## Safety model

`RetargetRelease` is designed to make repair explicit.

A few rules matter in practice:

- you identify the release to repair with `from`
- by default, the target is `HEAD`
- non-descendant repairs require `force = true`
- provider synchronization is optional and controlled with `sync_provider`

That means you can start with a safe preview, confirm the proposed movement, and only then run the real repair.

## Example

<!-- {=cliStepRetargetReleaseExample} -->

```toml
[cli.repair-release]
help_text = "Repair a recent release by retargeting its tags"

[[cli.repair-release.inputs]]
name = "from"
type = "string"
required = true

[[cli.repair-release.inputs]]
name = "target"
type = "string"
default = "HEAD"

[[cli.repair-release.inputs]]
name = "force"
type = "boolean"
default = "false"

[[cli.repair-release.inputs]]
name = "sync_provider"
type = "boolean"
default = "true"

[[cli.repair-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.repair-release.steps]]
type = "RetargetRelease"
```

<!-- {/cliStepRetargetReleaseExample} -->

## Composition ideas

### Repair and print a custom notification

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

### Use it in a dedicated maintenance command

`RetargetRelease` usually belongs in a maintenance-oriented command rather than a day-to-day release command.

It represents a different lifecycle phase: post-release repair.

### Preview first, then perform the repair

A good operational pattern is:

1. run the repair command with `--dry-run`
2. inspect `retarget.status`, `retarget.tags`, and the proposed target
3. rerun without `--dry-run` once the plan is correct

## Good fit / bad fit

**Good fit:**

- release repair workflows
- operational commands owned by maintainers or release engineers
- commands that need structured `retarget.*` output for notifications or audits

**Bad fit:**

- normal release publishing flows
- commands that should create a brand-new release plan
- situations where a simple patch release is the safer response

## Why choose it over manually moving tags?

Because the built-in step repairs the release as a coherent unit based on the stored `ReleaseRecord`.

That means it can:

- find the release record from history
- reason about the release as MonoChange recorded it
- coordinate provider synchronization at the same time
- expose structured repair results to later steps

A manual tag move can change refs, but it does not preserve that workflow-level structure.

## Common mistakes

Do not mix up `RetargetRelease` and `PrepareRelease`.

- `PrepareRelease` answers: "what should be released now?"
- `RetargetRelease` answers: "how should an already-recorded release be repaired?"

Also avoid:

- skipping `--dry-run` when the repair is high risk
- using `force` without first understanding why the target is not a descendant
- treating retargeting as a substitute for publishing a new patch release when a real follow-up release is more appropriate
