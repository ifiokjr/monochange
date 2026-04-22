# `PublishPackages`

## What it does

`PublishPackages` publishes package versions to their target registries using monochange's built-in ecosystem workflows.

For each package with a planned release version, the step:

- resolves the registry from the package's publish configuration
- checks whether the version already exists (skipping if it does)
- plans against registry rate limits before attempting any mutation
- runs the ecosystem-specific publish command (`cargo publish`, `npm publish`, `dart pub publish`, `flutter pub publish`, `deno publish`, and so on)
- produces a structured report of what was published, skipped, or planned

You can filter the publish set with the `package` input, or use an empty set to publish everything that is ready.

## Why use it

Use `PublishPackages` when you want monochange to handle the full package-registry publication workflow rather than scripting individual publish commands.

That gives you:

- one publish step for all supported ecosystems
- automatic rate-limit planning and enforcement
- version-existence checks that prevent duplicate publish attempts
- dry-run previews that show the full publish plan without touching registries
- structured `publish.*` template context for later `Command` steps

Use `PlaceholderPublish` instead when you need to bootstrap a package that does not yet exist in its registry with a minimal `0.0.0` placeholder.

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

- a previous `PrepareRelease` step in the same command, or a release record discoverable from `HEAD` that contains the package publication targets

## Side effects and outputs

- in dry-run mode, plans and previews publish operations without touching registries
- in normal mode, publishes package versions to their configured registries
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

[[cli.publish.steps]]
name = "publish packages"
type = "PublishPackages"
```

<!-- {/cliStepPublishPackagesExample} -->

## Composition ideas

### Publish after preparing a release

```toml
[cli.release-and-publish]
help_text = "Prepare a release and publish packages"

[[cli.release-and-publish.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-and-publish.steps]]
type = "PrepareRelease"

[[cli.release-and-publish.steps]]
name = "publish packages"
type = "PublishPackages"
```

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
- whether a version already exists (and should be skipped)
- ecosystem-specific publish commands, flags, and auth patterns
- rate-limit planning across registries
- dry-run behavior for safe CI previews
- trusted publishing setup and configuration

## Common mistakes

- confusing `PublishPackages` with `PublishRelease`: the former publishes to package registries, the latter creates hosted provider releases (such as GitHub releases)
- forgetting that `PublishPackages` needs a previous `PrepareRelease` step or a release record in `HEAD`
- running `PublishPackages` without rate-limit planning: use `PlanPublishRateLimits` first when you are unsure about registry windows
