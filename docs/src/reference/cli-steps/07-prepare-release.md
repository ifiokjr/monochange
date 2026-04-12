# `PrepareRelease`

## What it does

`PrepareRelease` is the core release execution step.

It discovers packages, loads authored changesets, computes the release plan, updates manifests and changelogs, and prepares the structured release result that later steps can consume.

In other words: most release-oriented commands are really **`PrepareRelease` plus something else**.

## Why use it

Use `PrepareRelease` whenever the command needs real release state.

It is the step that unlocks:

- release file updates
- changelog rendering
- release target calculation
- structured `release.*` template context
- later steps such as `CommitRelease`, `RenderReleaseManifest`, `PublishRelease`, `OpenReleaseRequest`, and `CommentReleasedIssues`

If your command eventually needs release metadata, start with `PrepareRelease` rather than trying to reconstruct that state in shell.

## Inputs

- `format` — `markdown`, `text`, or `json`

## Step-level `when` condition

All CLI steps support an optional `when = "..."` condition.

If the expression resolves to false at runtime, monochange skips the step and continues with the next step.

```toml
when = "{{ inputs.enabled }}"
```

## Prerequisites

None. `PrepareRelease` is the producer step for the rest of the release workflow.

## Side effects and outputs

`PrepareRelease` is stateful.

It can produce:

- updated manifests
- updated changelogs
- deleted or consumed changeset files
- release target information
- final command output in markdown, text, or JSON form
- structured `release.*` template values for later `Command` steps

Built-in release-oriented commands now default their human-readable `format` input to `markdown`. Use `text` when you explicitly want the older plain-text style, or `json` for automation.

It also fills the shorthand template values commonly used by `Command` steps:

- `{{ version }}`
- `{{ group_version }}`
- `{{ released_packages }}`
- `{{ changed_files }}`
- `{{ changesets }}`

## Example

<!-- {=cliStepPrepareReleaseExample} -->

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

<!-- {/cliStepPrepareReleaseExample} -->

## Composition ideas

### Prepare and run a custom follow-up command

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

### Prepare and then branch into provider automation

Typical production commands look like:

- `PrepareRelease` → `RenderReleaseManifest`
- `PrepareRelease` → `PublishRelease`
- `PrepareRelease` → `OpenReleaseRequest`
- `PrepareRelease` → `CommitRelease`
- `PrepareRelease` → `Command`

### Prepare once, consume several outputs

Because the later steps all depend on the same prepared state, you should generally do one `PrepareRelease` and then fan out from it with several typed steps rather than trying to run several independent release commands.

### Reuse prepared state across separate commands

When you do need to split the workflow across separate commands, monochange can now reuse a prepared release artifact instead of recomputing the release plan from scratch.

The default path is automatic:

```bash
mc release
mc release-pr --dry-run
```

`mc release` stores the prepared state in `.monochange/prepared-release-cache.json`, and later commands with a `PrepareRelease` step reuse it when the git `HEAD`, workspace status, tracked release inputs, and relevant configuration still match.

If you need to pass the artifact between explicit jobs or custom commands, use `--prepared-release`:

```bash
mc release --prepared-release /tmp/release-plan.json
mc release-pr --prepared-release /tmp/release-plan.json --format json
```

If the artifact is stale, monochange falls back to a fresh `PrepareRelease` run instead of trusting outdated release data.

## Good fit / bad fit

**Good fit:**

- any release workflow
- commands that need release metadata
- commands that need changelog or version updates

**Bad fit:**

- simple validation-only CI gates
- discovery-only inspection commands
- post-release repair flows (`RetargetRelease` is separate)

## Common mistakes

- putting `PublishRelease` or `OpenReleaseRequest` before `PrepareRelease`
- assuming `PrepareRelease` is just a read-only planner in non-dry-run mode
- forgetting that later `Command` steps can consume its structured output directly
- forgetting that `--quiet` suppresses stdout/stderr and forces dry-run behavior when the command supports dry-run semantics
