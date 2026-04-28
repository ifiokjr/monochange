<!-- {@projectReadmeOverview} -->

`monochange` is a release-planning toolkit for monorepos that span more than one package ecosystem.

It discovers packages, normalizes dependency data, applies group rules, turns explicit change files into release plans, and can run config-defined release preparation from those same inputs.

Use it when your repository has outgrown one-ecosystem release tooling and you want one model for Cargo, npm/pnpm/Bun, Deno, Dart/Flutter, and Python.

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
- `monochange_telemtry` — local-only telemetry event sink and privacy-preserving event schema helpers.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__telemtry-orange?logo=rust)](https://crates.io/crates/monochange_telemtry) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__telemtry-1f425f?logo=docs.rs)](https://docs.rs/monochange_telemtry/)
- `monochange_cargo` — Cargo discovery plus Rust semver evidence integration.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__cargo-orange?logo=rust)](https://crates.io/crates/monochange_cargo) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__cargo-1f425f?logo=docs.rs)](https://docs.rs/monochange_cargo/)
- `monochange_npm` — npm, pnpm, and Bun workspace discovery.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__npm-orange?logo=rust)](https://crates.io/crates/monochange_npm) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__npm-1f425f?logo=docs.rs)](https://docs.rs/monochange_npm/)
- `monochange_deno` — Deno workspace and package discovery.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__deno-orange?logo=rust)](https://crates.io/crates/monochange_deno) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__deno-1f425f?logo=docs.rs)](https://docs.rs/monochange_deno/)
- `monochange_dart` — Dart and Flutter workspace discovery.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__dart-orange?logo=rust)](https://crates.io/crates/monochange_dart) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__dart-1f425f?logo=docs.rs)](https://docs.rs/monochange_dart/)
- `monochange_python` — Python uv workspace, Poetry, and pyproject.toml discovery.
  - [![Crates.io](https://img.shields.io/badge/crates.io-monochange__python-orange?logo=rust)](https://crates.io/crates/monochange_python) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__python-1f425f?logo=docs.rs)](https://docs.rs/monochange_python/)

<!-- {/projectCrateCatalog} -->

<!-- {@projectMilestoneCapabilities} -->

- discover Cargo, npm/pnpm/Bun, Deno, Dart, Flutter, and Python packages
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
- expose a bundled assistant skill plus a stdio MCP server with `mc mcp`
- publish the CLI as `@monochange/cli` and the bundled agent skill as `@monochange/skill`
- publish end-user documentation through the mdBook in `docs/`

<!-- {/projectMilestoneCapabilities} -->

<!-- {@projectRecentPublishingImprovements} -->

### Recent package publishing improvements

Recent `monochange` improvements made package publishing guidance and diagnostics much more actionable:

- a dedicated trusted-publishing guide now covers `npm`, `crates.io`, `jsr`, and `pub.dev`
- CI examples now prefer the official registry-maintained workflows for `crates.io` and `pub.dev`
- a dedicated multi-package publishing guide now covers monorepo tag, workflow, and package-boundary patterns
- CLI output now gives clearer manual next steps for registries that still require registry-side trusted-publishing enrollment
- built-in publish preflight now validates and reports the expected GitHub repository, workflow, and environment context for manual registries when it can infer them

<!-- {/projectRecentPublishingImprovements} -->

<!-- {@projectCommandAutomationMatrix} -->

These are the commands most repositories use after running `mc init`. With the new CLI model, workflow names such as `discover`, `change`, `release`, `publish`, and `affected` come from `[cli.*]` tables in `monochange.toml`; hardcoded binary commands such as `validate`, `check`, `init`, and `mcp` stay built in. The underlying built-in steps are always available directly as immutable `mc step:*` commands.

| Goal                             | Command                                                     | Use it when                                                                                              |
| -------------------------------- | ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Validate config and changesets   | `mc validate`                                               | You changed `monochange.toml` or `.changeset/*.md` files                                                 |
| Inspect package ids and groups   | `mc discover --format json`                                 | You need the normalized workspace model                                                                  |
| Create release intent            | `mc change --package <id> --bump <severity> --reason "..."` | You need a new `.changeset/*.md` file                                                                    |
| Audit pending release context    | `mc diagnostics --format json`                              | You need git provenance, PR/MR links, or related issues                                                  |
| Preview the release plan         | `mc release --dry-run --diff`                               | You want changelog/version patches without mutating the repo                                             |
| Create a durable release commit  | `mc commit-release`                                         | You want a monochange-managed release commit with an embedded `ReleaseRecord`                            |
| Open or update a release request | `mc release-pr`                                             | You want a long-lived release PR/MR branch updated from current release state                            |
| Inspect a past release commit    | `mc release-record --from <ref>`                            | You need the durable release declaration from git history                                                |
| Check package publish readiness  | `mc publish-readiness --from HEAD --output <path>`          | You need a validated readiness artifact before package publication                                       |
| Plan ready package publishing    | `mc publish-plan --readiness <path>`                        | You want rate-limit batches that exclude non-ready package work                                          |
| Publish packages to registries   | `mc publish --readiness <path>`                             | You want `cargo publish`, `npm publish`, `deno publish`, or `dart pub publish` style package publication |
| Bootstrap release packages       | `mc publish-bootstrap --from HEAD --output <path>`          | You need a release-record-scoped placeholder bootstrap artifact before rerunning readiness               |
| Create post-merge release tags   | `mc tag-release --from HEAD`                                | You merged a monochange release commit and now need to create and push its declared tag set              |
| Repair a recent release          | `mc repair-release --from <tag> --target <commit>`          | You need to retarget a just-created release to a later commit                                            |
| Publish hosted/provider releases | `mc publish-release`                                        | You want GitHub/GitLab/Gitea release objects from prepared release state                                 |

<!-- {/projectCommandAutomationMatrix} -->

`mc publish-readiness` performs non-mutating registry checks before `mc publish`. For built-in Cargo publishes to crates.io it also verifies current manifest publishability: `publish = false` blocks publishing, `publish = [...]` must include `crates-io`, `description` must be set, and either `license` or `license-file` must be set. Workspace-inherited Cargo metadata is accepted, and already-published versions remain non-blocking when the readiness artifact still matches the current package set. `mc publish-plan --readiness <path>` validates the same artifact for planning and limits rate-limit batches to package ids that are ready in both the artifact and the fresh local readiness check. If readiness shows missing first-time registry packages, run `mc publish-bootstrap --from HEAD --output .monochange/bootstrap-result.json`, then rerun readiness before real publishing. Python packages support built-in PyPI publishing with `uv build` and `uv publish`; keep `mode = "external"` for private registries or custom Python publication flows.

<!-- {@projectCapabilityMatrix} -->

| Capability                                                                     | Current status                                                                                                 |
| ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| Multi-ecosystem discovery                                                      | Cargo, npm/pnpm/Bun, Deno, Dart, Flutter, Python                                                               |
| Package release planning                                                       | Built in                                                                                                       |
| Grouped/shared versioning                                                      | Built in                                                                                                       |
| Dry-run release diff previews                                                  | Built in via `mc release --dry-run --diff`                                                                     |
| Durable release history and post-merge tagging                                 | Built in via `ReleaseRecord`, `mc release-record`, `mc tag-release`, and `mc repair-release`                   |
| Hosted provider releases                                                       | GitHub, GitLab, Gitea                                                                                          |
| Hosted release requests                                                        | GitHub, GitLab, Gitea                                                                                          |
| Python release planning                                                        | Built in for discovery, version rewrites, dependency rewrites, lockfile command inference, and PyPI publishing |
| Built-in registry publishing                                                   | `crates.io`, `npm`, `jsr`, `pub.dev`, `pypi`; use external mode for custom registries                          |
| GitHub npm trusted-publishing automation                                       | Built in                                                                                                       |
| GitHub trusted-publishing guidance for `crates.io`, `jsr`, `pub.dev`, and PyPI | Built in, but manual registry enrollment is still required                                                     |
| GitLab trusted-publishing auto-derivation                                      | Not built in today                                                                                             |
| Release-retarget sync for hosted releases                                      | GitHub first                                                                                                   |

<!-- {/projectCapabilityMatrix} -->

<!-- {@projectGitHubAutomationOverview} -->

monochange can promote one prepared release into several source-provider automation flows without changing the underlying release-plan model.

- `mc release --dry-run --format json` refreshes the cached manifest and shows downstream automation data, including authored changesets plus linked release context metadata
- `mc publish-release --dry-run --format json` previews provider release payloads before publishing
- `mc release-pr --dry-run --format json` previews the release branch, commit, and release-request body
- when `[source.pull_requests].verified_commits = true` and `mc release-pr` runs on GitHub Actions for the configured GitHub repository, the GitHub provider pushes a normal release branch commit first, then attempts to replace it with a Git Database API commit that GitHub reports as verified; if verification or the API update fails, the normal pushed commit remains in place
- `mc release-record --from <tag>` inspects the durable release declaration stored in the release commit body
- `mc tag-release --from HEAD --dry-run --format json` previews the post-merge release tag set declared by that durable record
- `mc repair-release --from <tag> --dry-run` previews a release-retarget plan before mutating tags
- changelog templates can render linked change owners, review requests, commits, and closed issues through `{{ context }}` or fine-grained metadata variables
- `mc step:affected-packages --format json --verify --changed-paths ...` evaluates pull-request changeset policy from CI-supplied paths and labels without requiring a config-defined wrapper command
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
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
mc release-record --from v1.2.3
mc tag-release --from HEAD --dry-run --format json
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc publish-bootstrap --from HEAD --output .monochange/bootstrap-result.json
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc publish-plan --readiness .monochange/readiness.json --format json
mc publish --readiness .monochange/readiness.json
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
mc validate
lint:all
test:all
coverage:all
coverage:patch
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
mc validate
mc change --package monochange --bump patch --reason "describe the change"
lint:all
test:all
coverage:all
coverage:patch
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

[cli.discover]
help_text = "Discover packages across supported ecosystems"

[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.discover.steps]]
name = "discover packages"
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
name = "create change file"
type = "CreateChangeFile"

[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
name = "prepare release"
type = "PrepareRelease"

[cli.publish-release]
help_text = "Prepare a release and publish provider releases"

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
name = "prepare release"
type = "PrepareRelease"

[[cli.publish-release.steps]]
name = "publish release"
type = "PublishRelease"

[cli.release-pr]
help_text = "Prepare a release and open or update a provider release request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-pr.steps]]
name = "prepare release"
type = "PrepareRelease"

[[cli.release-pr.steps]]
name = "open release request"
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
name = "evaluate affected packages"
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
- optional `fileDiffs` previews when you request `--diff`

<!-- {/projectReleaseOutputIncludes} -->

<!-- {@projectCoreWorkflow} -->

Generate a starter config from the packages monochange detects:

```bash
mc init
```

`mc init` writes an annotated `monochange.toml`, including starter `[cli.*]` workflow commands such as `discover`, `change`, `release`, `publish`, and `affected`. Those generated tables are the editable source of truth for named workflow commands; the binary also exposes immutable `mc step:*` commands for every built-in step when you need a direct, config-free entry point.

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

When a package is only changing because another dependency or version group moved first, author that context explicitly instead of relying on anonymous propagation:

```bash
mc change --package <dependent-id> --bump none --caused-by <upstream-id> --reason "dependency-only follow-up"
```

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
