# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-30)

### Changed

#### Add Go ecosystem support

monochange now discovers and manages Go modules from `go.mod` files in single-module and multi-module repositories.

**Configuration:**

```toml
[defaults]
package_type = "go"

[package.api]
path = "api"

[package.shared]
path = "shared"

[ecosystems.go]
enabled = true
```

**What it discovers:**

- Go modules by scanning for `go.mod` files
- Multi-module monorepos with separate modules in subdirectories
- Module paths, including major version suffixes (`/v2`, `/v3`)
- Cross-module `require` directives as dependency edges
- Indirect dependencies marked as development dependencies

**Version management:**

- Go versions come from git tags, not manifest files — the adapter reports `None` for `current_version` and stores the module path as metadata for tag resolution
- Updates `require` directives in `go.mod` when cross-module dependencies change
- Preserves `replace`, `exclude`, `retract` directives and comments
- Adds `v` prefix to version strings automatically when missing

**Lockfile commands:**

- Infers `go mod tidy` for all Go modules (updates both `go.mod` and `go.sum`)
- Configurable via `[ecosystems.go].lockfile_commands`

**Key design decisions:**

- Module names are derived from the last non-version segment of the module path (`github.com/org/repo/api/v2` → `api`)
- The full module path and relative directory path are stored as metadata for downstream tag resolution
- Parse errors during discovery are treated as warnings, not hard errors

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #156](https://github.com/monochange/monochange/pull/156) _Introduced in:_ [`519f841`](https://github.com/monochange/monochange/commit/519f841929c6a06d5b3a578b206982d2d6cc1548) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#133](https://github.com/monochange/monochange/issues/133)

#### Add Python ecosystem support

monochange now discovers and manages Python packages from uv workspaces, Poetry projects, and standalone `pyproject.toml` files.

**Configuration:**

```toml
[defaults]
package_type = "python"

[package.core]
path = "packages/core"

[ecosystems.python]
enabled = true
lockfile_commands = [{ command = "uv lock" }]
```

**What it discovers:**

- uv workspaces via `[tool.uv.workspace].members` glob patterns
- Poetry projects via `[tool.poetry]` sections
- Standalone `pyproject.toml` files with PEP 621 `[project]` metadata

**Version management:**

- Reads and updates `[project].version` in `pyproject.toml`
- Parses PEP 440 versions and maps to semver (e.g., `1.2` → `1.2.0`)
- Updates dependency version constraints in `[project].dependencies`
- Handles `dynamic = ["version"]` gracefully (reports `None` for dynamic versions)

**Lockfile commands:**

- Infers `uv lock` for uv projects (detected by `uv.lock`)
- Infers `poetry lock --no-update` for Poetry projects (detected by `poetry.lock`)
- Configurable via `[ecosystems.python].lockfile_commands`

**Dependency extraction:**

- PEP 621 `[project].dependencies` and `[project.optional-dependencies]`
- Poetry `[tool.poetry.dependencies]` and `[tool.poetry.group.*.dependencies]`
- PEP 503 name normalization for cross-package dependency matching
- PEP 508 version specifier parsing with extras support

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #152](https://github.com/monochange/monochange/pull/152) _Introduced in:_ [`81b0882`](https://github.com/monochange/monochange/commit/81b0882525ab51d74b0e8cc2a0114aac0fdb3a7f) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#132](https://github.com/monochange/monochange/issues/132)

#### Add PyPI publishing support

Add first-class PyPI publishing support for Python packages, including PyPI as the built-in Python registry, placeholder package generation, `uv build` / `uv publish` command generation, PyPI JSON API publication checks, trusted-publisher setup guidance, and rate-limit planning coverage.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #322](https://github.com/monochange/monochange/pull/322) _Introduced in:_ [`afe5959`](https://github.com/monochange/monochange/commit/afe595944146c3ec4c9798098c92c031d4279e6a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Update repository URLs

Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add local-only telemetry events

Users can now opt in to local-only telemetry for CLI support and debugging without sending data over the network. By default, nothing is recorded. Setting `MC_TELEMETRY=local` writes OpenTelemetry-style JSON Lines events to the user state directory, while `MC_TELEMETRY_FILE=/path/to/telemetry.jsonl` writes to a chosen file.

Command:

```bash
MC_TELEMETRY=local mc validate
MC_TELEMETRY_FILE=/tmp/mc-telemetry.jsonl mc validate
```

