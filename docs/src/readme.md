# Introduction

`monochange` is a cross-ecosystem release planner for monorepos.

The current milestone focuses on:

<!-- {=projectMilestoneCapabilities} -->

- discover Cargo, npm/pnpm/Bun, Deno, Dart, and Flutter packages
- normalize dependency edges across ecosystems
- coordinate shared package groups from `monochange.toml`
- compute release plans from explicit change input
- expose top-level CLI commands from workflow definitions
- run config-defined release workflows from `.changeset/*.md`
- apply Rust semver evidence when provided
- publish end-user documentation through the mdBook in `docs/`

<!-- {/projectMilestoneCapabilities} -->

## Core workflow

<!-- {=projectCoreWorkflow} -->

Initialize the repository with detected packages, groups, and default workflows:

```bash
mc init
```

The generated `monochange.toml` becomes the source of truth for top-level commands like `mc validate`, `mc discover`, `mc change`, and `mc release`.

Validate the repository:

```bash
mc validate
```

Discover the workspace:

```bash
mc discover --format json
```

Create a change file:

```bash
mc change --package monochange --bump minor --reason "add release planning"
```

Preview the release workflow:

```bash
mc release --dry-run --format json
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
- release groups derived from configured groups
- warnings

<!-- {/projectDiscoveryOutputIncludes} -->

Release-plan output includes:

<!-- {=projectReleaseOutputIncludes} -->

- per-package bump decisions
- synchronized group outcomes
- compatibility evidence
- warnings and unresolved items

<!-- {/projectReleaseOutputIncludes} -->
