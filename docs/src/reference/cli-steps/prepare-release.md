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

- `format` — `text` or `json`

## Prerequisites

None. `PrepareRelease` is the producer step for the rest of the release workflow.

## Side effects and outputs

`PrepareRelease` is stateful.

It can produce:

- updated manifests
- updated changelogs
- deleted or consumed changeset files
- release target information
- final command output in text or JSON form
- structured `release.*` template values for later `Command` steps

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