The recorded events are limited to low-cardinality command and step metadata such as command name, step kind, duration, outcome, and sanitized error category. The local sink does not record package names, repository paths, repository URLs, branch names, tags, commit hashes, command strings, raw error messages, or release-note text.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #302](https://github.com/monochange/monochange/pull/302) _Introduced in:_ [`72a126c`](https://github.com/monochange/monochange/commit/72a126cf0789ffbf1c8866f043f307c9f570b088) _Last updated in:_ [`46abfea`](https://github.com/monochange/monochange/commit/46abfea87db6ffed185a0739f5e15c46532192d6) _Related issues:_ [#295](https://github.com/monochange/monochange/issues/295), [#296](https://github.com/monochange/monochange/issues/296), [#297](https://github.com/monochange/monochange/issues/297), [#298](https://github.com/monochange/monochange/issues/298), [#299](https://github.com/monochange/monochange/issues/299), [#300](https://github.com/monochange/monochange/issues/300)

#### Add OXC JavaScript tooling

Add OXC tooling for all JavaScript and TypeScript in the project

- Integrate `dprint-plugin-oxc` as the formatter for JS/TS files, replacing the dprint typescript plugin.
- Add `oxfmt` and `oxlint` configuration (`.oxfmtrc.json` and `.oxlintrc.json`) with rules adapted from the sibling `actions` repo.
- Add `tsgo` for type-checking and `tsdown` for bundling, with a root `tsdown.config.json`.
- Wire everything into `devenv.nix` with new scripts: `lint:js`, `lint:js:syntax`, `lint:js:types`, `fix:js`, and `build:js`. Update `lint:all` and `fix:all` to include the JS checks.
- Add JS devDependencies (`oxfmt`, `oxlint`, `@rslint/tsgo`, `tsdown`) to `package.json`.
- Add JS dependency installation (`pnpm install --frozen-lockfile`) to the shared `devenv` GitHub Action so CI has the tools available.
- Add `lint:js:syntax` check to the CI `lint` job.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #286](https://github.com/monochange/monochange/pull/286) _Introduced in:_ [`09298d3`](https://github.com/monochange/monochange/commit/09298d326fa75e4229080159051fa1c3e6363794) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add the publish bootstrap command

Add `mc publish-bootstrap --from <ref>` for release-record-scoped first-time package setup.

The command uses the release record to choose package ids, runs placeholder publishing for that release package set, supports `--dry-run`, and can write a JSON bootstrap result artifact with `--output <path>`. Documentation now recommends rerunning `mc publish-readiness` after bootstrap before planning or publishing packages.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #318](https://github.com/monochange/monochange/pull/318) _Introduced in:_ [`cadb6fc`](https://github.com/monochange/monochange/commit/cadb6fccaac2ff9107db8b03bf6156762bc5a9b2) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add readiness-backed publish planning

`mc publish-plan` now accepts `--readiness <path>` for normal package publish planning. The plan validates that the `mc publish-readiness` artifact matches the current release record and covers the selected package set, then limits rate-limit batches to package ids that are ready in both the artifact and a fresh local readiness check.

Placeholder publish planning continues to reject readiness artifacts and should be run with `mc publish-plan --mode placeholder`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #305](https://github.com/monochange/monochange/pull/305) _Introduced in:_ [`e80c2b8`](https://github.com/monochange/monochange/commit/e80c2b8f1fd1df155e4aa05df8977f245f89bbc5) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Require publish readiness artifacts

Require real `mc publish` package-registry runs to pass a readiness artifact generated by `mc publish-readiness`.

`mc publish-readiness` JSON artifacts now include schema metadata, release-record commit metadata, and a deterministic package-set fingerprint. `PublishPackages` validates the artifact before registry mutation and rejects missing, blocked, stale, malformed, duplicate, or package-mismatched readiness artifacts while leaving `--dry-run` publish previews artifact-free.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #301](https://github.com/monochange/monochange/pull/301) _Introduced in:_ [`97337aa`](https://github.com/monochange/monochange/commit/97337aad65e1f9dfc4d97fd381592b3bd57bc30a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add publish resume support

Add package publish result artifacts plus `mc publish --resume <path>` for retrying incomplete registry publishing after partial failures.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #323](https://github.com/monochange/monochange/pull/323) _Introduced in:_ [`8bca357`](https://github.com/monochange/monochange/commit/8bca35730a78f61c22dc71e473bc67a77210c4c6) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add release branch policy enforcement

monochange can now enforce that release tags and registry publishing only run from commits that are reachable from configured release branches. This moves release-branch safety from ad hoc CI shell checks into reusable CLI behavior that works in detached CI checkouts.

**Before (`monochange.toml`):**

