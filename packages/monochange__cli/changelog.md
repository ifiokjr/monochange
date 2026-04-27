## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-27)

### Changed

#### Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740)

#### Add OXC tooling for all JavaScript and TypeScript in the project

- Integrate `dprint-plugin-oxc` as the formatter for JS/TS files, replacing the dprint typescript plugin.
- Add `oxfmt` and `oxlint` configuration (`.oxfmtrc.json` and `.oxlintrc.json`) with rules adapted from the sibling `actions` repo.
- Add `tsgo` for type-checking and `tsdown` for bundling, with a root `tsdown.config.json`.
- Wire everything into `devenv.nix` with new scripts: `lint:js`, `lint:js:syntax`, `lint:js:types`, `fix:js`, and `build:js`. Update `lint:all` and `fix:all` to include the JS checks.
- Add JS devDependencies (`oxfmt`, `oxlint`, `@rslint/tsgo`, `tsdown`) to `package.json`.
- Add JS dependency installation (`pnpm install --frozen-lockfile`) to the shared `devenv` GitHub Action so CI has the tools available.
- Add `lint:js:syntax` check to the CI `lint` job.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #286](https://github.com/monochange/monochange/pull/286) _Introduced in:_ [`09298d3`](https://github.com/monochange/monochange/commit/09298d326fa75e4229080159051fa1c3e6363794)

#### Document the new CLI process where `mc init` generates editable workflow commands in `monochange.toml` and every built-in step is available directly through immutable `mc step:*` commands. The docs now clarify reserved command names such as `validate` and recommend `mc step:affected-packages --verify` for direct changeset-policy checks.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #291](https://github.com/monochange/monochange/pull/291) _Introduced in:_ [`74ac16a`](https://github.com/monochange/monochange/commit/74ac16af949ab07644c9b583774a00da2d95a7be)

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
