# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.6.0](https://github.com/monochange/monochange/releases/tag/v0.6.0) (2026-05-20)

### 💥 Breaking Change

#### Async migration: Tokio async runtime end-to-end

This is a **breaking change** that migrates the entire CLI and workspace from synchronous I/O to Tokio async. All public APIs that previously returned `Result<T, E>` directly now return `impl Future<Output = Result<T, E>>` and must be `.await`ed.

The migration was made to reduce release-planning latency by overlapping external work, adding cancellation and timeout boundaries around hosted-source requests, and removing repeated manifest discovery from common policy paths. On a 200-package / 500-changeset / 500-commit fixture, the direct step-command benchmark matrix improved across every measured command with `0` regressions. Across the eight-command matrix, wall-clock time dropped by about **45% on average** (geometric mean about **3.0× faster**, arithmetic mean about **8.3× faster** because the fastest policy paths improved dramatically).

Notable wins:

- `mc step:affected-packages --dry-run --format json` improved from `1442.3 ms` to `35.8 ms` — about **40.3× faster** — by using configuration-only package/group indexes for changeset-policy checks instead of paying full manifest discovery cost.
- The explicit no-changeset affected-package path now completes in about `7.7 ms`, roughly **159× faster** than the pre-optimization async implementation.
- `mc step:diagnose-changesets --dry-run --format json` improved from `3072.2 ms` to `184.9 ms` — about **16.6× faster** — by using the same fast config-id path before falling back to discovery.
- `mc step:prepare-release --dry-run --format json` improved from `2374.8 ms` to `858.5 ms` — about **2.8× faster** — while retaining deterministic release output.
- Short command startup stayed fast by using a current-thread Tokio runtime for `mc`, `monochange`, and `xtask`; previously noisy commands such as `step:config` and `step:display-versions` now benchmark faster than `main`.

##### Breaking changes — Public API signatures

###### `monochange_core`

- **`git_command`** now returns `std::process::Command` (unchanged for inspection compatibility), but all execution functions (`git_checkout`, `git_clone`, `git_commit`, `git_push`, `git_fetch`, `git_merge`, `git_default_branch_name`, `git_rebase`, `git_create_branch`, `git_delete_branch`, `git_tag_create`, `git_read_tree`, `git_status`, etc.) are now `async fn` returning `impl Future`. Callers must `.await` these.
- **`DiscoverOptions`**, **`discover_workspace`**, and all git helper functions are now async.

###### `monochange_hosting`

- **All provider trait methods** (`verify_release_branch`, `publish_release`, `retarget_release_tags`, `create_release_pull_request`, `update_release_pull_request`, `find_existing_pull_request`, `find_existing_merge_request`, `find_existing_release`, `enrich_changeset_context`, `default_branch_name`, `no_identity`) are now `async fn`. Implementors must update their trait implementations.
- Provider lookup functions (`get_hosting_provider`, `get_provider`) remain sync.

###### `monochange_github`, `monochange_gitea`, `monochange_gitlab`, `monochange_forgejo`

- **All public sync entry points** that previously used `Runtime::new().block_on()` internally are now `async fn`; the sync bridge helper is kept only for tests. Public `async fn` signatures include:
  - `publish_release`, `find_existing_pull_request`, `find_existing_release`, `default_branch_name`, `create_change_request`, `verify_release_branch`, `retarget_release_tags`, `enrich_changeset_context`, `no_identity`
- **`reqwest::blocking::Client`** replaced with async `reqwest::Client` throughout.
- **`github_runtime()` / `gitea_runtime()` / etc.** removed from public API (only available as `#[cfg(test)]` helpers).

###### `monochange_publish`

- **`filter_pending_publish_requests`**, **`filter_pending_publish_requests_with_transport`**, **`registry_version_exists`**, **`crates_io_version_exists`**, **`crates_io_index_version_exists`** are now `async fn`. Callers must `.await`.
- **`execute_publish_requests`**, **`execute_publish_requests_with_progress`**, **`execute_publish_requests_with_process`**, **`execute_publish_requests_with_process_and_progress`**, and **`run_placeholder_publish`** are now async.
- **`reqwest::blocking::Client`** replaced with async `reqwest::Client` throughout.
- **`registry_client()`** is now a sync function returning `MonochangeResult<Client>` (no longer async).

###### `monochange`

- **`cli_runtime::block_on_in_context`** is now a `#[cfg(test)]` `pub(crate)` helper for compatibility tests; production code awaits async APIs directly.
- **`publish_source_change_request`**, **`publish_readiness::publish_plan_package_filter_from_readiness_artifact`**, **`plan_publish_rate_limits`**, and all async CLI step handlers are now `async fn`.
- **`run_publish_packages`**, **`run_publish_packages_with_resume`** remain async.
- **`run_placeholder_publish`** and **`execute_publish_requests_with_process`** are now async and should be awaited directly.
- **Test files** converted from `#[test]` to `#[tokio::test(flavor = "multi_thread")]` where they call async code.

##### Migration guide

1. Any code calling `monochange_core::git::*` functions must `.await` the result.
2. Any code using `monochange_hosting` provider traits must implement `async fn` methods.
3. Any code calling `monochange_publish::*` async functions must be in an async context.
4. Tests that call async code must use `#[tokio::test(flavor = "multi_thread")]`.
5. Replace `reqwest::blocking::Client` with `reqwest::Client` (async) in all custom code.
6. Avoid new sync-to-async bridges in production code; prefer async callers and `.await`. `block_on_in_context` is retained only for test compatibility boundaries.

```rust
// Before (sync):
let result = monochange_core::git::git_checkout(&repo_dir, branch)?;

// After (async, must await):
let result = monochange_core::git::git_checkout(&repo_dir, branch).await?;

// Before (sync):
let pending = monochange_publish::filter_pending_publish_requests(&config)?;

// After (async, must await):
let pending = monochange_publish::filter_pending_publish_requests(&config).await?;
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #440](https://github.com/monochange/monochange/pull/440) _Introduced in:_ [`10ef5bd`](https://github.com/monochange/monochange/commit/10ef5bda1e30003018408c9a6c1758af69e781aa) _Closed issues:_ [#407](https://github.com/monochange/monochange/issues/407)

### 🚀 Feature

#### Add configurable changelog rendering styles

Add configurable changelog and release-note rendering style options for section separators, package labels, metadata lines, and collapsed sections.

```toml
[changelog.style]
sectionSeparator = "blank_line"
packageLabelStyle = "inline"
packageLabelPlacement = "after_heading"
metadataStyle = "plain"
collapsedSectionStyle = "details"

[changelog.release_notes]
metadataStyle = "blockquote"
```

The config schema now includes `ChangelogStyle` and `ReleaseNotesStyleOverrides`, with release notes inheriting `[changelog.style]` unless a field-specific override is set.

Default section headings now include emoji in the `heading` string, while the stable section keys remain unchanged:

- `breaking`: `💥 Breaking Change`
- `feat`: `🚀 Feature`
- `change`: `📝 Changed`
- `fix`: `🐛 Fixed`
- `test`: `🧪 Testing`
- `refactor`: `🔨 Refactor`
- `docs`: `📖 Documentation`
- `security`: `🔒 Security`
- `perf`: `⚡ Performance`
- `none`: `🔖 None`

Semver level type aliases route to semantic sections: `major` to `breaking`, `minor` to `feat`, and `patch` to `fix`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #511](https://github.com/monochange/monochange/pull/511) _Introduced in:_ [`b03612b`](https://github.com/monochange/monochange/commit/b03612b5d69f05becd68a803efa535e0f874ee01)

### 🐛 Fixed

#### Add optional full release staging

Release commit and release request steps now support a `stage_all` input/config field that defaults to `false`. When enabled, the release commit stages every non-ignored working tree change, so generated lockfile updates like `pnpm-lock.yaml` can be included alongside configured release manifests and changelogs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #520](https://github.com/monochange/monochange/pull/520) _Introduced in:_ [`035dcb3`](https://github.com/monochange/monochange/commit/035dcb345cca8586440451836fa06fb631596c20)

## [0.5.1](https://github.com/monochange/monochange/releases/tag/v0.5.1) (2026-05-15)

### 🐛 Fixed

#### Fix publish failures and cargo package warnings

Remove `build.rs` from `monochange_schema` and replace the generated `CURRENT_SCHEMA_VERSION_TEXT` with a direct `include_str!` + `trim_ascii_end()` compile-time embed, eliminating the `OUT_DIR` dependency that caused `cargo publish` verification to fail.

Add `src/**/*.template` to `monochange`'s `package.include` list so that `monochange.toml.template` (referenced via `include_str!`) is included in the published crate tarball.

Add `tests/**/*.rs` to `monochange_core` and `monochange_github` package include lists to suppress cargo package "ignoring test" warnings.

Fix `monochange_go` readme from workspace-inherited path (`../../readme.md`) to local path (`readme.md`) to suppress the "readme outside package" cargo package warning.

Remove `doc-comment` dependency and replace `doc_comment::doctest!` with `#[doc = include_str!(...)]` in the book crate. Only keep local file references that resolve within the package directory.

Add rustdoc backticks around `PyPI` and `pub.dev` in configuration guide markdown to satisfy the missing-backticks lint.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #509](https://github.com/monochange/monochange/pull/509) _Introduced in:_ [`e6af821`](https://github.com/monochange/monochange/commit/e6af8212983b66c1b8370443e7978c6067b89fa6)

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-14)

### 💥 Breaking Change

#### require CLI steps to opt in to inherited command inputs

> **Breaking change** — CLI step inputs are now explicit. Command-level inputs no longer automatically appear in every configured CLI step.

A configured step now receives only the inputs listed in that step's `inputs` field. This removes ambiguous behavior where a command-level flag could unexpectedly shadow a step-specific input with the same name.

**Before:** every step implicitly saw all command inputs, even with no step-level `inputs` entry:

```toml
[cli.release]
inputs = [{ name = "format", type = "choice", choices = ["text", "json"], default = "text" }]
steps = [{ type = "PrepareRelease" }]
```