```toml
[source]
provider = "github"
owner = "acme"
repo = "widgets"
```

Release workflows had to add their own branch guard scripts before running commands such as `mc tag-release` or `mc publish`.

**After (`monochange.toml`):**

```toml
[source]
provider = "github"
owner = "acme"
repo = "widgets"

[source.releases]
branches = ["main", "release/*"]
enforce_for_tags = true
enforce_for_publish = true
enforce_for_commit = false
```

`mc tag-release`, `PublishRelease`, and `PublishPackages` now reject release refs that are not reachable from one of the configured release branch patterns. `CommitRelease` remains usable on release-preparation branches by default, but projects can opt into the same guard with `enforce_for_commit = true`.

**Before (manual CI guard):**

```bash
git fetch origin main
# custom shell checks here
mc tag-release --from v1.2.0 --push
```

**After (CLI-native guard):**

```bash
mc step:verify-release-branch --from v1.2.0
mc tag-release --from v1.2.0 --push
```

The explicit `step:verify-release-branch` command is available for pipelines that want an early, named verification step, while mutation commands still enforce the policy internally when the relevant `enforce_for_*` setting is enabled.

**Before (`monochange_core::ProviderReleaseSettings`):**

```rust
ProviderReleaseSettings {
    enabled,
    draft,
    prerelease,
    generate_notes,
    source,
}
```

**After:**

```rust
ProviderReleaseSettings {
    enabled,
    draft,
    prerelease,
    generate_notes,
    source,
    branches,
    enforce_for_tags,
    enforce_for_publish,
    enforce_for_commit,
}
```

Callers constructing `ProviderReleaseSettings` directly should include the release branch policy fields or use `ProviderReleaseSettings::default()` to keep the default `main` branch policy.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #321](https://github.com/monochange/monochange/pull/321) _Introduced in:_ [`e888554`](https://github.com/monochange/monochange/commit/e888554ca981816b80f5135086e8b226ee8f0a20) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#310](https://github.com/monochange/monochange/issues/310)

#### Document the generated CLI process

Document the new CLI process where `mc init` generates editable workflow commands in `monochange.toml` and every built-in step is available directly through immutable `mc step:*` commands. The docs now clarify reserved command names such as `validate` and recommend `mc step:affected-packages --verify` for direct changeset-policy checks.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #291](https://github.com/monochange/monochange/pull/291) _Introduced in:_ [`74ac16a`](https://github.com/monochange/monochange/commit/74ac16af949ab07644c9b583774a00da2d95a7be) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Prefer verified GitHub release PR commits

When `[source.pull_requests].verified_commits = true`, `mc release-pr` publishes a GitHub release pull request from GitHub Actions by asking the GitHub provider to recreate the release branch commit through the Git Database API and only moves the branch when GitHub marks the replacement commit as verified.

The setting defaults to `false`. If the API commit cannot be created, is not verified, or the branch changes before the replacement lands, monochange leaves the normal pushed git commit in place and continues with the release PR flow.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #306](https://github.com/monochange/monochange/pull/306) _Introduced in:_ [`f93547d`](https://github.com/monochange/monochange/commit/f93547d919cf5bcbffe61beb675fa053307520c8) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

### Added

#### Add `mc skill` for project-local monochange skill installation through the upstream `skills add` workflow.

Before:

```bash
npm install -g @monochange/skill
monochange-skill --copy ~/.pi/agent/skills/monochange
```

After:

```bash
mc help skill
mc skill
mc skill -a pi -y
mc skill --list
```

`mc skill` auto-detects `npx`, `pnpm dlx`, or `bunx`, forwards the remaining native `skills add` flags, and installs the monochange skill source into the current project by default.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #257](https://github.com/monochange/monochange/pull/257) _Introduced in:_ [`0a6937a`](https://github.com/monochange/monochange/commit/0a6937ac81dadbc6252e5eac60fd692335cee3a5) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### Add a package-scoped `mc analyze` CLI command for release-aware semantic analysis.

The command defaults `--release-ref` to the most recent tag for the selected package or its owning version group, compares `main -> head` for first releases when no prior tag exists, and supports text or JSON output for package-focused review workflows.

`monochange_config` now reserves the built-in `analyze` command name so workspace CLI definitions cannot collide with the new built-in subcommand.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #256](https://github.com/monochange/monochange/pull/256) _Introduced in:_ [`75e3329`](https://github.com/monochange/monochange/commit/75e33295d17248f4daca6e7bc83988855a033a08) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add per-crate Codecov coverage flags and crate-specific coverage badges

