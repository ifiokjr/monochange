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

## [0.5.1](https://github.com/monochange/monochange/releases/tag/v0.5.1) (2026-05-15)

### 📝 Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.5.1 as part of group `main`.

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-14)

### 🚀 Feature

#### Configurable publish-order dependency fields

Add configurable ecosystem-specific dependency fields for package publish ordering across npm, Cargo, Deno, Dart/Flutter, Python, and Go.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #472](https://github.com/monochange/monochange/pull/472) _Introduced in:_ [`0d9cf46`](https://github.com/monochange/monochange/commit/0d9cf461a05057b61efa987d361ebd27d800dbdb) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8) _Closed issues:_ [#465](https://github.com/monochange/monochange/issues/465)

#### Publish all configured packages

Add a `--all` flag to the PublishPackages CLI step so migration workflows can publish every configured package, including packages that were not part of the prepared release record.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #461](https://github.com/monochange/monochange/pull/461) _Introduced in:_ [`3d956cd`](https://github.com/monochange/monochange/commit/3d956cd3e34747e088add98fe0358251f388782f) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Add progress logging to `mc publish`

When running `mc publish`, each package being processed is now logged via `tracing::info!` so users can observe progress in real time. Use `--log-level info` or set `RUST_LOG=info` to see these messages. When `--quiet` is set, no tracing subscriber is initialized so the log messages are silently discarded (zero overhead).

Log events emitted during the publish loop:

- **`publishing package`** — at the start of processing each package, with `package_name`, `version`, `registry`, `dry_run`, and `mode` fields
- **`skipping external package`** — when a package opts out of built-in publishing
- **`skipping already-published version`** — when the version already exists on the registry
- **`would publish package (dry run)`** — when `--dry-run` would publish the package
- **`published package`** — on successful publish
- **`publish command failed to execute`** (`tracing::error`) — when the publish command cannot run
- **`publish command returned non-zero exit`** (`tracing::error`) — when the publish command fails

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #453](https://github.com/monochange/monochange/pull/453) _Introduced in:_ [`586ffb6`](https://github.com/monochange/monochange/commit/586ffb6b61c7f61b0a6bbcafc8dc2dbfa66d7203) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Remove automated npm trust configuration during publish

Removed the `npm trust` command execution from the publish loop. Trust configuration for npm packages must now be done manually or via separate tooling — `mc publish` no longer runs `npm trust github` or `npm trust list` automatically.

When trusted publishing is enabled for npm packages, the publish command now uses `npm` directly instead of `pnpm` (already the case via `npm_publish_program`). An environment variable override for forcing pnpm during trusted publishing can be added in a future release.

Removed `PublishTrustHandler::configure_successful_publish_trust` from the trait and its `CliPublishTrustHandler` implementation. Removed `configure_npm_trusted_publishing` from `package_publish`. Removed `build_npm_trust_list_command` from `monochange_npm`. The `trust_outcome_for_skip` and `planned_trust_outcome` methods remain, showing informational messages about how to manually configure trust.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #456](https://github.com/monochange/monochange/pull/456) _Introduced in:_ [`628a1ea`](https://github.com/monochange/monochange/commit/628a1ea18b62b60551c7648e16405a685cacb5f4) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

### 🐛 Fixed

#### Include Cargo development dependencies in publish ordering

Cargo package publishing now orders runtime, build, and development dependencies before dependents. This prevents a crate from being published before an unpublished workspace crate referenced through `dev-dependencies` or `build-dependencies`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #466](https://github.com/monochange/monochange/pull/466) _Introduced in:_ [`add0671`](https://github.com/monochange/monochange/commit/add0671b798d2dd4ab6e142801b1b5cac6842a1a) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Validate Cargo private dependency publishing hazards

Cargo linting now reports publishable packages that depend on private workspace packages through `dependencies`, `dev-dependencies`, or `build-dependencies`. Package publish dry runs now execute the registry dry-run command and preserve its stdout and stderr in the publish report instead of only planning the command.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #470](https://github.com/monochange/monochange/pull/470) _Introduced in:_ [`66ffdf7`](https://github.com/monochange/monochange/commit/66ffdf734129fb267fe61dd821e55c292dab5c0e) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Publish progress output

Add emoji-based publish progress reporting on stderr with deterministic CI-friendly output and terminal-aware loading markers.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #469](https://github.com/monochange/monochange/pull/469) _Introduced in:_ [`603c731`](https://github.com/monochange/monochange/commit/603c731a60d66f49b876a14467909efd4585408a) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

## [0.4.2](https://github.com/monochange/monochange/releases/tag/v0.4.2) (2026-05-10)

### 🚀 Feature

#### Order publish plans by dependencies

Order publish plans by workspace dependencies before applying registry rate-limit windows, and run CI publishing as one dependency-ordered publish operation.

