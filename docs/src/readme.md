# Introduction

`monochange` is a cross-ecosystem release planner for monorepos.

The current milestone focuses on:

<!-- {=projectMilestoneCapabilities} -->

- discover Cargo, npm/pnpm/Bun, Deno, Dart, and Flutter packages
- normalize dependency edges across ecosystems
- coordinate shared package groups from `monochange.toml`
- compute release plans from explicit change input
- expose top-level CLI commands from `[cli.<command>]` definitions
- run config-defined release commands from `.changeset/*.md`
- render changelogs through structured release notes and configurable formats
- emit stable release-manifest JSON for downstream automation
- preview or publish GitHub releases and release pull requests from typed command steps and shared release data
- model deployment intents for downstream automation and merge-driven release commands
- enforce pull-request changeset policy through typed command steps and reusable diagnostics
- apply Rust semver evidence when provided
- publish end-user documentation through the mdBook in `docs/`

<!-- {/projectMilestoneCapabilities} -->

## GitHub automation

<!-- {=projectGitHubAutomationOverview} -->

MonoChange can promote one prepared release into several GitHub-facing automation flows without changing the underlying release-plan model.

- `mc release-manifest` writes a stable JSON artifact for downstream jobs
- `mc publish-release --dry-run --format json` previews GitHub release payloads before publishing
- `mc release-pr --dry-run --format json` previews the release branch, commit, and pull request body
- `mc release-deploy --dry-run --format json` emits deployment intents for configured release targets
- `mc changeset-check --format json --changed-path ...` evaluates pull-request changeset policy from CI-supplied paths and labels

<!-- {/projectGitHubAutomationOverview} -->

## Core workflow

<!-- {=projectCoreWorkflow} -->

Initialize the repository with detected packages, groups, and default CLI commands:

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

Preview the release command:

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
