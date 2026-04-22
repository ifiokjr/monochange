# `PlaceholderPublish`

## What it does

`PlaceholderPublish` publishes minimal `0.0.0` placeholder versions for packages that do not yet exist in their target registries.

This is useful when you need to:

- reserve a package name before the first real release
- enable registry automation (such as trusted publishing or downstream dependency resolution) that requires the package to already be present
- bootstrap a new package into a registry so that later `PublishPackages` can update it with a real version

The step inspects each package's publish configuration, checks the registry to see if the package already exists, and only attempts to publish when the package is missing.

## Why use it

Use `PlaceholderPublish` when you want monochange to handle the initial registry bootstrap rather than running manual publish commands.

That gives you:

- automatic registry detection (the step skips packages that already exist)
- ecosystem-aware publish commands (`cargo publish`, `npm publish`, `dart pub publish`, `deno publish`, and so on)
- rate-limit planning before any mutation happens
- dry-run previews that show what would be published without touching registries
- structured `publish.*` template context for later `Command` steps

Use `PublishPackages` instead when you want to publish the real planned versions from a prepared release.

## Inputs

- `format` — `text`, `markdown`, or `json`
- `package` — optional repeated package ids used to filter the publish set

## Step-level `when` condition

All CLI steps support an optional `when = "..."` condition.

If the expression resolves to false at runtime, monochange skips the step and continues with the next step.

```toml
when = "{{ inputs.enabled }}"
```

## Prerequisites

None. `PlaceholderPublish` does not require a previous `PrepareRelease` step.

## Side effects and outputs

- in dry-run mode, plans and previews placeholder publish operations without touching registries
- in normal mode, publishes `0.0.0` placeholder versions for missing packages
- contributes `publish.*` and `publish_rate_limits.*` template context to the command result

## Example

<!-- {=cliStepPlaceholderPublishExample} -->

```toml
[cli.placeholder-publish]
help_text = "Publish placeholder package versions for missing registry packages"

[[cli.placeholder-publish.inputs]]
name = "format"
type = "choice"
choices = ["text", "markdown", "json"]
default = "text"

[[cli.placeholder-publish.inputs]]
name = "package"
type = "string_list"

[[cli.placeholder-publish.steps]]
name = "publish placeholder packages"
type = "PlaceholderPublish"
```

<!-- {/cliStepPlaceholderPublishExample} -->

## Composition ideas

### Plan placeholder publishing before running it

```toml
[cli.placeholder-plan]
help_text = "Plan and preview placeholder publishing"

[[cli.placeholder-plan.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.placeholder-plan.steps]]
name = "plan publish rate limits"
type = "PlanPublishRateLimits"
inputs = { mode = "placeholder" }

[[cli.placeholder-plan.steps]]
name = "publish placeholder packages"
type = "PlaceholderPublish"
```

### Placeholder publish as part of a CI bootstrapping command

```toml
[ci.bootstrap]
help_text = "Reserve package names for new packages"

[[ci.bootstrap.steps]]
type = "PlaceholderPublish"

[[ci.bootstrap.steps]]
type = "Command"
command = "echo placeholder publish completed for {{ publish.packages }}"
shell = true
```

## Why choose it over a raw `Command` step?

Because `PlaceholderPublish` understands:

- which packages are configured for publish
- which registries each ecosystem targets
- whether a package already exists (and should be skipped)
- ecosystem-specific publish commands and flags
- rate-limit planning across registries
- dry-run behavior for safe CI previews

## Common mistakes

- confusing `PlaceholderPublish` with `PublishPackages`: the former publishes `0.0.0` placeholders, the latter publishes the real planned versions
- forgetting that `PlaceholderPublish` does not require `PrepareRelease`, but `PublishPackages` does
- expecting placeholder versions to be updated automatically: placeholder publish is a one-time bootstrap step
