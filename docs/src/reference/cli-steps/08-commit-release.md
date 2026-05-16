# `CommitRelease`

## What it does

`CommitRelease` turns an already prepared release into a local git commit.

The step uses monochange's release-commit format and embeds a durable `ReleaseRecord` in the commit body. That record is what later powers release inspection and repair workflows such as `mc step:release-record` and `mc step:retarget-release`.

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

`CommitRelease` accepts one optional step-level boolean input:

| Input                 | Type    | Default | Description                                                                                                                                                                                                                                                |
| --------------------- | ------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `update_release_json` | boolean | `false` | When `true`, allows `CommitRelease` to create or overwrite the `.monochange/releases/<id>/release.json` record if it is missing or does not match the expected content. When `false` (the default), a missing or mismatched record is treated as an error. |

This input is useful when a previous step (such as `PrepareRelease` or a `Command` step that runs `dprint fmt`) may have modified the release record file, and you want `CommitRelease` to accept the regenerated content rather than fail with a mismatch error.

`CommitRelease` compares release records semantically (parsed JSON values), so formatting-only differences such as indentation or key ordering are ignored and never trigger a mismatch.

## Step-level `when` condition

All CLI steps support an optional `when = "..."` condition.

If the expression resolves to false at runtime, monochange skips the step and continues with the next step.

```toml
when = "{{ inputs.enabled }}"
```

## Step-level `always_run` flag

All CLI steps support an optional `always_run = true` flag.

When set, the step executes even if a previous step in the same command has failed. This is useful for cleanup, notification, or dry-run preview steps that must run regardless of earlier outcomes.

```toml
always_run = true
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

Before committing, `CommitRelease` validates the `.monochange/releases/<id>/release.json` record on disk. If the file exists, the step compares it against the expected content **semantically** (parsed JSON values), so formatting-only differences such as indentation or key ordering do not trigger a mismatch. If the file is missing or semantically different, the step either errors (default) or overwrites the file, depending on the `update_release_json` input.

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
inputs = ["format"]

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

### Prepare, format, then commit with record overwrite

If a formatting tool such as `dprint fmt` runs between `PrepareRelease` and `CommitRelease`, it may change the whitespace or key ordering of the generated `release.json`. By default, `CommitRelease` would treat this as a mismatch and error. Set `update_release_json = true` to allow `CommitRelease` to overwrite the formatted file with the semantically-equivalent regenerated content:

```toml
[[cli.release-pr.steps]]
type = "PrepareRelease"
name = "prepare release"

[[cli.release-pr.steps]]
type = "Command"
name = "format changed files"
command = "dprint fmt --allow-no-files {{ changed_files }} .monochange/releases/"

[[cli.release-pr.steps]]
type = "CommitRelease"
name = "create release commit"
update_release_json = true
```

This pattern is the recommended way to combine automated formatting with release record durability.

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
- running a formatter (such as `dprint fmt`) between `PrepareRelease` and `CommitRelease` without setting `update_release_json = true` on the `CommitRelease` step
