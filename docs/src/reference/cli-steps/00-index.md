# CLI step reference

<!-- {=cliStepReferenceOverview} -->

monochange CLI commands are built from ordered `[[cli.<command>.steps]]` entries.

A step is the smallest execution unit in a monochange workflow. Some steps are **standalone** (`Validate`, `Discover`, `AffectedPackages`, `DiagnoseChangesets`, `RetargetRelease`). Others are **stateful** and build on the result of an earlier `PrepareRelease` step (`RenderReleaseManifest`, `CommitRelease`, `PublishRelease`, `OpenReleaseRequest`, and `CommentReleasedIssues`).

When you design a command, think in terms of:

1. **what state the command needs**
2. **which step produces that state**
3. **which later step consumes it**
4. **what side effects are acceptable** in normal mode vs `--dry-run`

The reference pages in this section document each built-in step with:

- what the step does
- why you would choose it over a shell-only `Command`
- which inputs it accepts
- what prerequisite state it needs
- what it contributes to later steps
- examples of how it composes into full workflows

<!-- {/cliStepReferenceOverview} -->

## Choosing the right step

<!-- {=cliStepReferenceChoosingGuide} -->

| Step                    | Use it when you want to…                                                 | Requires previous step?          | Typical follow-up                                                                                                    |
| ----------------------- | ------------------------------------------------------------------------ | -------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `Validate`              | fail fast on invalid config, groups, or changesets                       | no                               | CI gate or local preflight                                                                                           |
| `Discover`              | inspect normalized package discovery across ecosystems                   | no                               | local inspection, debug commands                                                                                     |
| `CreateChangeFile`      | author a `.changeset/*.md` file from CLI inputs                          | no                               | run independently, or before planning                                                                                |
| `PrepareRelease`        | build the release result and update files                                | no                               | `CommitRelease`, `RenderReleaseManifest`, `PublishRelease`, `OpenReleaseRequest`, `CommentReleasedIssues`, `Command` |
| `CommitRelease`         | create a local release commit with an embedded `ReleaseRecord`           | `PrepareRelease`                 | `OpenReleaseRequest`, manual review, custom `Command`                                                                |
| `RenderReleaseManifest` | write a stable JSON artifact for downstream automation                   | `PrepareRelease`                 | CI upload, later `Command` steps                                                                                     |
| `PublishRelease`        | create or update hosted provider releases                                | `PrepareRelease` + `[source]`    | `CommentReleasedIssues`, custom notification commands                                                                |
| `OpenReleaseRequest`    | create or update a hosted release PR/MR                                  | `PrepareRelease` + `[source]`    | provider review, follow-up `Command` steps                                                                           |
| `CommentReleasedIssues` | post release follow-up comments to closed issues                         | `PrepareRelease` + GitHub source | normally after `PublishRelease`                                                                                      |
| `AffectedPackages`      | evaluate changeset coverage for changed files                            | no                               | CI enforcement, custom failure messaging                                                                             |
| `DiagnoseChangesets`    | inspect changeset context, commit provenance, and linked review metadata | no                               | local debugging, CI inspection                                                                                       |
| `RetargetRelease`       | repair a recent release by moving its tag set                            | no                               | custom `Command` steps using `retarget.*`                                                                            |
| `Command`               | run arbitrary shell/program commands with monochange context             | depends on your workflow         | any external tool                                                                                                    |

<!-- {/cliStepReferenceChoosingGuide} -->

## A note on composition

monochange executes steps in order.

That means composition is explicit:

- a step can only consume state created by an earlier step in the same command
- a later step never runs "in parallel" with an earlier one
- `--dry-run` flows through the whole command and changes the behavior of steps that support previews
- `--quiet` suppresses stdout/stderr and reuses dry-run behavior for commands that support it
- a plain `Command` step can bridge monochange and external tools, but built-in steps are preferable when you want stable semantics, structured JSON, or provider-aware behavior

In practice, most workflows fit one of four patterns:

1. **validation / inspection**
   - `Validate`
   - `Discover`
   - `AffectedPackages`
   - `DiagnoseChangesets`
2. **change authoring**
   - `CreateChangeFile`
3. **release preparation and publication**
   - `PrepareRelease`
   - then one or more of `CommitRelease`, `RenderReleaseManifest`, `PublishRelease`, `OpenReleaseRequest`, `CommentReleasedIssues`, `Command`
4. **post-release repair**
   - `RetargetRelease`
   - optionally followed by `Command`

## Shared concepts

### Step-local `name`

Every step can declare a `name = "..."` label.

Use that when you want human-friendly progress output such as `plan release`, `publish tags`, or `announce release` instead of the raw step kind.

### Step-local `when`

Every step can declare a `when = "..."` expression.

It uses minijinja-style expression evaluation with template context and supports logical combinations like `and`, `or`, and `not` (for example: `"{{ inputs.publish && !inputs.dry_run }}"`).

If the expression resolves to false, monochange skips that step and continues with the next step. Falsy values include `false`, `0`, and the empty string.

### Step-local `inputs`

Every step can define an `inputs = { ... }` override inside the step table.

Use that when:

- a command-level input should be rebound to a built-in step input
- you want to hardcode a value for one step but not the entire command
- you want to pass list or boolean values through direct template references such as `"{{ inputs.changed_paths }}"`

### Structured template namespaces

When you compose `Command` steps after built-in steps, monochange exposes structured context values such as:

- `release.*` after `PrepareRelease`
- `manifest.path` after `RenderReleaseManifest` with a `path`
- `affected.*` after `AffectedPackages`
- `retarget.*` after `RetargetRelease`
- `release_commit.*` after `CommitRelease`
- `steps.<id>.stdout` and `steps.<id>.stderr` after a `Command` step with `id = "..."`

Those namespaces are the main reason to prefer built-in steps over reimplementing the same workflow in shell.

## Pages in this section

- [Validate](01-validate.md)
- [Discover](02-discover.md)
- [CreateChangeFile](03-create-change-file.md)
- [AffectedPackages](04-affected-packages.md)
- [DiagnoseChangesets](05-diagnose-changesets.md)
- [RetargetRelease](06-retarget-release.md)
- [PrepareRelease](07-prepare-release.md)
- [CommitRelease](08-commit-release.md)
- [RenderReleaseManifest](09-render-release-manifest.md)
- [PublishRelease](10-publish-release.md)
- [OpenReleaseRequest](11-open-release-request.md)
- [CommentReleasedIssues](12-comment-released-issues.md)
- [Command](13-command.md)
