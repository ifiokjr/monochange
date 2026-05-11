# monochange commands

## Command classes

monochange exposes three kinds of commands:

1. **Built-in commands** are hardcoded in `crates/monochange/src/cli.rs`.
2. **Step commands** are generated from built-in `CliStepDefinition` variants and have names like `mc step:prepare-release`.
3. **User-defined workflow commands** are loaded from `[cli.<name>]` tables in `monochange.toml` and become top-level `mc <name>` commands in that repository.

Do not describe a workflow command as built in unless it appears in the built-in list below.

When deciding what to run, prefer the most repository-native command that is still safe for the task. Use configured workflows for normal maintainer flows, step commands for portable automation or debugging, and built-in commands for global operations such as validation, MCP, lint catalog, release-record inspection, and publish readiness.

## Built-in commands

| Command                | Purpose                                                                                    |
| ---------------------- | ------------------------------------------------------------------------------------------ |
| `mc init`              | Generate `monochange.toml` from detected packages, groups, and optional provider settings. |
| `mc populate`          | Add missing workflow command definitions to `monochange.toml` so they can be customized.   |
| `mc skill`             | Install the monochange skill bundle with the skills CLI.                                   |
| `mc subagents`         | Generate repo-local monochange agent/subagent guidance.                                    |
| `mc analyze`           | Analyze semantic changes for a package.                                                    |
| `mc tag-release`       | Create and push release tags from an embedded release record.                              |
| `mc release-record`    | Inspect the monochange release record embedded in a tag or commit.                         |
| `mc check`             | Validate configuration, changesets, and manifest lint rules.                               |
| `mc lint`              | Inspect and scaffold manifest lint rules and presets.                                      |
| `mc mcp`               | Start the stdio MCP server.                                                                |
| `mc validate`          | Validate `monochange.toml` and changesets.                                                 |
| `mc publish-readiness` | Check package registry publishing readiness without publishing packages.                   |
| `mc publish-bootstrap` | Publish first-time placeholder package versions for a release record.                      |

Global flags include `--quiet`, `--progress-format <auto|unicode|ascii|json>`, and `--jq <expression>` for JSON output filtering.

A few built-ins are deliberately narrow:

- `mc validate` is the cheapest correctness check for configuration and changeset targets.
- `mc check` adds manifest linting and is the better pre-merge/pre-release check.
- `mc publish-readiness` and `mc publish-bootstrap` operate from an existing release record, so run them after a release has been prepared and committed.
- `mc release-record` is useful when debugging publish jobs because it tells you which packages and versions a commit or tag is supposed to release.

## Built-in step commands

| Step command                       | Step type               | Purpose                                                                 |
| ---------------------------------- | ----------------------- | ----------------------------------------------------------------------- |
| `mc step:config`                   | `Config`                | Render resolved configuration and workspace metadata.                   |
| `mc step:validate`                 | `Validate`              | Validate configuration, package manifests, and changesets.              |
| `mc step:discover`                 | `Discover`              | Discover packages across supported ecosystems.                          |
| `mc step:display-versions`         | `DisplayVersions`       | Preview planned package and group versions.                             |
| `mc step:create-change-file`       | `CreateChangeFile`      | Create a structured `.changeset/*.md` file.                             |
| `mc step:prepare-release`          | `PrepareRelease`        | Plan version bumps, changelogs, versioned files, and release artifacts. |
| `mc step:commit-release`           | `CommitRelease`         | Create a release commit with an embedded release record.                |
| `mc step:verify-release-branch`    | `VerifyReleaseBranch`   | Verify that a release branch still targets a valid base.                |
| `mc step:publish-release`          | `PublishRelease`        | Create or update hosted source-provider releases.                       |
| `mc step:placeholder-publish`      | `PlaceholderPublish`    | Publish missing first-time placeholder package versions.                |
| `mc step:publish-packages`         | `PublishPackages`       | Publish package versions from a publish plan.                           |
| `mc step:plan-publish-rate-limits` | `PlanPublishRateLimits` | Plan package publish batches around registry rate limits.               |
| `mc step:open-release-request`     | `OpenReleaseRequest`    | Open or update a hosted release pull request.                           |
| `mc step:comment-released-issues`  | `CommentReleasedIssues` | Comment on issues referenced by released changesets.                    |
| `mc step:affected-packages`        | `AffectedPackages`      | Evaluate affected packages and changeset coverage.                      |
| `mc step:diagnose-changesets`      | `DiagnoseChangesets`    | Inspect changeset provenance and review metadata.                       |
| `mc step:retarget-release`         | `RetargetRelease`       | Repair release tags by retargeting a release.                           |

`Command` is a valid workflow step type but does not have a standalone `mc step:command` command.

Step commands are useful when a repository has not configured a friendly wrapper yet, or when you want to isolate one phase of a larger workflow. Release-state steps such as `CommitRelease`, `PublishRelease`, `OpenReleaseRequest`, `CommentReleasedIssues`, and package publishing steps usually expect artifacts or state produced by `PrepareRelease` or a release record.

## All `[cli.*]` step types

These are the current `CliStepDefinition` variants accepted in `monochange.toml`:

- `Config`
- `Validate`
- `Discover`
- `DisplayVersions`
- `CreateChangeFile`
- `PrepareRelease`
- `CommitRelease`
- `VerifyReleaseBranch`
- `PublishRelease`
- `PlaceholderPublish`
- `PublishPackages`
- `PlanPublishRateLimits`
- `OpenReleaseRequest`
- `CommentReleasedIssues`
- `AffectedPackages`
- `DiagnoseChangesets`
- `RetargetRelease`
- `Command`

Common step fields include `type`, optional `name`, optional `when`, optional `always_run`, and optional `inputs` overrides. `Command` steps additionally use fields such as `command`, `dry_run_command`, `shell`, and `variables`.

## `[cli.*]` inputs

Workflow command input definitions support these input types:

- `string`
- `string_list`
- `path`
- `choice`
- `boolean`

Inputs may also use `help_text`, `required`, `default`, `choices`, and `short`.

Use `choice` for values that should be self-documenting in `mc help`, such as output formats or bump severities. Use `string_list` for repeatable selectors such as packages, groups, labels, or `caused_by` references. Use `path` when the workflow reads or writes artifacts so shell completion and help text communicate that a filesystem path is expected.

## User-defined workflow example

```toml
[cli.discover]
help_text = "Discover packages as JSON for automation"
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json"], default = "json" },
]
steps = [
	{ name = "discover packages", type = "Discover" },
]

[cli.release-preview]
help_text = "Preview versioned files without writing them"
dry_run = true
inputs = [
	{ name = "format", type = "choice", choices = ["markdown", "json"], default = "markdown" },
]
steps = [
	{ name = "plan release", type = "PrepareRelease" },
]
```

Run `mc help` in the repository to see which `[cli.*]` commands are active.