**After:** inherit command inputs explicitly with the array shorthand:

```toml
[cli.release]
inputs = [{ name = "format", type = "choice", choices = ["text", "json"], default = "text" }]
steps = [{ type = "PrepareRelease", inputs = ["format"] }]
```

Map overrides still work for fixed or templated step values:

```toml
steps = [
	{ type = "PrepareRelease", inputs = ["format"] },
	{ type = "PublishRelease", inputs = { format = "json", draft = "{{ inputs.draft }}" } },
]
```

Migration path: review custom `[cli.<command>]` definitions and add `inputs = ["name"]` to every step that needs a command-level input. Built-in default CLI commands and generated templates have been updated to declare their inherited inputs explicitly.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #467](https://github.com/monochange/monochange/pull/467) _Introduced in:_ [`ce4712f`](https://github.com/monochange/monochange/commit/ce4712f2890e0636c368b056db756df32f4cf769) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### generate built-in release and validation step commands

> **Breaking change** — several hardcoded top-level commands now live under generated immutable `mc step:*` command names.

The release-record, publish-readiness, tag-release, placeholder-publish, and validation operations now share the generated step-command path used by the rest of the CLI step catalog. This keeps their help, schema metadata, docs, and automation examples consistent with configured workflow steps while preserving the distinction between binary commands, generated step commands, and optional user-defined `[cli.*]` workflow aliases.

**Before:** scripts could call these hardcoded top-level commands directly:

```bash
mc validate
mc release-record --from HEAD --format json
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc tag-release --from HEAD
mc publish-bootstrap --from HEAD --output .monochange/bootstrap-result.json
```

**After:** call the generated step command names instead:

```bash
mc step:validate
mc step:release-record --from HEAD --format json
mc step:publish-readiness --from HEAD --output .monochange/readiness.json
mc step:tag-release --from HEAD
mc step:placeholder-publish --from HEAD --output .monochange/bootstrap-result.json
```

`mc init` also writes a smaller starter configuration. It no longer seeds redundant generated `[cli.*]` aliases for commands that already exist as immutable step commands.

**Before:** starter configs included workflow aliases for generated behavior:

```toml
[cli.validate]
steps = [{ type = "Validate" }]
```

**After:** starter configs rely on the generated command directly and reserve `[cli.*]` for repository-specific chains, custom inputs, or shell `Command` steps:

```bash
mc step:validate
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #479](https://github.com/monochange/monochange/pull/479) _Introduced in:_ [`d9adff8`](https://github.com/monochange/monochange/commit/d9adff8fb396df908e335d2a6688aa729abb5f4d) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8) _Closed issues:_ [#476](https://github.com/monochange/monochange/issues/476)

### 🚀 Feature

#### Configurable publish-order dependency fields

Add configurable ecosystem-specific dependency fields for package publish ordering across npm, Cargo, Deno, Dart/Flutter, Python, and Go.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #472](https://github.com/monochange/monochange/pull/472) _Introduced in:_ [`0d9cf46`](https://github.com/monochange/monochange/commit/0d9cf461a05057b61efa987d361ebd27d800dbdb) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8) _Closed issues:_ [#465](https://github.com/monochange/monochange/issues/465)

#### Publish all configured packages

Add a `--all` flag to the PublishPackages CLI step so migration workflows can publish every configured package, including packages that were not part of the prepared release record.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #461](https://github.com/monochange/monochange/pull/461) _Introduced in:_ [`3d956cd`](https://github.com/monochange/monochange/commit/3d956cd3e34747e088add98fe0358251f388782f) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

### 🐛 Fixed

#### add release-record migration command

Add `mc migrate release-records` to rewrite persisted release records to the latest schema version, expose the release-record migration helper from core, and update the generated skill command inventory.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #500](https://github.com/monochange/monochange/pull/500) _Introduced in:_ [`bd56420`](https://github.com/monochange/monochange/commit/bd564204b786961371b0ac1bad21071ebe5fe90c) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### keep release-record discovery compatible across schema upgrades

Merged release commits that embed an older public release-record `schemaVersion` can now be read by newer monochange binaries. This lets commands such as:

```bash
mc step:release-record --from HEAD --format json
```

recognize an existing release commit after monochange itself has moved to a newer schema version, instead of reporting the older record as unsupported.

The GitHub Actions CI workflow now also includes a pull-request `release-records` preflight. For normal PRs, it creates the same local release commit used by the release test/lint preflights and verifies that `mc step:release-record` can read it and confirm that the generated commit is the resolved release-record commit.

Generated current and versioned release-record artifact fixtures are also checked into the schema crate. Schema checks ensure the current fixtures are regenerated during release planning, and integration tests load every checked-in release-record artifact through the real parser/migration path.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #491](https://github.com/monochange/monochange/pull/491) _Introduced in:_ [`914a6e8`](https://github.com/monochange/monochange/commit/914a6e88d4bcf31249d467914f0a3a3b240d931a) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

## [0.4.2](https://github.com/monochange/monochange/releases/tag/v0.4.2) (2026-05-10)

### 🚀 Feature

#### Order publish plans by dependencies

Order publish plans by workspace dependencies before applying registry rate-limit windows, and run CI publishing as one dependency-ordered publish operation.

This keeps dependent packages from publishing before their internal dependencies are available and adds realistic fixture coverage for non-alphabetical cargo dependency graphs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e) _Last updated in:_ [`2392845`](https://github.com/monochange/monochange/commit/2392845ec29289e3f219aca20ac343cf79ee965e)

## [0.4.1](https://github.com/monochange/monochange/releases/tag/v0.4.1) (2026-05-10)

### 🚀 Feature

#### Add `always_run` primitive to CLI steps and group/ecosystem filters to `PublishPackages`

##### `always_run` primitive

A new `always_run` boolean field is available on every CLI step definition. When `always_run: true`, the step continues to execute even when a previous step in the same command has failed.

This enables composable dry-run workflows such as:

```toml
[[cli.publish-dry-run]]
name = "publish-dry-run"
help_text = "Preview publishing without side effects"
steps = [
	{ type = "PrepareRelease", name = "prepare", inputs = { allow_empty_changesets = "true" } },
	{ type = "PublishPackages", name = "publish", always_run = true, inputs = { resume = ".monochange/local/previous-result.json" } },
]
```

Running `mc publish-dry-run --dry-run` will always execute the `PublishPackages` step regardless of whether `PrepareRelease` succeeds, because `PublishPackages` is marked `always_run = true`.

###### Behavior

- When a step fails and later steps have `always_run: true`, those steps still execute.
- Non-`always_run` steps after a failure are skipped.
- The overall command still returns the first error after all `always_run` steps finish.

##### `PublishPackages` filters

`PublishPackages` now accepts two new step inputs:

- `--group <group-id>` — resolves a group from the workspace configuration and publishes all packages in that group.
- `--ecosystem <ecosystem>` — filters publication targets to a specific ecosystem (`cargo`, `npm`, `deno`, `dart`, `flutter`, `python`, or `go`).

Both inputs can be repeated:

```bash
mc publish --group sdk --group apps --ecosystem npm --ecosystem cargo
```

Groups are resolved to their member packages before ecosystem filtering is applied.

##### Dry-run guards

`PublishPackages` now skips the following side-effecting operations when `--dry-run` is active:

- `release_branch_policy::verify_release_ref_for_publish`
- `publish_rate_limits::enforce_publish_rate_limits`
- writing the publish report artifact to disk

##### Per-command `dry_run` field

CLI command definitions now support a `dry_run` boolean field. When `dry_run = true`, the command always executes in dry-run mode regardless of whether `--dry-run` is passed on the CLI. This enables built-in preview commands such as:

```toml
[cli.publish-check]
help_text = "Validate the release and preview package publishing in dry-run mode"
dry_run = true
steps = [
	{ type = "PrepareRelease", name = "plan release" },
	{ type = "PublishPackages", name = "publish packages dry run" },
]
```

Running `mc publish-check` (without `--dry-run`) will still run in dry-run mode because the command definition sets `dry_run = true`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #426](https://github.com/monochange/monochange/pull/426) _Introduced in:_ [`6ea6236`](https://github.com/monochange/monochange/commit/6ea623624e36d795edd531ae72080a2e9c3fb86a)

#### Migrate JSON Schema generation from hand-tuned templates to schemars

Schema assets (`monochange.schema.json` and `release-record.schema.json`) are now generated from the Rust type tree via the `schemars` crate, eliminating manual drift between source types and committed schemas.

###### Added

- `schema` feature on `monochange_core` and `monochange_config` gating `schemars`.
- `JsonSchema` derives on `ReleaseRecord`, `RawWorkspaceConfiguration`, and their transitive types.
- `monochange_core::schema` and `monochange_config::schema` modules providing `release_record()` and `workspace_configuration()` schema generation functions.
- `xtask` binary crate providing `schema update` and `schema check` subcommands, with a `cargo xtask` alias.

###### Changed

- `devenv.nix` `schema:update` / `schema:check` now invoke `cargo xtask schema update` and `cargo xtask schema check`.
- `$defs` keys use camelCase names (e.g. `packageDefinition`) via `#[schemars(rename)]` attributes.
- Release-record `schemaVersion` and `kind` emit `const` constraints instead of `default`.

###### Removed

- `scripts/schema-assets.sh` shell script.
- `schemas/templates/*.schema.template.json` template files.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #438](https://github.com/monochange/monochange/pull/438) _Introduced in:_ [`d0676f0`](https://github.com/monochange/monochange/commit/d0676f067299fb4db38cc748dcbb619ab7532a49)

### 🐛 Fixed

#### allow boolean and numeric literals in `CliInputDefinition.default`

The JSON schema for `monochange.toml` `[cli.*.inputs]` previously rejected boolean and numeric defaults, even though the Rust deserializer already accepted them correctly.

**Before:**

