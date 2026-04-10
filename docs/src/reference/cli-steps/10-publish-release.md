# `PublishRelease`

## What it does

`PublishRelease` converts a prepared release into hosted provider release operations.

For example, with a configured source provider it can create or update the outward release objects that correspond to monochange's prepared release targets.

## Why use it

Use `PublishRelease` when you want monochange to handle provider-aware publication rather than stitching together release API calls manually.

That gives you:

- one publication step for grouped and package-owned releases
- dry-run previews that stay aligned with the prepared release state
- a typed boundary between planning and provider mutation
- source-provider integration driven by the same manifest and release target model as the rest of monochange

## Inputs

- `format` — `text` or `json`

## Prerequisites

- a previous `PrepareRelease` step in the same command
- `[source]` configuration

## Side effects and outputs

- in dry-run mode, builds preview release requests
- in normal mode, creates or updates provider releases
- contributes release request/result data to the command's final output

## Example

<!-- {=cliStepPublishReleaseExample} -->

```toml
[cli.publish-release]
help_text = "Prepare a release and publish hosted releases"

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
type = "PrepareRelease"

[[cli.publish-release.steps]]
type = "PublishRelease"
```

<!-- {/cliStepPublishReleaseExample} -->

## Composition ideas

### Publish and then comment on linked issues

<!-- {=cliStepCommentReleasedIssuesExample} -->

```toml
[cli.publish-and-comment]
help_text = "Publish a release and comment on linked issues"

[[cli.publish-and-comment.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-and-comment.steps]]
type = "PrepareRelease"

[[cli.publish-and-comment.steps]]
type = "PublishRelease"

[[cli.publish-and-comment.steps]]
type = "CommentReleasedIssues"
```

<!-- {/cliStepCommentReleasedIssuesExample} -->

This is one of the clearest examples of composition: `PublishRelease` performs outward release publication, and `CommentReleasedIssues` performs the follow-up communication step.

### Prepare, publish, then notify external systems

```toml
[cli.publish-and-notify]
help_text = "Prepare, publish, and notify another system"

[[cli.publish-and-notify.steps]]
type = "PrepareRelease"

[[cli.publish-and-notify.steps]]
type = "PublishRelease"

[[cli.publish-and-notify.steps]]
type = "Command"
command = "echo published {{ release.version }}"
shell = true
```

## Why choose it over a raw `Command` step?

Because `PublishRelease` understands monochange release targets, provider settings, and dry-run behavior. A hand-written shell command would need to rebuild all of that context.

## Common mistake

Do not treat `PublishRelease` as a planning step. It is the mutation step after planning is already complete.
