# `VerifyReleaseBranch`

## What it does

`VerifyReleaseBranch` checks that a git ref resolves to a commit reachable from one of the configured release branches.

The policy lives under `[source.releases]`:

```toml
[source.releases]
branches = ["main", "release/*"]
enforce_for_tags = true
enforce_for_publish = true
enforce_for_commit = false
```

`branches` accepts multiple branch names and glob patterns. The check uses commit reachability, so it also works in detached CI checkouts when the tag or `HEAD` commit is present in the repository history.

## Inputs

- `from` — git ref to verify. Defaults to `HEAD`.

## Example

```toml
[cli.verify-release-branch]
help_text = "Verify this checkout is on an allowed release branch"

[[cli.verify-release-branch.inputs]]
name = "from"
kind = "string"
default = "HEAD"

[[cli.verify-release-branch.steps]]
type = "VerifyReleaseBranch"
[cli.verify-release-branch.steps.inputs]
from = "{{ inputs.from }}"
```

## Built-in enforcement

You usually do not need to add this step manually for protected release operations:

- `mc tag-release` enforces `[source.releases]` when `enforce_for_tags = true`.
- `PublishRelease` and `PublishPackages` enforce `[source.releases]` during real publish runs when `enforce_for_publish = true`.
- `CommitRelease` enforces `[source.releases]` only when `enforce_for_commit = true`.

Use the explicit step when you want an early, standalone CI gate before other workflow work runs.
