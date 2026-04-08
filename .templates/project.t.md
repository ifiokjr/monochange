<!-- {@projectReadmeOverview} -->

`monochange` is a release-planning toolkit for monorepos that span more than one package ecosystem.

It discovers packages, normalizes dependency data, applies group rules, turns explicit change files into release plans, and can run config-defined release preparation from those same inputs.

Use it when your repository has outgrown one-ecosystem release tooling and you want one model for Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter.

<!-- {/projectReadmeOverview} -->

<!-- {@projectWhyUse} -->

- use one release-planning model across several language ecosystems
- replace ad hoc scripts with explicit change files and deterministic release output
- keep related packages synchronized with `[group.<id>]`
- propagate dependent bumps through one normalized dependency graph
- expose top-level CLI commands from `[cli.<command>]` entries in `monochange.toml`

<!-- {/projectWhyUse} -->

<!-- {@projectCrateCatalog} -->

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

<!-- {@projectMilestoneCapabilities} -->

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

<!-- {@projectGitHubAutomationOverview} -->

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

<!-- {@repoDevEnvironmentSetupCode} -->

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

[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"

[release_notes]
change_templates = [
	"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ details }}",
	"- {{ summary }}",
]

[package.sdk-core]
path = "crates/sdk_core"
extra_changelog_sections = [
	{ name = "Security", types = ["security"], default_bump = "patch" },
]

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

[group.sdk.changelog]
path = "docs/sdk-changelog.md"
format = "monochange"

[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[source.releases]
source = "monochange"

[source.pull_requests]
branch_prefix = "monochange/release"
base = "main"
title = "chore(release): prepare release"
labels = ["release", "automated"]
auto_merge = false

[source.bot.changesets]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["crates/**", "packages/**", "npm/**", "skills/**"]
ignored_paths = [
	"docs/**",
	"specs/**",
	"readme.md",
	"CONTRIBUTING.md",
	"license",
]

name = "production"
trigger = "release_pr_merge"
release_targets = ["sdk"]
requires = ["main"]

[cli.validate]
help_text = "Validate monochange configuration and changesets"

[[cli.validate.steps]]
type = "Validate"

[cli.discover]
help_text = "Discover packages across supported ecosystems"

[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.discover.steps]]
type = "Discover"

[cli.change]
help_text = "Create a change file for one or more packages"

[[cli.change.inputs]]
name = "interactive"
type = "boolean"
short = "i"

[[cli.change.inputs]]
name = "package"
type = "string_list"

[[cli.change.inputs]]
name = "bump"
type = "choice"
choices = ["none", "patch", "minor", "major"]
default = "patch"

[[cli.change.inputs]]
name = "version"
type = "string"

[[cli.change.inputs]]
name = "reason"
type = "string"

[[cli.change.inputs]]
name = "type"
type = "string"

[[cli.change.inputs]]
name = "details"
type = "string"

[[cli.change.inputs]]
name = "output"
type = "path"

[[cli.change.steps]]
type = "CreateChangeFile"

[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"

[cli.release-manifest]
help_text = "Prepare a release and write a stable JSON manifest"

[[cli.release-manifest.steps]]
type = "PrepareRelease"

[[cli.release-manifest.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"

[cli.publish-release]
help_text = "Prepare a release and publish provider releases"

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
type = "PrepareRelease"

[[cli.publish-release.steps]]
type = "PublishRelease"

[cli.release-pr]
help_text = "Prepare a release and open or update a provider release request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"

name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

type = "PrepareRelease"

[cli.affected]
help_text = "Evaluate pull-request changeset policy"

[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.affected.inputs]]
name = "label"
type = "string_list"

[[cli.affected.steps]]
type = "AffectedPackages"
```

<!-- {/projectSetupConfig} -->

<!-- {@projectSetupConfigNote} -->

This guide shows the preferred package/group configuration model together with an expanded CLI command surface.

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
