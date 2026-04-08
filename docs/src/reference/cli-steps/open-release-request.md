# `OpenReleaseRequest`

## What it does

`OpenReleaseRequest` turns a prepared release into a hosted release request, such as a release pull request.

It uses the prepared release state to build branch names, commit descriptions, and request bodies that correspond to the exact release content monochange prepared.

## Why use it

Use `OpenReleaseRequest` when you want a reviewable, provider-hosted release flow before publication.

This is a strong fit when your release process includes:

- opening or updating a release PR for human review
- staging release artifacts on a branch before merge
- reusing monochange's structured release data in the request body

## Inputs

- `format` — `text` or `json`

## Prerequisites

- a previous `PrepareRelease` step in the same command
- `[source]` configuration

## Side effects and outputs

- in dry-run mode, previews the request payload
- in normal mode, performs git and provider operations needed to open or update the request
- contributes release-request data to the command result

## Example

<!-- {=cliStepOpenReleaseRequestExample} -->

```toml
[cli.release-pr]
help_text = "Prepare a release and open or update a release request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
```

<!-- {/cliStepOpenReleaseRequestExample} -->

## Composition ideas

### Prepare, commit, then open a release request

```toml
[cli.release-pr-from-commit]
help_text = "Prepare a release, create the release commit, and open a release PR"

[[cli.release-pr-from-commit.steps]]
type = "PrepareRelease"

[[cli.release-pr-from-commit.steps]]
type = "CommitRelease"

[[cli.release-pr-from-commit.steps]]
type = "OpenReleaseRequest"
```

### Open a request and run an extra notification step

```toml
[cli.release-pr-notify]
help_text = "Open a release request and notify another system"

[[cli.release-pr-notify.steps]]
type = "PrepareRelease"

[[cli.release-pr-notify.steps]]
type = "OpenReleaseRequest"

[[cli.release-pr-notify.steps]]
type = "Command"
command = "echo opened release request for {{ release.version }}"
shell = true
```

## Why choose it over a custom git + provider script?

Because `OpenReleaseRequest` already knows:

- which release targets were prepared
- which files changed
- how monochange wants release requests described
- how dry-run should behave

## Common mistake

Do not assume `OpenReleaseRequest` can infer a release on its own. It is not a replacement for `PrepareRelease`.
