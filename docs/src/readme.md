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

For automated CI setup, include the `--provider` flag:

```bash
mc init --provider github
```

This configures the `[source]` section, generates CLI commands for `commit-release` and `release-pr`, and creates GitHub Actions workflows.

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

For human-readable local output, `mc release --dry-run` now defaults to terminal-friendly markdown. Use `--format json` for automation, `--format text` when you explicitly want the older plain-text rendering, `--versions` when you only need planned package and group versions, and `--quiet` when you want dry-run behavior without stdout/stderr output.

If you want a slower, more guided walkthrough, continue with [Start here](./guide/00-start-here.md) and [Your first release plan](./guide/02-setup.md).

## What to read next

- [Start here](./guide/00-start-here.md) — install, `mc init`, validation, discovery, and `--dry-run`
- [Installation](./guide/01-installation.md) — npm, Cargo, optional assistant tooling, and repository development setup
- [Your first release plan](./guide/02-setup.md) — generated config first, package ids before groups
- [Configuration reference](./guide/04-configuration.md) — the full package, group, changelog, and CLI model
- [Release planning](./guide/06-release-planning.md) — changesets, dry runs, diff previews, and planning rules
- [Advanced: GitHub automation](./guide/08-github-automation.md) — provider publishing and release requests
- [Advanced: CI, package publishing, and release PR flows](./guide/13-ci-and-publishing.md) — per-provider CI patterns, trusted publishing, and long-running release PR design notes
- [Advanced: Assistant setup and MCP](./guide/09-assistant-setup.md) — optional AI-assisted workflows
- [Reference: Manifest linting with `mc check`](./reference/linting.md) — `[ecosystems.<name>.lints]` rules for Cargo and npm-family manifests

<!-- {=projectRecentPublishingImprovements} -->

### Recent package publishing improvements

Recent `monochange` improvements made package publishing guidance and diagnostics much more actionable:

- a dedicated trusted-publishing guide now covers `npm`, `crates.io`, `jsr`, and `pub.dev`
- CI examples now prefer the official registry-maintained workflows for `crates.io` and `pub.dev`
- a dedicated multi-package publishing guide now covers monorepo tag, workflow, and package-boundary patterns
- CLI output now gives clearer manual next steps for registries that still require registry-side trusted-publishing enrollment
- built-in publish preflight now validates and reports the expected GitHub repository, workflow, and environment context for manual registries when it can infer them

<!-- {/projectRecentPublishingImprovements} -->

## Command and automation matrix

<!-- {=projectCommandAutomationMatrix} -->

| Goal                             | Command                                                     | Use it when                                                                                              |
| -------------------------------- | ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Validate config and changesets   | `mc validate`                                               | You changed `monochange.toml` or `.changeset/*.md` files                                                 |
| Inspect package ids and groups   | `mc discover --format json`                                 | You need the normalized workspace model                                                                  |
| Create release intent            | `mc change --package <id> --bump <severity> --reason "..."` | You need a new `.changeset/*.md` file                                                                    |
| Audit pending release context    | `mc diagnostics --format json`                              | You need git provenance, PR/MR links, or related issues                                                  |
| Preview the release plan         | `mc release --dry-run --diff`                               | You want changelog/version patches without mutating the repo                                             |
| Create a durable release commit  | `mc commit-release`                                         | You want a monochange-managed release commit with an embedded `ReleaseRecord`                            |
| Open or update a release request | `mc release-pr`                                             | You want a long-lived release PR/MR branch updated from current release state                            |
| Publish packages to registries   | `mc publish`                                                | You want `cargo publish`, `npm publish`, `deno publish`, or `dart pub publish` style package publication |
| Bootstrap missing packages       | `mc placeholder-publish`                                    | A package must exist in its registry before later automation can work                                    |
| Inspect a past release commit    | `mc release-record --from <ref>`                            | You need the durable release declaration from git history                                                |
| Create post-merge release tags   | `mc tag-release --from HEAD`                                | You merged a monochange release commit and now need to create and push its declared tag set              |
| Repair a recent release          | `mc repair-release --from <tag> --target <commit>`          | You need to retarget a just-created release to a later commit                                            |
| Publish hosted/provider releases | `mc publish-release`                                        | You want GitHub/GitLab/Gitea release objects from prepared release state                                 |

<!-- {/projectCommandAutomationMatrix} -->

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
- create post-merge release tags from a merged release commit with `mc tag-release --from HEAD`
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
lint:all
test:all
build:all
build:book
```

<!-- {/projectValidationCommands} -->
