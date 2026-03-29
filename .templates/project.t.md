<!-- {@projectMilestoneCapabilities} -->

- discover Cargo, npm/pnpm/Bun, Deno, Dart, and Flutter packages
- normalize dependency edges across ecosystems
- coordinate shared version groups from `monochange.toml`
- compute release plans from explicit change input
- run config-defined release workflows from `.changeset/*.md`
- apply Rust semver evidence when provided
- publish end-user documentation through the mdBook in `docs/`

<!-- {/projectMilestoneCapabilities} -->

<!-- {@repoDevEnvironmentSetupCode} -->

```bash
devenv shell
install:all
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
mc changes add --root . --package crates/monochange --bump patch --reason "describe the change"
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

[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk"]

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
```

<!-- {/projectSetupConfig} -->

<!-- {@projectSetupConfigNote} -->

This is the smallest config needed to make `mc release` work in the current implementation.

For changelog updates, add `[[package_overrides]]` entries with `changelog` paths. Discovery currently scans all supported ecosystems automatically; the top-level `[ecosystems.*]` settings are parsed today but are not yet used to filter discovery.

<!-- {/projectSetupConfigNote} -->

<!-- {@projectDiscoverCommand} -->

```bash
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
- version groups
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

[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk"]

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
```

This is the smallest config needed to make `mc release` work in the current implementation.

For changelog updates, add `[[package_overrides]]` entries with `changelog` paths. Discovery currently scans all supported ecosystems automatically; the top-level `[ecosystems.*]` settings are parsed today but are not yet used to filter discovery.

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
