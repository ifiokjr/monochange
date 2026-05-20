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

### 🐛 Fixed

#### add missing crate metadata and align READMEs with badge template

- Add `keywords` to `monochange_analysis`, `monochange_lint`, and `monochange_linting`
- Add `authors`, `categories`, `homepage`, `readme`, `rust-version`, and `keywords` to `monochange_test_helpers`
- Update `monochange_lint`, `monochange_linting`, and `monochange_test_helpers` READMEs to use the badge-row template consistent with other published crates

No API changes. crates.io metadata and documentation only.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #512](https://github.com/monochange/monochange/pull/512) _Introduced in:_ [`f7bc995`](https://github.com/monochange/monochange/commit/f7bc9950aaa58983c2d9b3d53ec1a942debc263d)

## [0.5.1](https://github.com/monochange/monochange/releases/tag/v0.5.1) (2026-05-15)

### 📝 Changed

- No package-specific changes were recorded; `monochange_analysis` was updated to 0.5.1 as part of group `main`.

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-14)

### 🚀 Feature

#### Publish all configured packages

Add a `--all` flag to the PublishPackages CLI step so migration workflows can publish every configured package, including packages that were not part of the prepared release record.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #461](https://github.com/monochange/monochange/pull/461) _Introduced in:_ [`3d956cd`](https://github.com/monochange/monochange/commit/3d956cd3e34747e088add98fe0358251f388782f) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

## [0.4.2](https://github.com/monochange/monochange/releases/tag/v0.4.2) (2026-05-10)

### 🚀 Feature

#### Order publish plans by dependencies

Order publish plans by workspace dependencies before applying registry rate-limit windows, and run CI publishing as one dependency-ordered publish operation.

