<!-- {@projectReadmeOverview} -->

`monochange` is a release-planning toolkit for monorepos that span more than one package ecosystem.

It discovers packages, normalizes dependency data, applies group rules, turns explicit change files into release plans, and can run workflow-driven release preparation from those same inputs.

Use it when your repository has outgrown one-ecosystem release tooling and you want one model for Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter.

<!-- {/projectReadmeOverview} -->

<!-- {@projectWhyUse} -->

- use one release-planning model across several language ecosystems
- replace ad hoc scripts with explicit change files and deterministic release output
- keep related packages synchronized with `[group.<id>]`
- propagate dependent bumps through one normalized dependency graph
- expose top-level CLI commands from workflows declared in `monochange.toml`

<!-- {/projectWhyUse} -->

<!-- {@projectCrateCatalog} -->

- `monochange` — end-user CLI and orchestration layer for discovery, planning, and workflow-driven releases.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange-orange?logo=rust)](https://crates.io/crates/monochange) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange-1f425f?logo=docs.rs)](https://docs.rs/monochange/)
- `monochange_core` — shared domain model for packages, dependency edges, groups, change signals, and release plans.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__core-orange?logo=rust)](https://crates.io/crates/monochange_core) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__core-1f425f?logo=docs.rs)](https://docs.rs/monochange_core/)
- `monochange_config` — loads `monochange.toml`, parses `.changeset/*.md`, and validates workflow inputs.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__config-orange?logo=rust)](https://crates.io/crates/monochange_config) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__config-1f425f?logo=docs.rs)](https://docs.rs/monochange_config/)
- `monochange_graph` — propagates release impact through dependency edges and synchronized groups.
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
- coordinate shared package groups from `monochange.toml`
- compute release plans from explicit change input
- expose top-level CLI commands from workflow definitions
- run config-defined release workflows from `.changeset/*.md`
- apply Rust semver evidence when provided
- publish end-user documentation through the mdBook in `docs/`

<!-- {/projectMilestoneCapabilities} -->

<!-- {@repoDevEnvironmentSetupCode} -->

```bash
devenv shell
install:all
mc validate
mc discover --format json
mc change --package monochange --bump minor --reason "add release planning"
mc release --dry-run --format json
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
mc validate
lint:all
test:all
coverage:all
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
mc validate
mc change --package monochange --bump patch --reason "describe the change"
lint:all
test:all
coverage:all
build:all
build:book
```

<!-- {/contributingCoreCommands} -->

<!-- {@projectSetupConfig} -->

```toml
[defaults]
parent_bump = "patch"
warn_on_group_mismatch = true
package_type = "cargo"
changelog = "{path}/changelog.md"

[package.sdk-core]
path = "crates/sdk_core"

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

[[workflows]]
name = "validate"
help_text = "Validate monochange configuration and changesets"

[[workflows.steps]]
type = "Validate"

[[workflows]]
name = "discover"
help_text = "Discover packages across supported ecosystems"

[[workflows.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[workflows.steps]]
type = "Discover"

[[workflows]]
name = "change"
help_text = "Create a change file for one or more packages"

[[workflows.inputs]]
name = "package"
type = "string_list"
required = true

[[workflows.inputs]]
name = "bump"
type = "choice"
choices = ["patch", "minor", "major"]
default = "patch"

[[workflows.inputs]]
name = "reason"
type = "string"
required = true

[[workflows.steps]]
type = "CreateChangeFile"

[[workflows]]
name = "release"
help_text = "Prepare a release from discovered change files"

[[workflows.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[workflows.steps]]
type = "PrepareRelease"
```

<!-- {/projectSetupConfig} -->

<!-- {@projectSetupConfigNote} -->

This guide shows the preferred package/group configuration model together with the default top-level workflows emitted by `mc init`.

If you omit `[[workflows]]`, MonoChange synthesizes the default `validate`, `discover`, `change`, and `release` workflows automatically. Repositories can then customize those commands by declaring workflows explicitly in `monochange.toml`.

<!-- {/projectSetupConfigNote} -->

<!-- {@projectDiscoverCommand} -->

```bash
mc validate
mc discover --format json
```

<!-- {/projectDiscoverCommand} -->

<!-- {@projectDryRunCommand} -->

```bash
mc release --dry-run --format json
```

<!-- {/projectDryRunCommand} -->

<!-- {@projectPlanCommand} -->

```bash
mc release --dry-run --format json
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
- release groups derived from configured groups
- warnings

<!-- {/projectDiscoveryOutputIncludes} -->

<!-- {@projectReleaseOutputIncludes} -->

- per-package bump decisions
- synchronized group outcomes
- compatibility evidence
- warnings and unresolved items

<!-- {/projectReleaseOutputIncludes} -->

<!-- {@projectCoreWorkflow} -->

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