```toml
[[cli.release-pr.inputs]]
name = "no_verify"
type = "boolean"
default = true # jsonschema error: "true is not of types \"null\", \"string\""
```

**After:**

The `default` field in `CliInputDefinition` now accepts `string | boolean | integer | number | null` in the generated schema. TOML like the snippet above validates cleanly, and numeric defaults such as `default = 42` are also accepted.

The internal `CliInputDefault` enum gained `Integer(i64)` and `Number(f64)` variants, and the `schemars` derive now generates a multi-type `anyOf` schema for the `default` property.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #445](https://github.com/monochange/monochange/pull/445) _Introduced in:_ [`1d42ece`](https://github.com/monochange/monochange/commit/1d42ece77ceda58cd44ce67749c5faa5d4ec8314)

#### Fix release record reformatting after dprint

Prevent `commit_release` from rewriting `release.json` after `dprint fmt` has already formatted it. The validation now compares parsed JSON values instead of raw strings, so formatting-only differences (such as tab vs space indentation) no longer trigger a rewrite.

The `CommitRelease` step now accepts an `update_release_json` input (default `false`). When `true`, the step will create or overwrite the `release.json` file if it is missing or mismatched. When `false`, a mismatch produces a clear error asking the user to set `update_release_json = true`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #439](https://github.com/monochange/monochange/pull/439) _Introduced in:_ [`f4b324e`](https://github.com/monochange/monochange/commit/f4b324ec920ea787ec6d5c511b81d9c22fbad753)

#### Release PR formatting, schema version, and publish batch ordering

1. Format generated `.monochange/releases/` manifests via `dprint fmt` in `[cli.release-pr]`.
2. Derive expected schema versions in snapshots and tests from the actual `Cargo.toml` version instead of hardcoding `0.0`.
3. Topologically sort publish requests by both runtime and development dependencies before batching so dependencies are published before dependents.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #436](https://github.com/monochange/monochange/pull/436) _Introduced in:_ [`ea78ccc`](https://github.com/monochange/monochange/commit/ea78ccc844318b0645010f15fcf60b9d8ea6a58c) _Related issues:_ [#434](https://github.com/monochange/monochange/issues/434)

#### Replace release record `groupVersion` with `versions`

Release records now include a `versions` map keyed by released package or group id, and no longer write the redundant `groupVersion` field.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #450](https://github.com/monochange/monochange/pull/450) _Introduced in:_ [`375bc19`](https://github.com/monochange/monochange/commit/375bc19dc69c125ffbd944d016b16ebc1c8cb7c5)

## [0.4.0](https://github.com/monochange/monochange/releases/tag/v0.4.0) (2026-05-09)

### 💥 Breaking Change

#### Publish durable release schema contracts

Impact: release records now use the first public durable schema header, `v = "0.1"`, and monochange rejects missing, invalid, old, or future durable schema versions instead of reading unsafe historical shapes. The new `monochange_schema` crate owns schema version parsing, release-record wire validation, committed schema assets, and durable migration helpers.

Usage: editors can use the hosted configuration schema once GitHub Pages publishes the docs, or the raw GitHub fallback immediately. Durable release records now embed the public version field instead of the internal Rust-only `schemaVersion` field:

```json
{
	"v": "0.1",
	"kind": "monochange.releaseRecord"
}
```

The `monochange_schema` package remains independently versioned from the main release group. Its crate version starts at `0.0.0` on this branch, while this major changeset gives release planning the explicit signal to publish the first crate release without changing the durable public schema version `0.1`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #396](https://github.com/monochange/monochange/pull/396) _Introduced in:_ [`563ef83`](https://github.com/monochange/monochange/commit/563ef83fa21260518ae60c972240e2f0562e9bc2) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

### 🚀 Feature

#### Consolidate adapter traits to remove ecosystem match arms

Replace hardcoded ecosystem and registry match arms in `workspace_ops`, `monochange_config`, and `monochange_publish` with adapter registry dispatch.

- Expand `EcosystemAdapter` in `monochange_core` with `load_configured`, `supported_versioned_file_kind`, and `validate_versioned_file`.
- Add `From<EcosystemType>` and `From<PackageType>` conversions for `Ecosystem`.
- Add `FromStr` for `Ecosystem` and extract `default_registry_kind_for_ecosystem` into `monochange_core`.
- Implement the new trait methods in all ecosystem adapter crates.
- Replace `discover_packages` body with `build_ecosystem_registry().discover_all(root)?`.
- Replace `discover_release_workspace` `load_configured` match arms with registry dispatch.
- Replace `path_is_supported_for_ecosystem` and `validate_ecosystem_version_readable` match arms in `monochange_config` with registry dispatch.
- Introduce `PublishAdapter` trait and `PublishCommandBuilder` in `monochange_publish` to replace `build_publish_command` registry match arms.
- Extract `default_registry_kind_for_ecosystem` mapping out of `package_publish.rs` into `monochange_core`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #429](https://github.com/monochange/monochange/pull/429) _Introduced in:_ [`271e554`](https://github.com/monochange/monochange/commit/271e55420154265e798a0de3adf26a64faba66c8)

#### Move ecosystem constants out of core and delegate validation to ecosystem crates

Each ecosystem crate now owns its own `default_dependency_version_prefix()`, `default_dependency_fields()`, and `validate_versioned_file()` functions. The `EcosystemType::default_prefix()` and `EcosystemType::default_fields()` methods on `monochange_core::EcosystemType` are deprecated in favor of the ecosystem crate equivalents. `monochange_config` versioned file validation now dispatches to ecosystem crate validators instead of embedding ecosystem-specific parsing logic in config.

Closes #137 Closes #138

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #411](https://github.com/monochange/monochange/pull/411) _Introduced in:_ [`57e2322`](https://github.com/monochange/monochange/commit/57e232282d41e89e70f3e5b34b3e07d8b2089fea) _Related issues:_ [#137](https://github.com/monochange/monochange/issues/137), [#138](https://github.com/monochange/monochange/issues/138)

#### File-based release records

Store release records as committed JSON files under `.monochange/releases/` instead of embedding them in commit message bodies.

##### Before

Release records were embedded inside the commit message body between HTML comment markers:

````markdown
chore(release): prepare release

## monochange Release Record

<!-- monochange:release-record:start -->

```json
{
	"schemaVersion": 1,
	"kind": "monochange.releaseRecord",
	"createdAt": "2026-05-08T08:00:00Z",
	"command": "release-pr",
	"releaseTargets": [
		{
			"id": "sdk",
			"kind": "group",
			"version": "1.2.3",
			"tag": true,
			"release": true
		}
	]
}
```
````

<!-- monochange:release-record:end -->

```
Discovery required parsing every commit message in first-parent ancestry with regex-based extraction.

## After

Release records are plain JSON files committed to the repository:
```

.monochange/ ├── local/ # gitignored — local artifacts │ ├── release-manifest.json │ └── prepared-release-cache.json └── releases/ └── <hash>/ # content-addressable directory └── release.json # the release record

```
The `<hash>` is derived from sorted `(package_id, version)` pairs via `DefaultHasher`. For a release targeting `sdk` at version `1.2.3` the hash might look like:
```

.monochange/releases/8f3e2a1b/c/release.json

````
(The exact hex value depends on the hasher state; it is always 16 hex characters.)

## Deduplication

When writing a new release record, any existing record that shares an overlapping `(package_id, version)` tag is automatically removed. This prevents stale records from accumulating when a release is retried or amended.

## Discovery

`mc release-record` now discovers files via `git diff-tree --no-commit-id --name-only -r` (falling back to `git ls-tree` for root commits) rather than parsing commit messages.

## CI detection