This keeps dependent packages from publishing before their internal dependencies are available and adds realistic fixture coverage for non-alphabetical cargo dependency graphs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e) _Last updated in:_ [`2392845`](https://github.com/monochange/monochange/commit/2392845ec29289e3f219aca20ac343cf79ee965e)

## [0.4.1](https://github.com/monochange/monochange/releases/tag/v0.4.1) (2026-05-10)

### 🐛 Fixed

#### Split crate boundaries for changelog, config, and publish behavior

Move changelog rendering into `monochange_changelog`, shift publish planning and execution helpers into `monochange_publish`, and reduce direct concrete ecosystem/provider dependencies in `monochange_config`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #441](https://github.com/monochange/monochange/pull/441) _Introduced in:_ [`ae8ea56`](https://github.com/monochange/monochange/commit/ae8ea563ae95c6cc4e8d3d1acdc5303069ea44cf)

## [0.4.0](https://github.com/monochange/monochange/releases/tag/v0.4.0) (2026-05-09)

### 💥 Breaking Change

#### Extract publish support into a dedicated crate

Move the publish support surface out of the top-level `monochange` crate and into the new `monochange_publish` crate. The extracted crate now owns the publish report/request models, trusted-publishing capability detection, provider/registry capability messages, and built-in publish command builders for npm, pnpm, Cargo, Dart, Flutter, JSR, PyPI, and Go proxy releases.

This keeps `monochange` focused on orchestration while giving publish integrations a dedicated crate boundary for future registry checks, readiness logic, and provider-specific publishing workflows.

```text
monochange_publish owns reusable publish capabilities and command construction.
monochange wires those capabilities into CLI workflows and release orchestration.
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #397](https://github.com/monochange/monochange/pull/397) _Introduced in:_ [`fa78e4d`](https://github.com/monochange/monochange/commit/fa78e4db56fd3a6897896c6e1b1c62ea2d8e46b9) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

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

#### Move CommandExecutor and command rendering into monochange_publish

Extract `CommandOutput`, `CommandExecutor`, `ProcessCommandExecutor`, and the helper functions `render_command` and `render_command_error` from `monochange::package_publish` into `monochange_publish`. This continues the Phase 2 crate boundary cleanup by ensuring the publish crate owns all command execution infrastructure used during publishing.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #409](https://github.com/monochange/monochange/pull/409) _Introduced in:_ [`f08c48b`](https://github.com/monochange/monochange/commit/f08c48be727539436ba7d839fa93a6ca5df7d0bb)

#### Move registry infrastructure from `monochange` into `monochange_publish`

This change relocates registry-facing utilities so the publish crate owns all HTTP transport and registry endpoint concerns:

- `RegistryEndpoints` – configurable registry base URLs with environment fallbacks
- `registry_client()` – shared blocking HTTP client with monochange user-agent
- `package_can_be_published()` – predicate that checks publish enablement and state
- `filter_pending_publish_requests()` – filters out already-published or external entries
- `filter_pending_publish_requests_with_transport()` – same with transport-aware checks
- `registry_version_exists()` – ecosystem-aware version existence probe
- `crates_io_version_exists()` – Crates.io API version lookup with index fallback
- `crates_io_index_version_exists()` – sparse-index version existence check
- `crates_io_index_entry_path()` – sparse-index path computation for a crate name

`monochange` now delegates to these via `monochange_publish` imports rather than owning the implementation. `publish_rate_limits.rs` also imports them from `monochange_publish` instead of `package_publish` directly.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #404](https://github.com/monochange/monochange/pull/404) _Introduced in:_ [`7b09570`](https://github.com/monochange/monochange/commit/7b09570cd076b97c49210b6f3e1aeb33fb7eaf68)

#### Move resume and dependency ordering to monochange_publish

Move resume/artifact logic (`read_publish_report_artifact`, `write_publish_report_artifact`, `ensure_publish_report_succeeded`, `resume_publish_requests`, `merge_publish_resume_report`) and dependency ordering (`order_release_requests_by_publish_dependencies`, `render_publish_dependency_cycle`) from `monochange` into `monochange_publish`.

This continues the Phase 2 crate boundary audit by removing more publish-orchestration helpers from the top-level `monochange` crate into the dedicated `monochange_publish` crate where they belong.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #412](https://github.com/monochange/monochange/pull/412) _Introduced in:_ [`86cbd66`](https://github.com/monochange/monochange/commit/86cbd668fbbd1ce20154a7b3102eed18e26209a8)

### 🐛 Fixed

#### Move Cargo publish readiness blockers into monochange_cargo

Move `cargo_publish_readiness_blockers` and workspace package table helpers (`read_workspace_package_table`, `maybe_read_workspace_manifest_contents`, `parse_workspace_manifest_value`, `extract_workspace_package_table`) from the top-level `monochange` crate into `monochange_cargo`.

Also fixes a clippy `indexing_slicing` lint in `monochange_publish` that was introduced by the previous resume/dependency-ordering extraction.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #413](https://github.com/monochange/monochange/pull/413) _Introduced in:_ [`904ba37`](https://github.com/monochange/monochange/commit/904ba37962c1fb2db7af87ebfa2ef80230c780a5)

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
