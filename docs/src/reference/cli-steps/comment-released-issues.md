# `CommentReleasedIssues`

## What it does

`CommentReleasedIssues` uses prepared release context to comment on issues linked from the release's changeset and review metadata.

It is a post-publication communication step, not a planning step.

## Why use it

Use `CommentReleasedIssues` when you want MonoChange to close the loop after publication by posting structured release follow-up comments.

This is especially valuable when:

- issues are part of the public release workflow
- you want issue comments to stay tied to the exact prepared release data
- you want a dry-run preview before touching hosted issue state

## Inputs

- `format` — `text` or `json`

## Prerequisites

- a previous `PrepareRelease` step in the same command
- `[source].provider = "github"`

## Side effects and outputs

- builds issue comment plans from prepared release context
- in dry-run mode, previews which issues would be touched
- in normal mode, creates or skips comments based on provider state

## Example

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

## Composition ideas

### Publish first, then comment

The most common and most sensible sequence is:

1. `PrepareRelease`
2. `PublishRelease`
3. `CommentReleasedIssues`

That ordering reflects the real-world intent: only comment after the release event exists.

### Comment and then run a reporting step

```toml
[cli.publish-comment-report]
help_text = "Publish a release, comment on issues, and print a short report"

[[cli.publish-comment-report.steps]]
type = "PrepareRelease"

[[cli.publish-comment-report.steps]]
type = "PublishRelease"

[[cli.publish-comment-report.steps]]
type = "CommentReleasedIssues"

[[cli.publish-comment-report.steps]]
type = "Command"
command = "echo issue comments processed for {{ release.version }}"
shell = true
```

## Why choose it over a custom GitHub API script?

Because the built-in step already consumes MonoChange's linked issue and review metadata model. A shell script would need to rediscover which issues matter for the release.

## Common mistake

Using `CommentReleasedIssues` without a GitHub source configuration. This step is intentionally provider-specific today.