This keeps dependent packages from publishing before their internal dependencies are available and adds realistic fixture coverage for non-alphabetical cargo dependency graphs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e) _Last updated in:_ [`2392845`](https://github.com/monochange/monochange/commit/2392845ec29289e3f219aca20ac343cf79ee965e)

## [0.4.1](https://github.com/monochange/monochange/releases/tag/v0.4.1) (2026-05-10)

### 📝 Changed

- No package-specific changes were recorded; `monochange_analysis` was updated to 0.4.1 as part of group `main`.

## [0.4.0](https://github.com/monochange/monochange/releases/tag/v0.4.0) (2026-05-09)

### 🐛 Fixed

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

- No package-specific changes were recorded; `monochange_analysis` was updated to 0.3.2 as part of group `main`.

## [0.3.1](https://github.com/monochange/monochange/releases/tag/v0.3.1) (2026-05-05)

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

### 🧪 Testing

#### Add mutation regression coverage

Add cargo-mutants regression coverage for `monochange_analysis`, `monochange_config`, `monochange_core`, and `monochange_hosting`.

- `monochange_analysis`: add tests for `latest_workspace_release_tag` to ignore namespaced tags, `snapshot_files_from_working_tree` and `read_text_file_from_git_object` for medium/exact-size files, `detect_raw_pr_environment` to reject non-PR events across CI providers, `default_branch_name` with origin HEAD symbolic ref, `get_merge_base` returning actual merge base, and `ChangeFrame::changed_files` distinguishing working directory from staged-only.
- `monochange_config`: add fixture-backed tests and proptests for ecosystem versioned-file inheritance, explicit group bump inference, and changelog validation defaults to close mutation gaps in release-planning configuration loading.
- `monochange_core`: add fixture-backed tests for discovery filtering around parent `.git` directories outside the workspace root and block-comment stripping edge cases in JSON helper logic.
- `monochange_hosting`: add HTTP-mock tests for error paths in `get_json`, `get_optional_json`, `post_json`, `put_json`, and `patch_json` to kill mutants on status-code checks. Update `Cargo.toml` to include `src/*.rs` in the distribution manifest so `__tests.rs` builds correctly with `httpmock` as a dev-dependency.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #277](https://github.com/monochange/monochange/pull/277) _Introduced in:_ [`4c411d9`](https://github.com/monochange/monochange/commit/4c411d9efe84aaefbe2231ac16e4065249fc2a06) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

### 📝 Changed

#### Update repository URLs

Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

### 🚀 Feature

#### add semantic change analysis crate

Introduces a new `monochange_analysis` crate that provides intelligent, artifact-aware changeset generation for the monochange ecosystem.

**What it does:**

The crate analyzes git diffs and suggests granular changesets based on the type of code being changed:

- **Libraries**: Detects public API changes (new functions, types, traits)
- **Applications**: Identifies UI components, routes, and state changes
- **CLI tools**: Extracts command and flag modifications

**Key features:**

- **Change frame detection**: Automatically detects what to analyze based on git state (working directory, branches, PRs, CI/CD environments)
- **Artifact type classification**: Determines if a package is a library, application, CLI tool, or mixed artifact
- **Semantic extraction**: Three levels of analysis - basic (file-level), signature (function/type signatures), and semantic (full AST)
- **Adaptive grouping**: Configurable thresholds for grouping related changes vs. creating separate changesets

**Example usage:**

```rust
use monochange_analysis::{
    analyze_changes,
    ChangeFrame,
    AnalysisConfig,
    DetectionLevel,
};

// Auto-detect the change frame
let frame = ChangeFrame::detect(Path::new("."))?;

let config = AnalysisConfig {
    detection_level: DetectionLevel::Signature,
    ..Default::default()
};

let analysis = analyze_changes(Path::new("."), &frame, &config)?;

// Get suggested changesets per package
for (package_id, pkg) in &analysis.package_changes {
    for cs in &pkg.suggested_changesets {
        println!("{}: {}", package_id, cs.summary);
    }
}
```

**Supported CI/CD environments:**

- GitHub Actions
- GitLab CI
- CircleCI
- Travis CI
- Azure Pipelines
- Buildkite

This crate is the foundation for the new `mc analyze` command and MCP tools that help agents generate better changesets automatically.

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

#### improve npm and Deno semantic analysis with parser-backed JS/TS export extraction

`monochange_analyze_changes` and `monochange_validate_changeset` now use parser-backed JavaScript and TypeScript export extraction for npm and Deno packages instead of relying primarily on line-based export scanning.

This improves semantic diff accuracy for cases such as:

- multiline named export blocks
- namespace re-exports like `export * as toolkit from "./toolkit"`
- anonymous default exports
- more complex TypeScript and module export syntax

The MCP output shape stays the same, but the semantic evidence for npm and Deno packages is now more robust and closer to the actual module structure.

This work also extracts the shared JavaScript and TypeScript export-analysis logic into the new `monochange_ecmascript` crate, so npm and Deno keep their ecosystem-specific manifest analysis while reusing one parser-backed module analyzer.

The monochange skill documentation now also teaches the new-package rule: the first changeset for a newly introduced published package or crate should use a `major` bump for that new package entry.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #250](https://github.com/monochange/monochange/pull/250) _Introduced in:_ [`0dd8460`](https://github.com/monochange/monochange/commit/0dd846060614b2de9d3b2dfb5c1337075774b167) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Related issues:_ [#247](https://github.com/monochange/monochange/issues/247), [#249](https://github.com/monochange/monochange/issues/249)

#### `monochange_analysis` can now return release-aware semantic analysis across three explicit frames:

- `release -> main`
- `main -> head`
- `release -> head`

This adds a first multi-frame API surface for issue #249, including explicit ref-based entry points plus automatic baseline resolution that uses the latest workspace-style release tag and the detected default branch.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #252](https://github.com/monochange/monochange/pull/252) _Introduced in:_ [`d1dce9d`](https://github.com/monochange/monochange/commit/d1dce9d880a1739253f5dccc3cd7cc73431b2b41) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Related issues:_ [#249](https://github.com/monochange/monochange/issues/249)

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

### 🧪 Testing

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
