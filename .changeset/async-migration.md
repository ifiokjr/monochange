---
monochange: major
monochange_analysis: major
monochange_config: major
monochange_core: major
monochange_forgejo: major
monochange_gitea: major
monochange_github: major
monochange_gitlab: major
monochange_hosting: major
monochange_publish: major
---

# Async migration: Tokio async runtime end-to-end

This is a **breaking change** that migrates the entire CLI and workspace from synchronous I/O to Tokio async. All public APIs that previously returned `Result<T, E>` directly now return `impl Future<Output = Result<T, E>>` and must be `.await`ed.

## Breaking changes — Public API signatures

### `monochange_core`

- **`git_command`** now returns `std::process::Command` (unchanged for inspection compatibility), but all execution functions (`git_checkout`, `git_clone`, `git_commit`, `git_push`, `git_fetch`, `git_merge`, `git_default_branch_name`, `git_rebase`, `git_create_branch`, `git_delete_branch`, `git_tag_create`, `git_read_tree`, `git_status`, etc.) are now `async fn` returning `impl Future`. Callers must `.await` these.
- **`DiscoverOptions`**, **`discover_workspace`**, and all git helper functions are now async.

### `monochange_hosting`

- **All provider trait methods** (`verify_release_branch`, `publish_release`, `retarget_release_tags`, `create_release_pull_request`, `update_release_pull_request`, `find_existing_pull_request`, `find_existing_merge_request`, `find_existing_release`, `enrich_changeset_context`, `default_branch_name`, `no_identity`) are now `async fn`. Implementors must update their trait implementations.
- Provider lookup functions (`get_hosting_provider`, `get_provider`) remain sync.

### `monochange_github`, `monochange_gitea`, `monochange_gitlab`, `monochange_forgejo`

- **All public sync entry points** that previously used `Runtime::new().block_on()` internally are now either `async fn` or delegate to `block_on_in_context`. Public `async fn` signatures include:
  - `publish_release`, `find_existing_pull_request`, `find_existing_release`, `default_branch_name`, `create_change_request`, `verify_release_branch`, `retarget_release_tags`, `enrich_changeset_context`, `no_identity`
- **`reqwest::blocking::Client`** replaced with async `reqwest::Client` throughout.
- **`github_runtime()` / `gitea_runtime()` / etc.** removed from public API (only available as `#[cfg(test)]` helpers).

### `monochange_publish`

- **`filter_pending_publish_requests`**, **`filter_pending_publish_requests_with_transport`**, **`registry_version_exists`**, **`crates_io_version_exists`**, **`crates_io_index_version_exists`** are now `async fn`. Callers must `.await`.
- **`execute_publish_requests_with_process`** and **`run_placeholder_publish`** remain sync (they bridge internally via `block_on_in_context`).
- **`reqwest::blocking::Client`** replaced with async `reqwest::Client` throughout.
- **`registry_client()`** is now a sync function returning `MonochangeResult<Client>` (no longer async).

### `monochange`

- **`cli_runtime::block_on_in_context`** is a new `pub(crate)` helper that safely handles both multi-threaded and current-thread Tokio runtimes, eliminating "Cannot start a runtime from within a runtime" panics.
- **`publish_source_change_request`**, **`publish_readiness::publish_plan_package_filter_from_readiness_artifact`**, **`plan_publish_rate_limits`**, and all async CLI step handlers are now `async fn`.
- **`run_publish_packages`**, **`run_publish_packages_with_resume`** remain async.
- **`run_placeholder_publish`** and **`execute_publish_requests_with_process`** remain sync, bridging to async internally.
- **Test files** converted from `#[test]` to `#[tokio::test(flavor = "multi_thread")]` where they call async code.

## Migration guide

1. Any code calling `monochange_core::git::*` functions must `.await` the result.
2. Any code using `monochange_hosting` provider traits must implement `async fn` methods.
3. Any code calling `monochange_publish::*` async functions must be in an async context.
4. Tests that call async code must use `#[tokio::test(flavor = "multi_thread")]`.
5. Replace `reqwest::blocking::Client` with `reqwest::Client` (async) in all custom code.
6. When bridging from sync to async, use `block_on_in_context` which handles multi-threaded runtimes via `block_in_place` and current-thread runtimes via `handle.block_on()`.

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
