# `CommitRelease`

## What it does

`CommitRelease` turns an already prepared release into a local git commit.

The step uses monochange's release-commit format and embeds a durable `ReleaseRecord` in the commit body. That record is what later powers release inspection and repair workflows such as `mc release-record` and `mc repair-release`.

Think of it as the step that makes a prepared release durable in git history.

## Why use it

Use `CommitRelease` when you want release planning and file updates to end in a reviewable, local commit before any provider-specific automation happens.

This is especially useful when you want to:

- create a durable release commit locally
- keep release history explicit in git rather than only in provider APIs
- open a release request from a known monochange-generated commit
- preserve the `ReleaseRecord` needed for later repair or inspection flows
- hand off a prepared release to later custom `Command` steps without reconstructing commit metadata yourself

## Inputs

`CommitRelease` does not accept built-in step inputs.

That is intentional: it consumes prepared release state instead of raw user input.

## Step-level `when` condition

All CLI steps support an optional `when = "..."` condition.

If the expression resolves to false at runtime, monochange skips the step and continues with the next step.

```toml
when = "{{ inputs.enabled }}"
```

## Prerequisites

`CommitRelease` needs prepared release state.

You can provide that state in either of two ways:

- run a previous `PrepareRelease` step in the same command
- reuse a saved prepared release artifact from `.monochange/prepared-release-cache.json` or `--prepared-release`

`CommitRelease` is a **consumer** step. It does not plan a release on its own.

## Side effects and outputs

In normal mode, `CommitRelease` creates a local commit.

In `--dry-run` mode, it previews the commit payload without creating the commit.

It exposes a structured `release_commit.*` namespace to later `Command` steps. Commonly useful fields include:

- `release_commit.subject`
- `release_commit.body`
- `release_commit.commit`
- `release_commit.tracked_paths`
- `release_commit.dry_run`
- `release_commit.status`

Use those values when you want later steps to:

- print the release commit sha
- generate custom notifications
- attach commit metadata to CI artifacts
- feed the created commit into external tooling

## Example

<!-- {=cliStepCommitReleaseExample} -->

```toml
[cli.commit-release]
help_text = "Prepare a release and create a local release commit"

[[cli.commit-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.commit-release.steps]]
type = "PrepareRelease"

[[cli.commit-release.steps]]
type = "CommitRelease"
```

<!-- {/cliStepCommitReleaseExample} -->

## Composition ideas

### Prepare, commit, then print commit metadata

```toml
[cli.commit-and-show]
help_text = "Prepare a release, create a commit, and print the commit sha"

[[cli.commit-and-show.steps]]
type = "PrepareRelease"

[[cli.commit-and-show.steps]]
type = "CommitRelease"

[[cli.commit-and-show.steps]]
type = "Command"
command = "echo release commit {{ release_commit.commit }}"
shell = true
```

### Prepare, commit, then open a release request

A strong provider-facing pattern is:

1. `PrepareRelease`
2. `CommitRelease`
3. `OpenReleaseRequest`

That sequence keeps the release branch and provider request aligned with the durable release commit that monochange created.

### Prepare once, then let custom tooling consume commit metadata

If your team has custom chat notifications, CI uploads, or deployment hooks, `CommitRelease` is a better producer than a hand-written `git commit` command because later steps can read structured `release_commit.*` values directly.

## Good fit / bad fit

**Good fit:**

- release workflows that should leave behind a durable git record
- teams that want provider automation to begin from a known release commit
- workflows that may later need `RetargetRelease`

**Bad fit:**

- validation or inspection-only commands
- workflows that do not prepare a release first
- commands where a plain custom shell commit is acceptable and no monochange release record is needed

## Why choose it over a plain `git commit` command?

Because `CommitRelease` understands prepared release state and writes monochange's `ReleaseRecord` contract for you.

A raw shell commit can create a commit, but it cannot automatically preserve the release metadata that later monochange repair and inspection features rely on unless you reimplement that format yourself.

## Common mistakes

- treating `CommitRelease` as a replacement for `PrepareRelease`
- assuming the cached `.monochange/release-manifest.json` artifact must be committed for `CommitRelease` to succeed
- assuming it publishes releases or opens a release request by itself
- forgetting that `--dry-run` previews the commit rather than creating it
- reaching for a custom `git commit` command and then losing durable release metadata
