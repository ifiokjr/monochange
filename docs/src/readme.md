# Introduction

`monochange` is a cross-ecosystem release planner for monorepos.

The current milestone focuses on:

<!-- {=projectMilestoneCapabilities} -->

- discover Cargo, npm/pnpm/Bun, Deno, Dart, and Flutter packages
- normalize dependency edges across ecosystems
- declare release-managed packages explicitly in `monochange.toml`
- coordinate shared release identity through named groups
- validate config and changesets with `mc check`
- compute release plans from explicit change input
- run config-defined release workflows from `.changeset/*.md`
- apply Rust semver evidence when provided
- publish end-user documentation through the mdBook in `docs/`

<!-- {/projectMilestoneCapabilities} -->

## Core workflow

<!-- {=projectCoreWorkflow} -->

Create a `monochange.toml` file:

```toml
[defaults]
parent_bump = "patch"
warn_on_group_mismatch = true

[package.monochange]
path = "crates/monochange"
type = "cargo"
changelog = "crates/monochange/CHANGELOG.md"

[package.monochange_core]
path = "crates/monochange_core"
type = "cargo"
changelog = "crates/monochange_core/CHANGELOG.md"

[group.workspace]
packages = ["monochange", "monochange_core"]
tag = true
release = true
version_format = "primary"

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
```

Validate the repository:

```bash
mc check --root .
```

Discover the workspace:

```bash
mc workspace discover --root . --format json
```

Create a change file:

```bash
mc changes add --root . --package monochange --bump minor --reason "add release planning"
```

Preview the release workflow:

```bash
mc release --dry-run
```

Inspect the raw planner when needed:

```bash
mc plan release --root . --changes .changeset/my-change.md --format json
```

Prepare the release:

```bash
mc release
```

<!-- {/projectCoreWorkflow} -->

Run the full validation suite:

<!-- {=projectValidationCommands} -->

```bash
docs:verify
lint:all
test:all
build:all
build:book
```

<!-- {/projectValidationCommands} -->

## What the JSON output includes

Discovery output includes:

<!-- {=projectDiscoveryOutputIncludes} -->

- normalized package records
- dependency edges
- version groups derived from configured groups
- warnings

<!-- {/projectDiscoveryOutputIncludes} -->

Release-plan output includes:

<!-- {=projectReleaseOutputIncludes} -->

- per-package bump decisions
- synchronized group outcomes
- compatibility evidence
- warnings and unresolved items

<!-- {/projectReleaseOutputIncludes} -->
