# Introduction

`monochange` is a cross-ecosystem release planner for monorepos.

It is easiest to learn with one safe local walkthrough before you touch provider publishing, release PRs, diagnostics, or MCP setup.

## Who this guide is for

- maintainers of monorepos that span more than one package ecosystem
- teams replacing ad hoc release scripts with explicit change files
- people who want a predictable release plan before adding automation

## Start with one safe walkthrough

Install the prebuilt CLI from npm:

```bash
npm install -g @monochange/cli
monochange --help
mc --help
```

Then run the core beginner flow:

<!-- {=projectCoreWorkflow} -->

Generate a starter config from the packages monochange detects:

```bash
mc init
```

`mc init` writes an annotated `monochange.toml`, so most first-time users can start with the generated file instead of hand-authoring config. If you later want editable copies of the built-in CLI commands, run `mc populate` to append any missing default command definitions to `monochange.toml`.

Validate the workspace:

```bash
mc validate
```

Discover the package ids you will use in commands and changesets:

```bash
mc discover --format json
```

Create one change file for a package id:

```bash
mc change --package <id> --bump patch --reason "describe the change"
```

Most changes should target a package id. Use group ids only when the change is intentionally owned by the whole group.

Preview the release plan safely:

```bash
mc release --dry-run --format json
```

Add `--diff` when you want unified file previews for version and changelog updates without mutating the workspace:

```bash
mc release --dry-run --diff
```

This first run is safe: nothing is published. Stop here until you are ready to prepare release files locally.

When you are ready to prepare the release locally, run `mc release`.

<!-- {/projectCoreWorkflow} -->

If you want a slower, more guided walkthrough, continue with [Start here](./guide/00-start-here.md) and [Your first release plan](./guide/02-setup.md).

## What to read next

- [Start here](./guide/00-start-here.md) — install, `mc init`, validation, discovery, and `--dry-run`
- [Installation](./guide/01-installation.md) — npm, Cargo, optional assistant tooling, and repository development setup
- [Your first release plan](./guide/02-setup.md) — generated config first, package ids before groups
- [Configuration reference](./guide/04-configuration.md) — the full package, group, changelog, and CLI model
- [Advanced: GitHub automation](./guide/08-github-automation.md) — provider publishing and release requests
- [Advanced: Assistant setup and MCP](./guide/09-assistant-setup.md) — optional AI-assisted workflows

## What monochange can do today

<!-- {=projectMilestoneCapabilities} -->

- discover Cargo, npm/pnpm/Bun, Deno, Dart, and Flutter packages
- normalize dependency edges across ecosystems
- coordinate shared package groups from `monochange.toml`
- compute release plans from explicit change input
- expose top-level CLI commands from `[cli.<command>]` definitions
- run config-defined release commands from `.changeset/*.md`
- render changelogs through structured release notes and configurable formats
- emit stable release-manifest JSON for downstream automation
- preview or publish provider releases and release requests from typed command steps and shared release data
- inspect durable release records from tags or descendant commits with `mc release-record`
- repair a recent source/provider release by retargeting its release tags with `mc repair-release`
- inspect changeset context and review metadata with `mc diagnostics` for both human and automation workflows
- apply Rust semver evidence when provided
- expose built-in assistant setup guidance with `mc assist` and a stdio MCP server with `mc mcp`
- publish the CLI as `@monochange/cli` and the bundled agent skill as `@monochange/skill`
- publish end-user documentation through the mdBook in `docs/`

<!-- {/projectMilestoneCapabilities} -->

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
- optional `fileDiffs` previews when you request `--diff`

<!-- {/projectReleaseOutputIncludes} -->

## Contributing to monochange itself

If you are working on the monochange repository, run the full local validation suite before opening a PR:

<!-- {=projectValidationCommands} -->

```bash
docs:verify
lint:all
test:all
build:all
build:book
```

<!-- {/projectValidationCommands} -->