```bash
git diff-tree --no-commit-id --name-only -r HEAD |
  grep '^\.monochange/releases/.*/release\.json$'
````

## Breaking changes

- `.monochange/*` is no longer fully gitignored; only `.monochange/local/` is ignored.
- The `ReleaseRecord` JSON schema itself remains identical; only the storage location changes.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #408](https://github.com/monochange/monochange/pull/408) _Introduced in:_ [`cd989c3`](https://github.com/monochange/monochange/commit/cd989c303a9722ca8240c44003f1ef4c96abc284)

#### Add Forgejo source provider

Add Forgejo as a hosted source provider for releases and release pull requests.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #401](https://github.com/monochange/monochange/pull/401) _Introduced in:_ [`86026ac`](https://github.com/monochange/monochange/commit/86026acb83e338fe8d07c200fb8e38693616b6e8)

#### Move release record generation from commit_release to prepare_release

The release record JSON is now written during the `PrepareRelease` CLI step instead of the `CommitRelease` step. This gives users a formatting preview and the opportunity to review or edit the record before it is committed.

##### What changed

- `ReleaseManifest` and `PreparedRelease` no longer store a `release_record_path` field. The path is derived on demand via the new `ReleasePaths` helper, which computes the hash, relative path, and absolute path from the manifest's `release_targets`.
- A new `ReleasePaths` runtime helper provides `hash`, `relative`, and `absolute` paths for any release record. Steps that need the path can call `ReleasePaths::from_manifest` or `ReleasePaths::from_record` instead of reading a cached field.
- The `PrepareRelease` step calls `write_release_record_file` to write the record to `.monochange/releases/<hash>/release.json`. The file is left unstaged for user review.
- `commit_release` now validates the pre-written record with `validate_release_record_file` instead of generating it.
- `deduplicate_overlapping_release_records` is now cached per-process to avoid redundant filesystem scans when both `write_release_record_file` and `validate_release_record_file` run in the same CLI invocation.
- `git_stage_paths_command` adds `-f` so that ignored `.monochange/releases/` files can still be staged.
- `release_path_requires_staging` explicitly allows `.monochange/releases/` paths even when gitignored.
- `write_release_record_file` skips overwriting an existing record file so that subsequent `PrepareRelease` steps (for example during `mc release-pr`) do not dirty the working tree.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #421](https://github.com/monochange/monochange/pull/421) _Introduced in:_ [`46fb400`](https://github.com/monochange/monochange/commit/46fb40022d6275faa9f8e231764643665248b773) _Closed issues:_ [#418](https://github.com/monochange/monochange/issues/418)

### 🐛 Fixed

#### Add schema version redaction to snapshot settings and release record tests

Stop hardcoding `monochange_schema` public schema version (`0.0`) in snapshot assertions and unit tests. Use insta redaction for the release record `"v"` wire-format field in multiline snapshots, and read the expected schema version from `monochange_schema::CURRENT_SCHEMA_VERSION_TEXT` at runtime in `monochange_core` unit tests.

This prevents failures after every release when the `monochange_schema` version bumps and `CURRENT_SCHEMA_VERSION_TEXT` changes.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #410](https://github.com/monochange/monochange/pull/410) _Introduced in:_ [`f9396c3`](https://github.com/monochange/monochange/commit/f9396c3029d7a49502908fcec908bca00f3853b4)

#### Remove grouped release member summaries

Grouped release notes no longer include generated changed or synchronized member lists, keeping the release note summary focused on the group release itself.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #395](https://github.com/monochange/monochange/pull/395) _Introduced in:_ [`2d012ff`](https://github.com/monochange/monochange/commit/2d012ff900a612f4aed6e4d7034c8c876f50aeae) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

### 🧪 Testing

#### Extract inline test modules into separate files

Move all inline `#[cfg(test)] mod tests { ... }` blocks out of source files into dedicated test files. This reduces source file sizes and keeps test code in a consistent `__tests/` directory structure next to the module it tests.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #416](https://github.com/monochange/monochange/pull/416) _Introduced in:_ [`3535c88`](https://github.com/monochange/monochange/commit/3535c887c46d66db2768377cb5f01406f6e9a8b6)

#### Normalize Rust unit test file layout

Move Rust unit tests into colocated `__tests__/` directories and name each file after the module under test with a `_tests.rs` suffix.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #428](https://github.com/monochange/monochange/pull/428) _Introduced in:_ [`b61cc3e`](https://github.com/monochange/monochange/commit/b61cc3e66989fd83ffb16a31568d2f46d7075216)

## [0.3.4](https://github.com/monochange/monochange/releases/tag/v0.3.4) (2026-05-06)

### 🐛 Fixed

#### Preserve publish batch dependency order

Carry prior packages into later publish-plan batches so dependency-ordered publish requests remain available when registry rate limits split a release into multiple jobs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #389](https://github.com/monochange/monochange/pull/389) _Introduced in:_ [`12d3582`](https://github.com/monochange/monochange/commit/12d35826c3b0a8768bbf05c82b1e999a0e9ca30a)

#### Use npm for trusted npm publishing

Route trusted npm publishes through the npm CLI even in pnpm-managed workspaces so npm's OIDC trusted publishing flow can exchange the GitHub Actions identity for a short-lived publish credential. The release workflow also relies on devenv environment cleaning directly instead of the removed `strip:env` wrapper.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #388](https://github.com/monochange/monochange/pull/388) _Introduced in:_ [`72773bc`](https://github.com/monochange/monochange/commit/72773bc438167b55c26bb7c3f5dd9d7a21c99084)

## [0.3.3](https://github.com/monochange/monochange/releases/tag/v0.3.3) (2026-05-06)

### 🐛 Fixed

#### preserve GitHub OIDC environment variables in devenv

The development environment's `devenv.yaml` now keeps the GitHub Actions and OIDC identity variables that monochange needs to detect trusted publishing when running inside `devenv shell`. Previously, `strip: env` removed these variables and caused built-in publishing to fail with "No supported CI provider identity was detected."

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #386](https://github.com/monochange/monochange/pull/386) _Introduced in:_ [`fd1a798`](https://github.com/monochange/monochange/commit/fd1a798e57234fc465c33537077ec6acf0a47db8)

## [0.3.2](https://github.com/monochange/monochange/releases/tag/v0.3.2) (2026-05-06)

### 📝 Changed

- No package-specific changes were recorded; `monochange_core` was updated to 0.3.2 as part of group `main`.

## [0.3.1](https://github.com/monochange/monochange/releases/tag/v0.3.1) (2026-05-05)

### 🚀 Feature

#### Allow `PrepareRelease` to succeed with no changesets

`PrepareRelease` steps in CLI commands now support an `allow_empty_changesets` option. When enabled, missing or empty `.changeset` directories no longer cause errors — instead the step succeeds with an empty release plan, and downstream steps can gate on `number_of_changesets` in their `when` conditions.

This enables `[cli.release-pr]` workflows to run against repos with no pending changes without failing the CI job.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #360](https://github.com/monochange/monochange/pull/360) _Introduced in:_ [`51df5ba`](https://github.com/monochange/monochange/commit/51df5ba79f06a533d0067fdfab3bdeef9df48696) _Closed issues:_ [#344](https://github.com/monochange/monochange/issues/344) _Related issues:_ [#354](https://github.com/monochange/monochange/issues/354)

### 🐛 Fixed

#### Preserve rendered changelog metadata in release records

Release records now store full changelog metadata so publish flows reconstructed from git history can use the rendered release notes instead of falling back to minimal release bodies.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #356](https://github.com/monochange/monochange/pull/356) _Introduced in:_ [`6f38c00`](https://github.com/monochange/monochange/commit/6f38c003a77fcc4a95e33ae1c344340bbcce1017)

#### Preserve configured changelog sections for scalar change types

Configured changelog types now take precedence over scalar bump names so generated release notes retain their intended sections. Local telemetry JSONL writes now append complete event lines to avoid malformed records during concurrent command runs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #363](https://github.com/monochange/monochange/pull/363) _Introduced in:_ [`8c8c9dc`](https://github.com/monochange/monochange/commit/8c8c9dc98f6a95d2c8a2d55fb986a66c08f29312)

#### Filter placeholder publish reports to packages that need action

`mc placeholder-publish` now hides already-published and skipped packages from the default report so dry runs focus on packages that still need placeholder publishing, and real runs focus on packages that were published or failed.

Pass `--show-all` to include the full package report when auditing every selected package.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #372](https://github.com/monochange/monochange/pull/372) _Introduced in:_ [`26f20e6`](https://github.com/monochange/monochange/commit/26f20e6347429e57bc94aea06a40eec81f85c54d)

#### Publish packages in dependency order without readiness artifacts

Package publishing now derives release work directly from prepared release or `HEAD` release state, orders internal publish-relevant dependencies before dependents, and rejects publish-relevant dependency cycles while allowing development-only cycles.

The publish order now works like this:

1. Build the selected publish requests from the prepared release or `HEAD` release state.
2. Materialize the workspace dependency graph.
3. Consider only dependencies where **both packages are part of the selected publish set**.
4. Ignore development dependency edges.
5. Topologically sort the publish requests so dependencies are emitted before dependents.

So for a tree like:

```text
core        # no dependencies
utils       # depends on core
api         # depends on utils
app         # depends on core, utils, api
```

the publish order becomes:

```text
core
utils
api
app
```

If multiple packages are independent at the same depth, their order is deterministic by package id, registry, and version.

A package with no selected dependencies is eligible first. A package is not published until all of its selected publish-relevant dependencies have been ordered before it. Dependencies outside the selected publish set do not block ordering. Development-only cycles are ignored. Runtime, build, peer, workspace, and unknown dependency cycles fail before publishing anything, with a cycle diagnostic.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e)

#### Make release workspace publishing preserve Cargo verification

`monochange_test_helpers` is now publishable so crates that use the shared helpers in their dev-dependencies can still pass Cargo's normal publish verification. `monochange_core` no longer dev-depends on the helper crate: its integration-style discovery filter coverage now lives in the unpublished `monochange_integration_tests` crate, preventing a dependency cycle between the published core crate and the test helper crate.

Package publishing keeps Cargo verification enabled and still runs JavaScript registry tooling without inherited `LD_LIBRARY_PATH`, preserving PNPM support while avoiding Nix/devenv library-path leakage into system Node.js launchers.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #368](https://github.com/monochange/monochange/pull/368) _Introduced in:_ [`b79eef1`](https://github.com/monochange/monochange/commit/b79eef170a01234b69b2b83c8ebd4ef946a079ac)

#### Use `GITHUB_TOKEN` for Git Data API to create verified commits

The `release-pr` workflow now passes `GITHUB_COMMIT_TOKEN` (set to `secrets.GITHUB_TOKEN`) specifically for Git Database API operations (blob, tree, commit creation, and ref updates). This allows GitHub to automatically sign commits with the `web-flow` GPG key, producing verified commits on release pull requests.

The `GH_TOKEN` (PAT) continues to be used for all other GitHub API operations like pull request creation and updates.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #371](https://github.com/monochange/monochange/pull/371) _Introduced in:_ [`3770b48`](https://github.com/monochange/monochange/commit/3770b48bab6b41c80086a0d3e2e4e6a9a7540c39)

### 📦 Other

#### Resolve git identity from token for release PR commits

The `release-pr` workflow now queries the GitHub API for the authenticated user's `id`, `login`, and `name`, then constructs the standard GitHub noreply email (`{id}+{login}@users.noreply.github.com`) for `git config user.email`. This replaces the previous hardcoded `github-actions[bot]` identity, so release PR commits are properly attributed to the account that owns the `RELEASE_PR_MERGE_TOKEN`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #367](https://github.com/monochange/monochange/pull/367) _Introduced in:_ [`920bf04`](https://github.com/monochange/monochange/commit/920bf04ba34aa7050e0dc6a9be5c488c9431d085)

#### Use the current monochange CLI when publishing release tags

The publish workflow now builds the `mc` binary from the workflow commit before checking out the release tag. Publish jobs still operate on the requested release tag's files and release state, but they execute the current workflow version of `mc` so post-release publishing fixes apply when rerunning publication for an older tag.

The workflow keeps full branch and tag history available after switching to the release tag so publish-time release branch reachability checks still work. The release workflow also dispatches `publish.yml` at the current workflow commit, allowing a fixed publish workflow to publish an older release tag.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #366](https://github.com/monochange/monochange/pull/366) _Introduced in:_ [`9bb5ca9`](https://github.com/monochange/monochange/commit/9bb5ca9ca5315f60a1079a55470f7b77ff8e3ea2) _Related issues:_ [#364](https://github.com/monochange/monochange/issues/364)

## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-30)

### 🚀 Feature

#### Improve lint and check progress output

Add beautiful interactive progress reporting for `mc check` and `mc lint`.

- Introduced `LintProgressReporter` trait in `monochange_core::lint` with 14 lifecycle hooks from planning through summary.
- Added `NoopLintProgressReporter` for silent / backward-compatible operation.
- Updated `Linter::lint_workspace` to emit planning, suite, file, rule, and summary events to the reporter.
- Created `HumanLintProgressReporter` in `monochange` that writes animated spinners, suite-level progress, fix tracking, and a styled summary to stderr.
- Enhanced `format_check_report` to list which files were fixed when `--fix` is active.
- Respects `NO_COLOR` and `MONOCHANGE_NO_PROGRESS` environment variables.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #281](https://github.com/monochange/monochange/pull/281) _Introduced in:_ [`a9eec58`](https://github.com/monochange/monochange/commit/a9eec586e72d0a8704d08b7552523e0bd85ed20d) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Configure changeset lint rules

Add configurable changeset lint rules under `[lints.rules]` for summaries, section headings, bump-specific requirements, and changelog-type-specific requirements.

Rules can target built-in or custom changeset types with dynamic ids like `changesets/types/breaking` and `changesets/types/unicorns`, while unknown type ids are rejected during configuration loading.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #326](https://github.com/monochange/monochange/pull/326) _Introduced in:_ [`285fd69`](https://github.com/monochange/monochange/commit/285fd697b99502e9d95716413c533251307f0010) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

### 🧪 Testing

#### Add property-based and mutation tests

Add property-based testing with proptest to `monochange_core` and `monochange_semver`. Add mutation-testing-killing tests to `monochange_graph` and `monochange_config` to close coverage gaps identified by `cargo-mutants`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #266](https://github.com/monochange/monochange/pull/266) _Introduced in:_ [`9ef1044`](https://github.com/monochange/monochange/commit/9ef1044ccdde303143453d6880cc68df374c0c13) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add mutation regression coverage

Add cargo-mutants regression coverage for `monochange_analysis`, `monochange_config`, `monochange_core`, and `monochange_hosting`.

- `monochange_analysis`: add tests for `latest_workspace_release_tag` to ignore namespaced tags, `snapshot_files_from_working_tree` and `read_text_file_from_git_object` for medium/exact-size files, `detect_raw_pr_environment` to reject non-PR events across CI providers, `default_branch_name` with origin HEAD symbolic ref, `get_merge_base` returning actual merge base, and `ChangeFrame::changed_files` distinguishing working directory from staged-only.
- `monochange_config`: add fixture-backed tests and proptests for ecosystem versioned-file inheritance, explicit group bump inference, and changelog validation defaults to close mutation gaps in release-planning configuration loading.
- `monochange_core`: add fixture-backed tests for discovery filtering around parent `.git` directories outside the workspace root and block-comment stripping edge cases in JSON helper logic.
- `monochange_hosting`: add HTTP-mock tests for error paths in `get_json`, `get_optional_json`, `post_json`, `put_json`, and `patch_json` to kill mutants on status-code checks. Update `Cargo.toml` to include `src/*.rs` in the distribution manifest so `__tests.rs` builds correctly with `httpmock` as a dev-dependency.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #277](https://github.com/monochange/monochange/pull/277) _Introduced in:_ [`4c411d9`](https://github.com/monochange/monochange/commit/4c411d9efe84aaefbe2231ac16e4065249fc2a06) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

### 📝 Changed

#### Add additional property-based tests

Add additional property-based tests across crates.

- `monochange_config`: proptests for `render_changelog_path_template` with `{{package_dir}}`, `{{package_name}}`, unknown placeholder passthrough, and idempotence. Remove unused `minijinja` dependency.
- `monochange_core`: proptests for `strip_json_comments` on string literal preservation, line/block comment removal, idempotence, and comment-free input preservation.
- `monochange_graph`: proptests for `trigger_priority` ordering, `propagation_is_suppressed` correctness, and `NormalizedGraph` reverse edge correctness.
- `monochange_semver`: proptests for `merge_severities` idempotence and Major severity absorbing all values.
- Fix clippy `needless_borrow` lint in `monochange_github` and `indexing_slicing` lint in `monochange_graph` proptests.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #268](https://github.com/monochange/monochange/pull/268) _Introduced in:_ [`015c48d`](https://github.com/monochange/monochange/commit/015c48d77818b6e195d740986d7fa571867ad9b5) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### add attestation policy configuration

Add first-class package and release attestation policy settings. Package publish settings now support `publish.attestations.require_registry_provenance`, inherited from ecosystem publish settings and overridable per package.

When registry provenance is required, built-in release publishing fails before invoking registry commands unless trusted publishing is enabled, the current CI/OIDC identity is verifiable, the provider/registry capability matrix reports registry-native provenance support, and the built-in publisher can require that provenance. npm release publishes add `--provenance` only when this policy is enabled.

GitHub release asset attestation intent is modeled under `[source.releases.attestations]` with `require_github_artifact_attestations`, which is accepted only for GitHub sources.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #339](https://github.com/monochange/monochange/pull/339) _Introduced in:_ [`2849f7b`](https://github.com/monochange/monochange/commit/2849f7b771c24b2e3459450661d24c7f36c91a9c) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#315](https://github.com/monochange/monochange/issues/315)

#### Add initial changelog headers

Changelog targets now support `initial_header` in `[defaults.changelog]`, `[package.<id>.changelog]`, and `[group.<id>.changelog]`. monochange renders the header only when creating a changelog from empty content, preserving existing preambles on later releases.

When no custom header is configured, the selected changelog format provides a built-in default header.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #344](https://github.com/monochange/monochange/pull/344) _Introduced in:_ [`ad6c84b`](https://github.com/monochange/monochange/commit/ad6c84b8de13a5849b262a4db2e2ac87710944f2)

#### Add changelog section thresholds

`monochange` changelog rendering can now hide or collapse sections based on each section's configured priority. This lets you keep high-signal sections expanded while moving low-priority notes into collapsible markdown blocks or omitting them entirely.

Add the new workspace setting under `[changelog.section_thresholds]`:

```toml
[changelog.section_thresholds]
collapse = 50
ignored = 100
```

With that configuration:

- sections with `priority < 50` stay fully expanded
- sections with `priority >= 50` render inside markdown `<details>` blocks
- sections with `priority > 100` are omitted from the rendered changelog

**Before:** every configured `changelog.sections` entry rendered normally once it had entries.

```toml
[changelog.sections]
feat = { heading = "Added", priority = 20 }
docs = { heading = "Documentation", priority = 40 }
other = { heading = "Other", priority = 50 }
```

```md
## 1.2.3

### 🚀 Feature

- ship a new release workflow

### 📦 Other

- internal cleanup note
```

**After:** lower-priority sections can collapse automatically.

```toml
[changelog.sections]
feat = { heading = "Added", priority = 20 }
docs = { heading = "Documentation", priority = 40 }
other = { heading = "Other", priority = 50 }

[changelog.section_thresholds]
collapse = 50
ignored = 100
```

```md
## 1.2.3

### 🚀 Feature

- ship a new release workflow

<details>
<summary><strong>Other</strong></summary>

- internal cleanup note

</details>
```

This release also updates the generated init config and workspace config annotations so the new thresholds are documented where `monochange.toml` is authored.

> **Breaking change for Rust library consumers** — `monochange_core::ReleaseNotesSection` and `monochange_core::ChangelogSettings` now carry the new changelog-threshold metadata, so manual struct literals must include the added fields.

**Before (`monochange_core`):**

```rust
ReleaseNotesSection {
    title: "Documentation".to_string(),
    entries: vec!["- update migration guide".to_string()],
}

ChangelogSettings {
    templates,
    sections,
    types,
}
```

**After:**

```rust
ReleaseNotesSection {
    title: "Documentation".to_string(),
    collapsed: true,
    entries: vec!["- update migration guide".to_string()],
}

ChangelogSettings {
    templates,
    sections,
    section_thresholds,
    types,
}
```

_Owner:_ Ifiok Jr. _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`4426b99`](https://github.com/monochange/monochange/commit/4426b9916791ceff82957f61837be1e681988c9a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add post-merge release automation

- Add `release-pr-manual-merge-blocker` job to CI that fails on PRs from `monochange/release/*` branches, forcing the `/merge` slash-command workflow
- Protect the `release-pr` job with `environment: publisher` so branch-protection rules apply
- Introduce a `release-post-merge` job that runs `PublishRelease` and `CommentReleasedIssues` steps after a release PR merges
- Add `from-ref` support to `PublishRelease` and `CommentReleasedIssues` for discovering the release record from the merge commit when `prepared_release` context is unavailable
- Add `auto-close-issues` flag to `CommentReleasedIssues` that closes released issues not already closed by a PR reference
- Store `changesets` in `ReleaseRecord` so post-merge steps can resolve related issues without access to the deleted changeset files
- Update `plan_released_issue_comments` to include all issue relationships and set `close` state appropriately
- Update `comment_released_issues_with_client` to PATCH issue state to `"closed"` when `plan.close` is `true`
- Add dedicated composite actions: `publish-release` and `comment-released-issues`
- Add `publish-release` and `comment-released-issues` CLI step definitions to `monochange.toml.template`

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #282](https://github.com/monochange/monochange/pull/282) _Introduced in:_ [`014491d`](https://github.com/monochange/monochange/commit/014491ddb0de1a562bd0ca6552bba9646baf7f42) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Expose resolved configuration in CLI templates

Configured CLI workflows can now reference the resolved workspace configuration and paths without adding a dedicated setup step. Templates receive `config`, `project_root`, and `config_path` by default, so later steps can read values such as `{{ config.packages.0.id }}` while regular workflow execution stays quiet unless a step intentionally emits output.

The new built-in config step also makes the generated command available for CI and debugging:

```bash
mc step:config
```

It prints JSON containing the canonical project root, the `monochange.toml` path, and the resolved configuration:

```json
{
	"projectRoot": "/workspace/repo",
	"configPath": "/workspace/repo/monochange.toml",
	"config": {
		"packages": []
	}
}
```

This gives scripts and GitHub Actions a stable command for inspecting the exact configuration that `monochange` loaded while preserving the no-output default for configured workflow steps.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #343](https://github.com/monochange/monochange/pull/343) _Introduced in:_ [`fc627b3`](https://github.com/monochange/monochange/commit/fc627b38392bc32dc9402a64a5bcc95572a94c3c)

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #156](https://github.com/monochange/monochange/pull/156) _Introduced in:_ [`519f841`](https://github.com/monochange/monochange/commit/519f841929c6a06d5b3a578b206982d2d6cc1548) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#133](https://github.com/monochange/monochange/issues/133)

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #152](https://github.com/monochange/monochange/pull/152) _Introduced in:_ [`81b0882`](https://github.com/monochange/monochange/commit/81b0882525ab51d74b0e8cc2a0114aac0fdb3a7f) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#132](https://github.com/monochange/monochange/issues/132)

#### Add PyPI publishing support

Add first-class PyPI publishing support for Python packages, including PyPI as the built-in Python registry, placeholder package generation, `uv build` / `uv publish` command generation, PyPI JSON API publication checks, trusted-publisher setup guidance, and rate-limit planning coverage.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #322](https://github.com/monochange/monochange/pull/322) _Introduced in:_ [`afe5959`](https://github.com/monochange/monochange/commit/afe595944146c3ec4c9798098c92c031d4279e6a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Handle large release commit messages reliably

Release commit creation now streams the generated commit message through standard input instead of passing the full release record as a command-line argument. This avoids operating-system argument length limits for large release records.

Git command spawning now reuses a stable path that contains `git`, and benchmark fixture git commands now use monochange's sanitized git command helper with automatic garbage collection disabled for deterministic synthetic history setup in CI.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #320](https://github.com/monochange/monochange/pull/320) _Introduced in:_ [`f41f985`](https://github.com/monochange/monochange/commit/f41f985288e1440b3b64c2fa9c1cda987925ef8a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Update repository URLs

Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Publish GitHub releases through drafts

- Add a boolean `draft` input to the built-in `PublishRelease` step so CLI commands can create hosted releases as drafts while preserving `[source.releases].draft` defaults.
- Update release automation to create draft GitHub releases, run the asset upload workflow against those drafts, then publish the drafts after assets are attached.
- Add a global `--jq` filter for JSON-producing commands so automation can extract release tags and other fields directly from `--format json` output.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #325](https://github.com/monochange/monochange/pull/325) _Introduced in:_ [`bee06fe`](https://github.com/monochange/monochange/commit/bee06fed90e50cda3de695b973f415dd162eec29) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add readiness-backed publish planning

`mc publish-plan` now accepts `--readiness <path>` for normal package publish planning. The plan validates that the `mc publish-readiness` artifact matches the current release record and covers the selected package set, then limits rate-limit batches to package ids that are ready in both the artifact and a fresh local readiness check.

Placeholder publish planning continues to reject readiness artifacts and should be run with `mc publish-plan --mode placeholder`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #305](https://github.com/monochange/monochange/pull/305) _Introduced in:_ [`e80c2b8`](https://github.com/monochange/monochange/commit/e80c2b8f1fd1df155e4aa05df8977f245f89bbc5) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Require publish readiness artifacts

Require real `mc publish` package-registry runs to pass a readiness artifact generated by `mc publish-readiness`.

`mc publish-readiness` JSON artifacts now include schema metadata, release-record commit metadata, and a deterministic package-set fingerprint. `PublishPackages` validates the artifact before registry mutation and rejects missing, blocked, stale, malformed, duplicate, or package-mismatched readiness artifacts while leaving `--dry-run` publish previews artifact-free.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #301](https://github.com/monochange/monochange/pull/301) _Introduced in:_ [`97337aa`](https://github.com/monochange/monochange/commit/97337aad65e1f9dfc4d97fd381592b3bd57bc30a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add publish resume support

Add package publish result artifacts plus `mc publish --resume <path>` for retrying incomplete registry publishing after partial failures.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #323](https://github.com/monochange/monochange/pull/323) _Introduced in:_ [`8bca357`](https://github.com/monochange/monochange/commit/8bca35730a78f61c22dc71e473bc67a77210c4c6) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #321](https://github.com/monochange/monochange/pull/321) _Introduced in:_ [`e888554`](https://github.com/monochange/monochange/commit/e888554ca981816b80f5135086e8b226ee8f0a20) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#310](https://github.com/monochange/monochange/issues/310)

#### Commit release PR messages from a file

Commit generated release pull request messages from a temporary file and include detailed commit diagnostics when git cannot create the release commit.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #328](https://github.com/monochange/monochange/pull/328) _Introduced in:_ [`7c4f80a`](https://github.com/monochange/monochange/commit/7c4f80a217cf16716221d701ef3fe52c1ea65443) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add no-verify support to release automation

> **Breaking change** — library consumers that construct `monochange_core::CliStepDefinition::CommitRelease` or `OpenReleaseRequest`, or that call the exported git/provider release helpers directly, must now handle the new `no_verify` field/argument.

Release automation can now bypass local git hooks when creating the generated release commit and when pushing the release request branch. This is useful for CI-driven `mc release-pr` flows where repository hooks depend on tools that are not available in the runner environment.

**Before (`monochange.toml`):**

```toml
[cli.release-pr]
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json", "markdown"], default = "markdown" },
]
steps = [
	{ type = "CommitRelease", name = "create release commit" },
	{ type = "OpenReleaseRequest", name = "create the pr" },
]
```

**After:**

```toml
[cli.release-pr]
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json", "markdown"], default = "markdown" },
	{ name = "no_verify", type = "boolean", default = true },
]
steps = [
	{ type = "CommitRelease", name = "create release commit", inputs = { no_verify = "{{ inputs.no_verify }}" } },
	{ type = "OpenReleaseRequest", name = "create the pr", inputs = { no_verify = "{{ inputs.no_verify }}" } },
]
```

That keeps the `mc release-pr` invocation the same while making hook bypass explicit in config:

```bash
mc release-pr
```

For crate consumers, the step and git helper APIs now carry the same flag through the full release-request pipeline.

**Before (`monochange_core` / hosting adapters):**

```rust
// before
CliStepDefinition::CommitRelease { name, when, inputs }
CliStepDefinition::OpenReleaseRequest { name, when, inputs }

git_commit_paths_command(root, &message)
git_push_branch_command(root, branch)
```

**After:**

```rust
// after
CliStepDefinition::CommitRelease { name, when, no_verify, inputs }
CliStepDefinition::OpenReleaseRequest { name, when, no_verify, inputs }

git_commit_paths_command(root, &message, no_verify)
git_push_branch_command(root, branch, no_verify)
```

Provider-facing release helpers in `monochange_hosting`, `monochange_github`, `monochange_gitlab`, and `monochange_gitea` now forward that flag so a single `no_verify` choice applies consistently to commit creation and branch push operations.

_Owner:_ Ifiok Jr. _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`8b73540`](https://github.com/monochange/monochange/commit/8b7354011d99194a74450ad6907bcff5978b8e28) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Consolidate affected-package configuration

> **Breaking change** — affected-package policy now lives in `[changesets.affected]`. Configurations using the previous `[changesets.verify]` section must rename it, and configurations using the hosted-source affected-package policy section (`[source.bot.changesets]` in older configs, or `[source.affected]` in prerelease configs) must move `enabled`, `required`, `skip_labels`, `comment_on_failure`, `changed_paths`, and `ignored_paths` into `[changesets.affected]`.

Move affected-package policy settings into the changesets configuration:

```toml
[changesets.affected]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["Cargo.toml", "Cargo.lock"]
ignored_paths = ["**/tests/**"]
```

The Rust configuration model now exposes `ChangesetSettings::affected` with `ChangesetAffectedSettings`; the previous `ChangesetSettings::verify`, `SourceConfiguration::bot`, `SourceConfiguration::affected`, `ProviderChangesetBotSettings`, `ProviderAffectedSettings`, and `ProviderBotSettings` types or fields have been removed.

The `mc affected` policy command now reports `skipped` when it runs on a generated release pull request branch whose name starts with `source.pull_requests.branch_prefix`, allowing CI to ignore those branches.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #340](https://github.com/monochange/monochange/pull/340) _Introduced in:_ [`d5d8856`](https://github.com/monochange/monochange/commit/d5d8856b1522cd4ad70eeb06abd4d33ad7f0c9b6)

#### Expose built-in CLI steps as commands

Expose built-in CLI steps as immutable `step:*` commands and move default workflows into generated config.

Rename the `AffectedPackages` revision input from `since` to `from`, so the generated command now accepts `mc step:affected-packages --from <ref>`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #290](https://github.com/monochange/monochange/pull/290) _Introduced in:_ [`ec09ba2`](https://github.com/monochange/monochange/commit/ec09ba234cdc81fb5468292651a7c7968ffe7677) _Last updated in:_ [`72fca4d`](https://github.com/monochange/monochange/commit/72fca4d87f53053dd103e577f871581031a5088d)

#### Prefer verified GitHub release PR commits

When `[source.pull_requests].verified_commits = true`, `mc release-pr` publishes a GitHub release pull request from GitHub Actions by asking the GitHub provider to recreate the release branch commit through the Git Database API and only moves the branch when GitHub marks the replacement commit as verified.

The setting defaults to `false`. If the API commit cannot be created, is not verified, or the branch changes before the replacement lands, monochange leaves the normal pushed git commit in place and continues with the release PR flow.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #306](https://github.com/monochange/monochange/pull/306) _Introduced in:_ [`f93547d`](https://github.com/monochange/monochange/commit/f93547d919cf5bcbffe61beb675fa053307520c8) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

### 💥 Breaking Change

#### replace changelog_sections with [changelog] section

The `changelog_sections` array-based config and `[release_notes]` section have been replaced with a new top-level `[changelog]` section that separates concerns into types (changeset type → section + bump routing) and sections (display headings with priorities).

**Before** (`[release_notes]` and `changelog_sections`):

```toml
[release_notes]
change_templates = ["- {{ summary }}"]

[package.core]
changelog_sections = [
	{ name = "Security", types = ["security"], bump = "patch" },
]
```

**After** (`[changelog]` with sections and types):

```toml
[changelog]
templates = ["- {{ summary }}"]

[changelog.sections.security]
heading = "Security"
priority = 40

[changelog.types.security]
section = "security"
bump = "patch"

[package.core]
excluded_changelog_types = [] # filter inherited types instead of overriding
```

Key changes:

- `[release_notes]` → `[changelog]` with `templates` instead of `change_templates`
- `changelog_sections` arrays → `[changelog.sections]` (keyed map) + `[changelog.types]` (keyed map)
- Per-package/group overrides → `excluded_changelog_types` (filters workspace defaults instead of complete replacement)
- Type keys must be lowercase, start with a letter, and contain at most one underscore
- Built-in defaults provide 13 sections and 13 types when `[changelog]` is empty
- Bump-severity shorthand types (`minor`, `patch`, `major`, `none`) each get their own title-case section heading (Minor, Patch, Major, None) by default, instead of routing to semantic sections like Added/Fixed

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #259](https://github.com/monochange/monochange/pull/259) _Introduced in:_ [`c698d29`](https://github.com/monochange/monochange/commit/c698d29ffabefab418a0a750a06de3b1d6074561) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### 🚀 Feature

#### add `caused_by` changeset context for dependency propagation

You can now annotate dependency-only follow-up changesets with `caused_by`, use `mc change --caused-by ...` to author them, inspect the linkage in diagnostics output, and suppress matching automatic dependency propagation during release planning.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #245](https://github.com/monochange/monochange/pull/245) _Introduced in:_ [`8ec612b`](https://github.com/monochange/monochange/commit/8ec612beb9a8b8100037435695826042bc7361c4) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#208](https://github.com/monochange/monochange/issues/208)

#### add MCP tools for change analysis and validation

Introduces two new MCP tools to help agents generate and validate changesets programmatically:

**`monochange_analyze_changes`**

Analyzes git diffs and suggests granular changeset structure. Supports multiple detection modes:

```json
{
	"path": "/path/to/repo",
	"frame": "main...feature-branch",
	"detection_level": "signature",
	"max_suggestions": 10
}
```

The tool automatically detects:

- Which packages have changes
- What type of artifact each package is (library, app, CLI)
- Semantic changes (new functions, modified components, etc.)
- Appropriate grouping based on configurable thresholds

**`monochange_validate_changeset`**

Validates that a changeset accurately describes the actual code changes:

```json
{
	"path": "/path/to/repo",
	"changeset_path": ".changeset/feature.md"
}
```

Checks:

- Does the summary match the actual diff content?
- Is the bump level appropriate for the change type?
- Are there undocumented API changes?

**Before:**

Agents had to manually inspect diffs and decide what changesets to create.

**After:**

```bash
# Start MCP server
mc mcp

# Then use the analyze_changes tool to get suggestions
# for all packages with modifications
```

These tools integrate with the new `monochange_analysis` crate to provide intelligent, context-aware changeset recommendations.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #206](https://github.com/monochange/monochange/pull/206) _Introduced in:_ [`a417022`](https://github.com/monochange/monochange/commit/a417022f80f93d61add00b8087e0f80102a9fd52) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #255](https://github.com/monochange/monochange/pull/255) _Introduced in:_ [`26e13ff`](https://github.com/monochange/monochange/commit/26e13fff071e93dc32fe071a5771232c980ebd46) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add ecosystem-specific semantic analysis for MCP changeset workflows

`monochange_analyze_changes` and `monochange_validate_changeset` now return real semantic analysis for Cargo, npm, Deno, and Dart/Flutter packages instead of placeholder results.

**Before (`monochange_analyze_changes`):**

```json
{
	"ok": true,
	"summary": "Analysis complete - review suggested changesets for each package",
	"analysis": {
		"package_changes": {}
	}
}
```

**After (`monochange_analyze_changes`):**

```json
{
	"ok": true,
	"summary": "Analyzed 1 package(s) and found 6 semantic change(s)",
	"analysis": {
		"packageAnalyses": {
			"core": {
				"semanticChanges": [
					{
						"category": "public_api",
						"kind": "modified",
						"itemPath": "greet"
					},
					{
						"category": "dependency",
						"kind": "added",
						"itemPath": "tracing"
					}
				]
			}
		}
	}
}
```

`monochange_validate_changeset` now validates authored changesets against that semantic diff and can flag stale symbol references or underspecified summaries across all supported ecosystems.

Current analyzer coverage includes:

- Cargo public Rust API diffs plus `Cargo.toml` dependency and manifest metadata changes
- npm-family JS/TS exported symbol diffs plus `package.json` exports, commands, dependency, and script changes
- Deno JS/TS exported symbol diffs plus `deno.json` exports, import aliases, task, and compiler-option changes
- Dart and Flutter public `lib/` API diffs plus `pubspec.yaml` executables, dependency, environment, and plugin-platform changes

**Before (`monochange_validate_changeset`):**

```json
{
	"ok": true,
	"valid": true,
	"issues": []
}
```

**After (`monochange_validate_changeset`):**

```json
{
	"ok": false,
	"valid": false,
	"lifecycle_status": "stale",
	"issues": [
		{
			"severity": "error",
			"message": "changeset references `OldGreeter` but that item was not found in the current semantic diff"
		}
	]
}
```

`monochange_core` now exposes shared semantic-analysis contracts and diff record types so ecosystem crates can own their analyzers without moving parser logic into the CLI crate.

`@monochange/skill` now documents the semantic-analysis-backed MCP workflows and the expanded cross-ecosystem validation guidance for assistant consumers.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #247](https://github.com/monochange/monochange/pull/247) _Introduced in:_ [`8c96c8f`](https://github.com/monochange/monochange/commit/8c96c8f0a3b9d44bf30148b5a83067d7ce3ab62b) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#243](https://github.com/monochange/monochange/issues/243) _Related issues:_ [#244](https://github.com/monochange/monochange/issues/244)

#### add core linting types

Add `monochange_core::lint` module with the foundational types for the linting system:

- `LintSeverity` (Off, Warning, Error) — rule severity levels
- `LintCategory` (Style, Correctness, Performance, Suspicious, BestPractice) — rule classification
- `LintRule` — rule definition with id, name, description, and autofixable flag
- `LintResult`, `LintLocation` — individual findings with file location and byte spans
- `LintFix`, `LintEdit` — autofix suggestions with span-based replacements
- `LintRuleConfig` — flexible configuration supporting simple severity or detailed options
- `LintReport` — aggregated results with error/warning counts
- `LintContext` — rule input with workspace root, manifest path, and file contents
- `LintRuleRunner` trait — executable rule interface with `rule()`, `applies_to()`, and `run()`
- `LintRuleRegistry` — rule registration and discovery

Also adds `lints` field to `EcosystemSettings` for per-ecosystem lint configuration and `Lint` variant to `CliStepDefinition` with `format`, `fix`, and `ecosystem` inputs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #207](https://github.com/monochange/monochange/pull/207) _Introduced in:_ [`a650862`](https://github.com/monochange/monochange/commit/a650862f2dc69b6538f6403bab5b66079f9c1304) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add lint include/exclude path filters with optional gitignore override

`[lints]` now supports workspace-level path filters for manifest linting.

**New options:**

- `include` — optional glob patterns that opt manifest paths into linting
- `exclude` — glob patterns that remove matching manifest paths from linting
- `disable_gitignore` — opt back into linting gitignored manifests when needed

By default, monochange now skips gitignored manifests during linting while still allowing repositories to explicitly opt them back in.

The repository config and bundled linting docs now show how to exclude `examples/**` when example manifests would otherwise trigger false positives.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #248](https://github.com/monochange/monochange/pull/248) _Introduced in:_ [`b45cb78`](https://github.com/monochange/monochange/commit/b45cb787a75746a95799f957f01d92020d27f72f) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add package publication targets and publish step definitions

`monochange_core` now models package publication as first-class release metadata instead of leaving registry publication outside the release graph.

**Before:**

```rust
use monochange_core::CliStepDefinition;
use monochange_core::ReleaseManifest;

let step = CliStepDefinition::PublishRelease { /* ... */ };