monochange now uploads one Codecov coverage flag per public crate while keeping the existing workspace-wide upload.

**Before:**

- Codecov only received the overall workspace LCOV upload
- crate READMEs linked their coverage badge to the shared repository-wide Codecov page
- Codecov patch coverage enforced a 100% target for PR status checks

**After:**

- CI splits the workspace LCOV report into one upload per public crate using a Codecov flag named after the crate
- each published crate README now points its coverage badge at that crate’s own Codecov flag page, for example `?flag=monochange_core`
- the repository keeps the overall workspace coverage upload and lowers the Codecov patch coverage status target to 95%

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #255](https://github.com/monochange/monochange/pull/255) _Introduced in:_ [`26e13ff`](https://github.com/monochange/monochange/commit/26e13fff071e93dc32fe071a5771232c980ebd46) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Changed

#### static npm packages in packages/ directory

All npm packages now live as static directories under `packages/` instead of being dynamically generated during the release workflow.

**Before:**

The `@monochange/cli` and platform packages were generated on-the-fly by `build-packages.mjs` into a temporary directory, then published from there. `@monochange/skill` lived in `npm/skill`.

**After:**

Package directories are permanently present under `packages/` using the `@scope__name` convention:

```
packages/monochange__cli/              # @monochange/cli
packages/monochange__cli-darwin-arm64/  # @monochange/cli-darwin-arm64
packages/monochange__skill/            # @monochange/skill
...
```

`build-packages.mjs` still runs during release to populate platform binaries into `packages/*/bin/`, but it no longer generates the package structure from scratch. `publish-packages.mjs` now validates that each package has the expected binaries before publishing, preventing accidental empty publishes.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #204](https://github.com/monochange/monochange/pull/204) _Introduced in:_ [`a90638b`](https://github.com/monochange/monochange/commit/a90638b911d0aca00afcda8c5686da46ead14831) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Documentation

#### Document the new 100% patch-coverage requirement in the generated repository command lists.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #246](https://github.com/monochange/monochange/pull/246) _Introduced in:_ [`62a4f85`](https://github.com/monochange/monochange/commit/62a4f856af4528d9cbacdd7719c5cfb538fbb1c3) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add `mc tag-release` for post-merge release PR workflows

monochange now ships a first-class `mc tag-release` command for the long-running release PR flow.

**Before:**

```bash
mc release-record --from HEAD --format json
mc publish
```

That let CI detect a merged monochange release commit and publish package registries from the durable `ReleaseRecord`, but monochange did not have a built-in command to create and push the release tag set after merge.

**After:**

```bash
mc release-record --from HEAD --format json
mc tag-release --from HEAD
mc publish
```

`mc tag-release` reads the durable `ReleaseRecord` on the merged release commit, creates the declared tag set on that commit, and pushes those tags to `origin`.

**Before (generated GitHub Actions `release.yml`):**

```yaml
- name: prepare and open release PR
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: mc commit-release
```

**After:**

```yaml
- name: refresh release PR
  if: steps.release_record.outputs.is_release_commit != 'true'
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: mc release-pr

- name: create release tags
  if: steps.release_record.outputs.is_release_commit == 'true'
  run: mc tag-release --from HEAD

- name: publish packages
  if: steps.release_record.outputs.is_release_commit == 'true'
  run: mc publish
```

The generated GitHub workflow now refreshes the release PR on normal `main` pushes, then switches into post-merge tagging and package publication when `HEAD` is already the merged monochange release commit.

The bundled `@monochange/cli` documentation now describes this post-merge tagging flow as part of the recommended release PR workflow.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #220](https://github.com/monochange/monochange/pull/220) _Introduced in:_ [`cf5e581`](https://github.com/monochange/monochange/commit/cf5e58113adcda077dfff7c3dd8f5e7598e411d8) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#209](https://github.com/monochange/monochange/issues/209)

### Refactor

#### align crate docs and readability with the workspace style guide

This pass improves readability and documentation consistency across the workspace without changing release behavior or public APIs.

**What changed:**

- extracted shared crate-level docs into `.templates/crates.t.md` and reused them from Rust `lib.rs` module docs and crate readmes
- added missing readmes and module docs for `monochange_analysis`, `monochange_hosting`, and `monochange_test_helpers`
- rewrote a few nested control-flow sections into flatter early-return or `match`-based forms in `monochange`, `monochange_config`, `monochange_gitea`, `monochange_gitlab`, `monochange_npm`, and the shared test helpers
- replaced duplicated fixture-copy helpers in `monochange_cargo` and `monochange_core` tests with the shared `monochange_test_helpers::copy_directory` utility

