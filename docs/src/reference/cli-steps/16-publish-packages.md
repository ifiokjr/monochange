# `PublishPackages`

## What it does

`PublishPackages` publishes package versions to their target registries using monochange's built-in ecosystem workflows.

The step derives publish work from durable monochange release state: a prepared release artifact when the command has one, or the release record discoverable from `HEAD` otherwise. It does not require a `readiness` artifact. Before publishing, it orders selected package publications by internal publish-relevant dependencies so dependencies are attempted before dependents. Runtime, build, peer, workspace, and unknown dependency kinds participate in ordering and cycle validation; development-only dependency cycles are ignored.

For each package with a planned release version, the step:

- resolves the registry from the package's publish configuration
- validates publish-relevant dependency cycles before registry mutation
- publishes dependencies before dependents within the selected publish set
- checks whether the version already exists (skipping if it does)
- plans against registry rate limits before attempting any mutation
- runs the ecosystem-specific publish command (`cargo publish`, `npm publish`, `dart pub publish`, `flutter pub publish`, `deno publish`, and so on)
- produces a structured report of what was published, skipped, or planned

You can filter the publish set with the `package` input, or use an empty set to publish everything from the selected release state.

## Publication order

Package publication order is dependency-aware. monochange publishes packages with no selected dependencies first, then publishes packages that depend on those packages, walking up the dependency tree until packages that depend on the most selected packages are published last.

The order is computed like this:

1. Build the selected publish requests from the prepared release or `HEAD` release state.
2. Materialize the workspace dependency graph.
3. Consider only dependencies where **both packages are part of the selected publish set**.
4. Ignore development dependency edges.
5. Topologically sort the publish requests so dependencies are emitted before dependents.

For example, with this internal package graph:

```text
core        # no dependencies
utils       # depends on core
api         # depends on utils
app         # depends on core, utils, api
```

monochange publishes in this order:

```text
core
utils
api
app
```

If multiple packages are independent at the same depth, their order is deterministic by package id, registry, and version.

A package with no selected dependencies is eligible first. A package is not published until all of its selected publish-relevant dependencies have been ordered before it. Dependencies outside the selected publish set do not block ordering. Development-only cycles are ignored. Runtime, build, peer, workspace, and unknown dependency cycles fail before publishing anything, with a cycle diagnostic.

## Why use it

Use `PublishPackages` when you want monochange to handle the full package-registry publication workflow rather than scripting individual publish commands.

That gives you:

- one publish step for all supported ecosystems
- automatic dependency ordering across internal package publications
- publish-relevant cycle detection before registry mutation
- automatic rate-limit planning and enforcement
- version-existence checks that prevent duplicate publish attempts
- dry-run previews that show the full publish plan without touching registries
- structured `publish.*` template context for later `Command` steps

Use `PlaceholderPublish` instead when you need to bootstrap a package that does not yet exist in its registry with a minimal `0.0.0` placeholder.

## Inputs

- `format` — `text`, `markdown`, or `json`
- `package` — optional repeated package ids used to filter the publish set
- `resume` — optional path to a JSON result artifact from an earlier real `mc publish` run; completed package versions are skipped and failed or pending work is retried
- `output` — optional path where monochange writes the package publish result JSON artifact for retry/resume workflows

## Step-level `when` condition

All CLI steps support an optional `when = "..."` condition.

If the expression resolves to false at runtime, monochange skips the step and continues with the next step.

```toml
when = "{{ inputs.enabled }}"
```

## Prerequisites

- a prepared release artifact or a release record discoverable from `HEAD` that contains the package publication targets
- no cycles among selected publish-relevant internal dependencies; development-only cycles are allowed
- for built-in Cargo publishes to crates.io, a publishable current `Cargo.toml`: no `publish = false`, any `publish = [...]` list includes `crates-io`, `description` is set, and either `license` or `license-file` is set; workspace-inherited values are accepted

## Side effects and outputs

- in dry-run mode, plans and previews publish operations without touching registries
- in normal mode, validates release-branch policy and publish-relevant dependency cycles, then publishes package versions to their configured registries
- when `output` is set, writes the package publish result artifact even if a registry publish command fails, then exits non-zero for failed package outcomes
- contributes `publish.*` and `publish_rate_limits.*` template context to the command result

## Example

<!-- {=cliStepPublishPackagesExample} -->

```toml
[cli.publish]
help_text = "Publish package versions from monochange release state using built-in workflows"

[[cli.publish.inputs]]
name = "format"
type = "choice"
choices = ["text", "markdown", "json"]
default = "text"

[[cli.publish.inputs]]
name = "package"
type = "string_list"

[[cli.publish.inputs]]
name = "resume"
type = "path"
help_text = "JSON result artifact from an earlier mc publish run; completed package versions are skipped"

[[cli.publish.inputs]]
name = "output"
type = "path"
help_text = "Write the package publish result JSON artifact for retry/resume"

[[cli.publish.steps]]
name = "publish packages"
type = "PublishPackages"
```

<!-- {/cliStepPublishPackagesExample} -->

## Composition ideas

### Preview readiness before publishing

Use `mc publish-readiness` when you want a reviewable preflight report, then publish directly from the same release state:

```bash
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc publish --output .monochange/publish-result.json
```

The readiness artifact is informational for `PublishPackages`; it is not required by `mc publish`. If a real publish fails after writing `.monochange/publish-result.json`, fix the registry/auth issue and rerun with `mc publish --resume .monochange/publish-result.json --output .monochange/publish-result.json`.

### Publish only a specific package

```toml
[cli.publish-core]
help_text = "Publish a specific package"

[[cli.publish-core.inputs]]
name = "package"
type = "string_list"
required = true

[[cli.publish-core.steps]]
name = "publish packages"
type = "PublishPackages"
```

### Publish with rate-limit planning

```toml
[cli.publish-planned]
help_text = "Plan and publish with rate-limit awareness"

[[cli.publish-planned.steps]]
name = "plan publish rate limits"
type = "PlanPublishRateLimits"

[[cli.publish-planned.steps]]
name = "publish packages"
type = "PublishPackages"
```

## Why choose it over a raw `Command` step?

Because `PublishPackages` understands:

- which packages were planned for release
- which ecosystem and registry each package targets
- which selected internal packages must publish before others
- whether publish-relevant dependency cycles would make a safe order impossible
- whether a version already exists (and should be skipped)
- ecosystem-specific publish commands, flags, and auth patterns
- rate-limit planning across registries
- dry-run behavior for safe CI previews
- trusted publishing setup and configuration

## Common mistakes

- confusing `PublishPackages` with `PublishRelease`: the former publishes to package registries, the latter creates hosted provider releases (such as GitHub releases)
- assuming `mc publish` consumes the JSON file from `mc publish-readiness`; use readiness for preflight review or `mc publish-plan --readiness`, not as a `PublishPackages` input
- omitting `output` in CI, which makes partial registry failures harder to resume safely
- expecting development-only dependency cycles to block publishing; only publish-relevant dependency kinds participate in cycle validation
- running `PublishPackages` without rate-limit planning: use `PlanPublishRateLimits` first when you are unsure about registry windows