let manifest = ReleaseManifest {
    release_targets: vec![],
    released_packages: vec![],
    changed_files: vec![],
    // no package publication metadata
    ..todo!()
};
```

**After:**

```rust
use monochange_core::CliStepDefinition;
use monochange_core::PackagePublicationTarget;
use monochange_core::PublishMode;
use monochange_core::RegistryKind;
use monochange_core::ReleaseManifest;

let step = CliStepDefinition::PublishPackages { /* ... */ };

let manifest = ReleaseManifest {
    package_publications: vec![PackagePublicationTarget {
        package: "core".to_string(),
        ecosystem: monochange_core::Ecosystem::Cargo,
        registry: Some(monochange_core::PublishRegistry::Builtin(
            RegistryKind::CratesIo,
        )),
        version: "1.2.3".to_string(),
        mode: PublishMode::Builtin,
        trusted_publishing: Default::default(),
    }],
    ..todo!()
};
```

New public types include:

- `PublishMode`
- `RegistryKind`
- `PublishRegistry`
- `PlaceholderSettings`
- `TrustedPublishingSettings`
- `PublishSettings`
- `PackagePublicationTarget`

`PackageDefinition`, `EcosystemSettings`, `ReleaseManifest`, and `ReleaseRecord` now all carry publish metadata, and the built-in CLI command set includes `placeholder-publish` and `publish` step definitions.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #205](https://github.com/monochange/monochange/pull/205) _Introduced in:_ [`3ed719e`](https://github.com/monochange/monochange/commit/3ed719e42d89d66b7db47528a69d1ecf1cdeada2) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add a dedicated `mc versions` command

monochange now ships a dedicated `mc versions` command and `DisplayVersions` CLI step for rendering planned package and group versions without the rest of the release preview.

**Before:**

```bash
mc release --dry-run --format text
```

Rendered the full release summary, including release targets, changed files, and other follow-up details.

**After:**

```bash
mc versions --format text
mc versions --format markdown
mc versions --format json
```

This trims the output down to package and group version summaries only, without mutating manifests, changelogs, or consuming changesets.

`monochange_config` also now includes the built-in `versions` command in the default CLI command set returned from workspace configuration loading, so embedded callers and config-driven integrations see the same built-in command list as the `mc` binary.

You can also expose the same behavior from custom commands with `DisplayVersions`:

```toml
[cli.versions]
help_text = "Display planned package and group versions"
inputs = [
	{ name = "format", type = "choice", choices = ["text", "markdown", "json"], default = "text" },
]
steps = [{ type = "DisplayVersions" }]
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #229](https://github.com/monochange/monochange/pull/229) _Introduced in:_ [`f148184`](https://github.com/monochange/monochange/commit/f148184d69fc4dc8720cde8db22768a8c1def8f7) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add publish rate-limit planning and batched publish metadata