**Before:**

```rust
if let Some(existing_pr) = &existing {
    if content_matches {
        // skipped response
    } else {
        // update response
    }
} else {
    // create response
}
```

**After:**

```rust
match existing {
    Some(existing_pr) if content_matches => {
        // skipped response
    }
    Some(existing_pr) => {
        // update response
    }
    None => {
        // create response
    }
}
```

The result is more consistent crate documentation, less duplicated prose, and flatter control flow in a few high-traffic code paths.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #224](https://github.com/monochange/monochange/pull/224) _Introduced in:_ [`d0f76ed`](https://github.com/monochange/monochange/commit/d0f76ed56fa18e0ca9d9ec20fa9e44d413014db7) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Changed

#### add `caused_by` changeset context for dependency propagation

You can now annotate dependency-only follow-up changesets with `caused_by`, use `mc change --caused-by ...` to author them, inspect the linkage in diagnostics output, and suppress matching automatic dependency propagation during release planning.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #245](https://github.com/monochange/monochange/pull/245) _Introduced in:_ [`8ec612b`](https://github.com/monochange/monochange/commit/8ec612beb9a8b8100037435695826042bc7361c4) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#208](https://github.com/monochange/monochange/issues/208)

#### add the first Dart lint suite foundation

monochange now wires Dart manifests into the ecosystem-owned lint registry and supports scaffolding Dart lint files with `mc lint new dart/<rule-name>`.

This foundation change adds:

- a new `monochange_dart::lints` module with target collection for `pubspec.yaml`
- Dart lint suite registration in the `mc lint` and `mc check` command paths
- Dart lint scaffolding support in `mc lint new`
- tests covering managed Dart lint target collection and fixture filtering

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #235](https://github.com/monochange/monochange/pull/235) _Introduced in:_ [`5a7a4fe`](https://github.com/monochange/monochange/commit/5a7a4fed84603f51dd5d152d11e739f30dea2b64) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#230](https://github.com/monochange/monochange/issues/230)

#### centralize manifest lint configuration and split lint suites by ecosystem

monochange now configures manifest linting from a top-level `[lints]` section instead of per-ecosystem `[ecosystems.<name>.lints]` tables, and the runtime engine now loads lint suites from ecosystem crates instead of hard-coding Cargo and npm behavior in `monochange_lint`.

**Before (`monochange.toml`):**

```toml
[ecosystems.cargo.lints]
"cargo/internal-dependency-workspace" = "error"

[ecosystems.npm.lints]
"npm/workspace-protocol" = "error"
```

**After:**

```toml
[lints]
use = ["cargo/recommended", "npm/recommended"]

[lints.rules]
"cargo/internal-dependency-workspace" = "error"
"npm/workspace-protocol" = "error"

[[lints.scopes]]
name = "published cargo packages"
match = { ecosystems = ["cargo"], managed = true, publishable = true }
rules = { "cargo/required-package-fields" = "error" }
```

**New CLI surface:**

```bash
# before
mc check --ecosystem cargo --format json

# after
mc check --ecosystem cargo --only cargo/internal-dependency-workspace --format json
mc lint list
mc lint explain cargo/internal-dependency-workspace
mc lint new cargo/no-path-dependencies
```

**Library-facing changes:**

```rust
// before
let configuration = load_workspace_configuration(root)?;
let cargo_rules = configuration.cargo.lints.clone();

// after
let configuration = load_workspace_configuration(root)?;
let lint_settings = configuration.lints.clone();
```

```rust
// new shared contracts in monochange_core::lint
pub trait LintSuite: Send + Sync {
	fn suite_id(&self) -> &'static str;
	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>>;
	fn presets(&self) -> Vec<LintPreset>;
	fn collect_targets(
		&self,
		workspace_root: &Path,
		configuration: &WorkspaceConfiguration,
	) -> MonochangeResult<Vec<LintTarget>>;
}
```

This release also adds two new support crates:

- `monochange_linting` — authoring helpers for lint-rule metadata and suite construction
- `monochange_lint_testing` — snapshot-friendly helpers for lint reports and autofix output

The Cargo and npm suites now live in `monochange_cargo::lints` and `monochange_npm::lints`, so ecosystem-specific parsing and rule behavior stay with their ecosystem adapters.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #228](https://github.com/monochange/monochange/pull/228) _Introduced in:_ [`94f06a0`](https://github.com/monochange/monochange/commit/94f06a057150d26e5f330e2e49a08f71eb12fc92) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)
