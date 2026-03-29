<!-- {@projectReadmeOverview} -->

`monochange` is a release-planning toolkit for monorepos that span more than one package ecosystem.

It discovers packages, normalizes dependency data, applies version-group rules, turns explicit change files into release plans, and can run workflow-driven release preparation from those same inputs.

Use it when your repository has outgrown one-ecosystem release tooling and you want one model for Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter.

<!-- {/projectReadmeOverview} -->

<!-- {@projectWhyUse} -->

- use one release-planning model across several language ecosystems
- replace ad hoc scripts with explicit change files and deterministic release output
- keep related packages synchronized with configured groups
- propagate dependent bumps through one normalized dependency graph
- embed the same discovery and planning logic in your own tooling through the workspace crates

<!-- {/projectWhyUse} -->

<!-- {@projectCrateCatalog} -->

- `monochange` — end-user CLI and orchestration layer for discovery, planning, and workflow-driven releases.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange-orange?logo=rust)](https://crates.io/crates/monochange) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange-1f425f?logo=docs.rs)](https://docs.rs/monochange/)
- `monochange_core` — shared domain model for packages, dependency edges, version groups, change signals, and release plans.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__core-orange?logo=rust)](https://crates.io/crates/monochange_core) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__core-1f425f?logo=docs.rs)](https://docs.rs/monochange_core/)
- `monochange_config` — loads `monochange.toml`, parses `.changeset/*.md`, and validates planning inputs.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__config-orange?logo=rust)](https://crates.io/crates/monochange_config) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__config-1f425f?logo=docs.rs)](https://docs.rs/monochange_config/)
- `monochange_graph` — propagates release impact through dependency edges and synchronized version groups.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__graph-orange?logo=rust)](https://crates.io/crates/monochange_graph) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__graph-1f425f?logo=docs.rs)](https://docs.rs/monochange_graph/)
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

<!-- {@projectMilestoneCapabilities} -->

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

<!-- {@repoDevEnvironmentSetupCode} -->

```bash
devenv shell
install:all
mc check --root .
mc workspace discover --root . --format json
mc changes add --root . --package monochange --bump minor --reason "add release planning"
mc release --dry-run
mc release
```

<!-- {/repoDevEnvironmentSetupCode} -->

<!-- {@repoCommonDevelopmentCommands} -->

```bash
monochange --help
mc --help
docs:check
docs:update
docs:verify
docs:doctor
mc check --root .
lint:all
test:all
build:all
build:book
```

<!-- {/repoCommonDevelopmentCommands} -->

<!-- {@contributingCoreCommands} -->

```bash
monochange --help
mc --help
docs:check
docs:update
docs:verify
docs:doctor
mc check --root .
mc changes add --root . --package monochange --bump patch --reason "describe the change"
lint:all
test:all
build:all
build:book
```

<!-- {/contributingCoreCommands} -->

<!-- {@projectSetupConfig} -->

```toml
[defaults]
parent_bump = "patch"
warn_on_group_mismatch = true

[package.sdk-core]
path = "crates/sdk_core"
type = "cargo"
changelog = "crates/sdk_core/CHANGELOG.md"

[package.web-sdk]
path = "packages/web-sdk"
type = "npm"

[package.mobile-sdk]
path = "packages/mobile-sdk"
type = "dart"

[group.sdk]
packages = ["sdk-core", "web-sdk", "mobile-sdk"]
tag = true
release = true
version_format = "primary"

[ecosystems.npm]
enabled = true

[ecosystems.deno]
enabled = true

[ecosystems.dart]
enabled = true

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
```

<!-- {/projectSetupConfig} -->

<!-- {@projectSetupConfigNote} -->

This guide shows the preferred package/group configuration model.

If you are migrating from older config, replace legacy `[[version_groups]]` and `[[package_overrides]]` entries with `[group.<id>]` and `[package.<id>]` declarations before relying on `mc check` and `mc release`.

<!-- {/projectSetupConfigNote} -->

<!-- {@projectDiscoverCommand} -->

```bash
mc check --root .
mc workspace discover --root . --format json
```

<!-- {/projectDiscoverCommand} -->

<!-- {@projectDryRunCommand} -->

```bash
mc release --dry-run
```

<!-- {/projectDryRunCommand} -->

<!-- {@projectPlanCommand} -->

```bash
mc plan release --root . --changes .changeset/my-change.md --format json
```

<!-- {/projectPlanCommand} -->

<!-- {@projectReleaseCommand} -->

```bash
mc release
```

<!-- {/projectReleaseCommand} -->

<!-- {@projectValidationCommands} -->

```bash
docs:verify
lint:all
test:all
build:all
build:book
```

<!-- {/projectValidationCommands} -->

<!-- {@projectDiscoveryOutputIncludes} -->

- normalized package records
- dependency edges
- version groups derived from configured groups
- warnings

<!-- {/projectDiscoveryOutputIncludes} -->

<!-- {@projectReleaseOutputIncludes} -->

- per-package bump decisions
- synchronized group outcomes
- compatibility evidence
- warnings and unresolved items

<!-- {/projectReleaseOutputIncludes} -->

<!-- {@projectCoreWorkflow} -->

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