monochange can now plan package-registry publish work before mutating registries. The new default `mc publish-plan` command renders built-in rate-limit metadata, recommended publish batches, evidence links, and optional CI snippets for GitHub Actions or GitLab CI. `mc publish` and `mc placeholder-publish` now also accept repeated `--package` filters so the planned batches can be executed one window at a time.

**Before:**

```bash
# no dedicated planning command
mc publish --dry-run
mc placeholder-publish --dry-run
```

Built-in publish flows had no registry-window report and no first-class batch plan to hand off to CI.

**After:**

```bash
mc publish-plan --format json
mc publish-plan --mode placeholder --ci github-actions
mc publish --dry-run --package core --package web --format json
mc placeholder-publish --package core --format json
```

`mc publish-plan` now reports:

- per-registry publish windows
- `batches` with explicit package ids per window
- evidence URLs plus confidence levels
- optional CI snippets that expand to `mc publish --package ...` invocations

Representative JSON now includes a publish-rate-limit section alongside publish output:

```json
{
	"packagePublish": { "...": "..." },
	"publishRateLimits": {
		"windows": [
			{
				"registry": "pub_dev",
				"pending": 13,
				"batchesRequired": 2,
				"fitsSingleWindow": false
			}
		],
		"batches": [
			{
				"registry": "pub_dev",
				"batchIndex": 1,
				"totalBatches": 2,
				"packages": ["core", "web"]
			}
		]
	}
}
```

