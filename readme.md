# monochange

> manage versions and releases for your multiplatform, multilanguage monorepo

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=projectReadmeOverview} -->

`monochange` is a release-planning toolkit for monorepos that span more than one package ecosystem.

It discovers packages, normalizes dependency data, applies group rules, turns explicit change files into release plans, and can run config-defined release preparation from those same inputs.

Use it when your repository has outgrown one-ecosystem release tooling and you want one model for Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter.

<!-- {/projectReadmeOverview} -->

## Who `monochange` is for

- maintainers of monorepos that span more than one package ecosystem
- teams replacing ad hoc release scripts with explicit, reviewable change files
- people who want one safe release-planning model before adding provider automation

## First 10 minutes

Install the prebuilt CLI from npm:

```bash
npm install -g @monochange/cli
monochange --help
mc --help
```

If you prefer a Rust-native install, use `cargo install monochange` instead.

Then run one safe local walkthrough:

<!-- {=projectCoreWorkflow} -->

Generate a starter config from the packages monochange detects:

```bash
mc init
```

`mc init` writes an annotated `monochange.toml`, so most first-time users can start with the generated file instead of hand-authoring config.

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

This first run is safe: nothing is published. Stop here until you are ready to prepare release files locally.

When you are ready to prepare the release locally, run `mc release`.

<!-- {/projectCoreWorkflow} -->

If you do not know which package id to target, rerun `mc discover --format json` and copy an id directly from the output.

## Next steps

- [Start here](docs/src/guide/00-start-here.md) — the shortest beginner path through installation, `mc init`, and `--dry-run`
- [Installation](docs/src/guide/01-installation.md) — npm, Cargo, optional assistant tooling, and repository-development setup
- [Your first release plan](docs/src/guide/02-setup.md) — a fuller walkthrough built around generated config
- [Discovery](docs/src/guide/03-discovery.md) — what monochange finds and how ids are rendered
- [Configuration reference](docs/src/guide/04-configuration.md) — evolve the generated config once the basics feel familiar
- [Groups and shared release identity](docs/src/guide/05-version-groups.md) — when to reach for group ids instead of package ids

## Why use `monochange`?

<!-- {=projectWhyUse} -->

- use one release-planning model across several language ecosystems
- replace ad hoc scripts with explicit change files and deterministic release output
- keep related packages synchronized with `[group.<id>]`
- propagate dependent bumps through one normalized dependency graph
- expose top-level CLI commands from `[cli.<command>]` entries in `monochange.toml`

<!-- {/projectWhyUse} -->

## Advanced workflows

### GitHub automation

<!-- {=projectGitHubAutomationOverview} -->

monochange can promote one prepared release into several source-provider automation flows without changing the underlying release-plan model.

- `mc release-manifest` writes a stable JSON artifact for downstream jobs, including authored changesets plus linked release context metadata
- `mc publish-release --dry-run --format json` previews provider release payloads before publishing
- `mc release-pr --dry-run --format json` previews the release branch, commit, and release-request body
- `mc release-record --from <tag>` inspects the durable release declaration stored in the release commit body
- `mc repair-release --from <tag> --dry-run` previews a release-retarget plan before mutating tags
- changelog templates can render linked change owners, review requests, commits, and closed issues through `{{ context }}` or fine-grained metadata variables
- `mc affected --format json --changed-paths ...` evaluates pull-request changeset policy from CI-supplied paths and labels
- `mc diagnostics --format json` shows all discovered changeset context or restricts to explicit inputs

<!-- {/projectGitHubAutomationOverview} -->

### Assistant setup and MCP

Assistant tooling is optional.

When you want AI-assisted workflows, monochange ships built-in setup guidance and an MCP server:

```bash
npm install -g @monochange/skill
monochange-skill --print-install
mc assist pi
mc mcp
```

See [Advanced: Assistant setup and MCP](docs/src/guide/09-assistant-setup.md) for the full setup flow.

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

## Workspace crates

<!-- {=projectCrateCatalog} -->

- `monochange` — end-user CLI and orchestration layer for discovery, planning, and CLI-defined release commands.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange-orange?logo=rust)](https://crates.io/crates/monochange) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange-1f425f?logo=docs.rs)](https://docs.rs/monochange/)
- `monochange_core` — shared domain model for packages, dependency edges, groups, change signals, and release plans.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__core-orange?logo=rust)](https://crates.io/crates/monochange_core) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__core-1f425f?logo=docs.rs)](https://docs.rs/monochange_core/)
- `monochange_config` — loads `monochange.toml`, parses `.changeset/*.md`, and validates CLI command inputs.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__config-orange?logo=rust)](https://crates.io/crates/monochange_config) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__config-1f425f?logo=docs.rs)](https://docs.rs/monochange_config/)
- `monochange_graph` — propagates release impact through dependency edges and synchronized groups.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__graph-orange?logo=rust)](https://crates.io/crates/monochange_graph) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__graph-1f425f?logo=docs.rs)](https://docs.rs/monochange_graph/)
- `monochange_github` — converts release manifests into GitHub release payloads and publishing operations.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__github-orange?logo=rust)](https://crates.io/crates/monochange_github) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__github-1f425f?logo=docs.rs)](https://docs.rs/monochange_github/)
- `monochange_semver` — merges requested bumps with compatibility-provider evidence.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__semver-orange?logo=rust)](https://crates.io/crates/monochange_semver) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__semver-1f425f?logo=docs.rs)](https://docs.rs/monochange_semver/)
- `monochange_cargo` — Cargo discovery plus Rust semver evidence integration.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__cargo-orange?logo=rust)](https://crates.io/crates/monochange_cargo) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__cargo-1f425f?logo=docs.rs)](https://docs.rs/monochange_cargo/)
- `monochange_npm` — npm, pnpm, and Bun workspace discovery.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__npm-orange?logo=rust)](https://crates.io/crates/monochange_npm) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__npm-1f425f?logo=docs.rs)](https://docs.rs/monochange_npm/)
- `monochange_deno` — Deno workspace and package discovery.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__deno-orange?logo=rust)](https://crates.io/crates/monochange_deno) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__deno-1f425f?logo=docs.rs)](https://docs.rs/monochange_deno/)
- `monochange_dart` — Dart and Flutter workspace discovery.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__dart-orange?logo=rust)](https://crates.io/crates/monochange_dart) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__dart-1f425f?logo=docs.rs)](https://docs.rs/monochange_dart/)

<!-- {/projectCrateCatalog} -->

## Repository development

Enter the reproducible development shell and install workspace tooling:

<!-- {=repoDevEnvironmentSetupCode} -->

```bash
devenv shell
install:all
mc validate
mc discover --format json
mc change --package monochange --bump minor --reason "add release planning"
mc diagnostics --format json
mc release --dry-run --format json
mc release-manifest --dry-run
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
mc release-record --from v1.2.3
mc repair-release --from v1.2.3 --target HEAD --dry-run
mc release
```

<!-- {/repoDevEnvironmentSetupCode} -->

Useful commands:

<!-- {=repoCommonDevelopmentCommands} -->

```bash
monochange --help
mc --help
docs:check
docs:update
docs:verify
docs:doctor
mc validate
lint:all
test:all
coverage:all
build:all
build:book
```

<!-- {/repoCommonDevelopmentCommands} -->

See `docs/` for user-facing guides and `CONTRIBUTING.md` for contribution expectations.

<!-- {=monochangeBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange
[docs-image]: https://img.shields.io/badge/docs.rs-monochange-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange/

<!-- {/monochangeBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