Built-in catalog coverage now includes `crates.io`, `npm`, `jsr`, and `pub.dev`, with confidence and evidence attached to each policy.

#### add `publish.rate_limits.enforce` to workspace configuration

`monochange_config` and `monochange_core` now model per-ecosystem and per-package publish rate-limit enforcement so teams can decide whether planned overages should warn or block.

**Before (`monochange.toml`):**

```toml
[ecosystems.dart.publish]
mode = "builtin"
```

**After:**

```toml
[ecosystems.dart.publish]
mode = "builtin"

[ecosystems.dart.publish.rate_limits]
enforce = true
```

When `enforce = true`, built-in publish commands stop before running a package set that requires more than one known registry window. This lets CI fail early and lets teams split the work into planned follow-up batches instead of discovering throttling halfway through a release.

#### extend public publish planning types in `monochange_core`

`monochange_core` now exposes rate-limit settings and batch metadata for callers that build their own CLI or automation around monochange release plans.

**Before (`monochange_core`):**

```rust
pub struct PublishSettings {
	pub enabled: bool,
	pub mode: PublishMode,
	pub registry: Option<PublishRegistry>,
	pub trusted_publishing: TrustedPublishingSettings,
	pub placeholder: PlaceholderSettings,
}

pub struct PublishRateLimitReport {
	pub dry_run: bool,
	pub windows: Vec<RegistryRateLimitWindowPlan>,
	pub warnings: Vec<String>,
}
```

**After:**

```rust
pub struct PublishSettings {
	pub enabled: bool,
	pub mode: PublishMode,
	pub registry: Option<PublishRegistry>,
	pub trusted_publishing: TrustedPublishingSettings,
	pub rate_limits: PublishRateLimitSettings,
	pub placeholder: PlaceholderSettings,
}

pub struct PublishRateLimitReport {
	pub dry_run: bool,
	pub windows: Vec<RegistryRateLimitWindowPlan>,
	pub batches: Vec<PublishRateLimitBatch>,
	pub warnings: Vec<String>,
}
```

This keeps config parsing, runtime enforcement, dry-run JSON, and library consumers aligned around the same publish-batch model.

#### fix standalone npm package discovery ids in `monochange_npm`

`monochange_npm` now normalizes standalone package ids relative to the discovery root so repositories that contain multiple standalone `package.json` files no longer collapse distinct packages into one record.

**Before:**

- two standalone npm packages under different directories could share the same internal `npm:package.json` id
- the adapter could keep only one package depending on filesystem traversal order

**After:**

- standalone npm packages get stable root-relative ids such as `npm:packages/docs/package.json`
- publish planning and other discovery-driven flows now keep both packages consistently across platforms

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #225](https://github.com/monochange/monochange/pull/225) _Introduced in:_ [`98013cb`](https://github.com/monochange/monochange/commit/98013cb86d5644a7327dc2ee5803d747d4a0372c) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### 📝 Changed

#### move crate include lists into published manifests

The published library crates in this workspace now declare their `include` file lists in each crate's own `Cargo.toml` instead of inheriting that setting from `[workspace.package]`.

**Before (`crates/monochange_core/Cargo.toml`):**

```toml
[package]
include = { workspace = true }
readme = "readme.md"
```

The package contents depended on the root workspace manifest carrying:

```toml
[workspace.package]
include = ["src/**/*.rs", "Cargo.toml", "readme.md"]
```

**After:**

```toml
[package]
include = ["src/**/*.rs", "Cargo.toml", "readme.md"]
readme = "readme.md"
```

This keeps each published crate self-contained when packaging, auditing, or updating manifest metadata and avoids relying on a shared workspace-level `include` definition for crates.io package contents.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #227](https://github.com/monochange/monochange/pull/227) _Introduced in:_ [`78af3c2`](https://github.com/monochange/monochange/commit/78af3c244a4090965b455e2879b33a160e28da77) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #228](https://github.com/monochange/monochange/pull/228) _Introduced in:_ [`94f06a0`](https://github.com/monochange/monochange/commit/94f06a057150d26e5f330e2e49a08f71eb12fc92) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### 🔨 Refactor

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #224](https://github.com/monochange/monochange/pull/224) _Introduced in:_ [`d0f76ed`](https://github.com/monochange/monochange/commit/d0f76ed56fa18e0ca9d9ec20fa9e44d413014db7) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### 📝 Changed

#### add the first Dart lint suite foundation

monochange now wires Dart manifests into the ecosystem-owned lint registry and supports scaffolding Dart lint files with `mc lint new dart/<rule-name>`.

This foundation change adds:

- a new `monochange_dart::lints` module with target collection for `pubspec.yaml`
- Dart lint suite registration in the `mc lint` and `mc check` command paths
- Dart lint scaffolding support in `mc lint new`
- tests covering managed Dart lint target collection and fixture filtering

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #235](https://github.com/monochange/monochange/pull/235) _Introduced in:_ [`5a7a4fe`](https://github.com/monochange/monochange/commit/5a7a4fed84603f51dd5d152d11e739f30dea2b64) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#230](https://github.com/monochange/monochange/issues/230)

## [0.1.0](https://github.com/monochange/monochange/releases/tag/v0.1.0) (2026-04-13)

### 💥 Breaking Change

#### 🚀 Initial public release of monochange

**monochange** is a Rust-based release-planning toolkit for monorepos that span multiple package ecosystems. It is designed from the ground up to support the modern, AI-driven development landscape where agents and automation play a central role in software delivery.

##### What is monochange?

In today's agent-driven development environment, managing releases across diverse package ecosystems (Rust, JavaScript/TypeScript, Dart, Python, etc.) becomes increasingly complex. monochange provides a unified, programmatic interface for:

- **Change tracking**: Structured changesets that capture intent across multiple packages
- **Release planning**: Automated versioning and changelog generation
- **Multi-ecosystem support**: Native handling of Cargo, NPM, Dart, Deno, and more
- **CI/CD integration**: Seamless workflows for Gitea, GitHub, and GitLab
- **Graph-based dependency analysis**: Understanding package relationships across your monorepo

##### Why monochange matters for AI-driven workflows

As development teams increasingly rely on AI agents to generate code, manage dependencies, and orchestrate releases, monochange provides the structured foundation these agents need to operate effectively. It transforms release management from a manual, error-prone process into a deterministic, automatable workflow.

##### What's included in this release

This first release includes:

- Core changeset management engine
- Multi-ecosystem package detection and versioning
- Hosting provider integrations (Gitea, GitHub, GitLab)
- Semantic versioning utilities
- Configurable release workflows
- CLI tooling for validation and release orchestration

For complete feature details, architecture overview, and usage examples, see the [documentation](https://docs.rs/monochange).

_Owner:_ Ifiok Jr. _Introduced in:_ [`4542b5a`](https://github.com/monochange/monochange/commit/4542b5aee8b63a86c7ffc0ea9436090162a18056)
