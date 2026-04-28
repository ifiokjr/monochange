# Python ecosystem refresh

## Problem statement

PR #152 added Python ecosystem support, but it was created against an older monochange architecture and no longer applied cleanly to `main`.

## Scope

- rebase the PR #152 work onto current `main` in an isolated worktree
- preserve Python package discovery for uv workspaces, Poetry projects, and standalone `pyproject.toml` projects
- integrate Python ecosystem configuration into current core/config/CLI/release-update paths
- refresh documentation so Python appears alongside Cargo, npm, Deno, and Dart / Flutter
- validate the rebased branch with targeted tests and repository quality checks

## Non-goals

- redesigning Python packaging semantics beyond the original PR scope
- adding registry publishing to PyPI in this refresh
- changing unrelated ecosystem behavior except where required by current architecture

## Affected files

- `crates/monochange_python/**`
- `crates/monochange_core/src/lib.rs`
- `crates/monochange_config/src/lib.rs`
- `crates/monochange/src/versioned_files.rs`
- `crates/monochange/src/workspace_ops.rs`
- `crates/monochange/src/monochange.init.toml`
- `.templates/**`
- `docs/src/**`
- `fixtures/tests/python/**`

## Plan

- [x] create isolated worktree on `feat/python-ecosystem-refresh`
- [x] inspect PR #152 and identify stale integration points
- [x] rebase original PR branch onto current `main`
- [x] resolve architecture conflicts for workspace discovery, versioned files, config normalization, and docs
- [x] run targeted compile check for `monochange` with Python enabled
- [x] run formatter (`cargo fmt --all`)
- [x] run targeted Python adapter tests
- [x] run repository lint/test validation through the pre-push hook
- [x] run workspace config validation (`mc validate`)
- [x] update PR #152 branch
- [x] monitor GitHub checks
- [x] fix changeset coverage for packages reported by CI check
- [x] add targeted patch-coverage tests for Python/versioned-file/config/core/workspace integration

## Validation

- [x] `cargo check -p monochange --features python`
- [x] `cargo test -p monochange_python`
- [x] `cargo test -p monochange_config`
- [x] `cargo test -p monochange --features python --lib`
- [x] `cargo fmt --all`
- [x] `devenv shell lint:all` phases through docs/lint/deny/validation completed before publish dry-run timeout in the interactive run
- [x] pre-push `lint and test` hook completed successfully before force-push
- [x] `mc validate`
- [x] `cargo fmt --all --check`
- [x] `cargo test -p monochange_config load_workspace_configuration_inherits_python_ecosystem_defaults`
- [x] `cargo test -p monochange_config load_workspace_configuration_rejects_python_versioned_file_glob_unsupported_files`
- [x] `cargo test -p monochange_core python_package_type_and_ecosystem_defaults_are_canonical`
- [x] `cargo test -p monochange --features python apply_versioned_file_definition_updates_python_manifest_and_lock_variants`
- [x] `cargo check -p monochange --all-features --tests`
- [x] `cargo test -p monochange_python` after additional patch-coverage tests
- [x] `cargo test -p monochange --features python read_cached_document_reports_python_error_paths --lib`
- [x] `cargo test -p monochange --features python apply_versioned_file_definition_reports_python_error_paths --lib`
- [x] `cargo test -p monochange --features python inferred_lockfile_ecosystem_type_maps_python_when_commands_are_not_configured --lib`
- [x] `cargo test -p monochange --features python render_annotated_init_config_includes_python_package_type --lib`

## Notes

- Worktree: `/Users/ifiokjr/.pi/agent/worktrees/root/root/Users/ifiokjr/Developer/projects/monochange/monochange/worktrees/feat-python-ecosystem-refresh`
- Source PR: #152 (`worktree-feat-python-ecosystem`)
- Refresh branch: `feat/python-ecosystem-refresh`
- PR branch updated: `worktree-feat-python-ecosystem`
- Python lockfiles are handled through inferred commands (`uv lock`, `poetry lock --no-update`) instead of direct lockfile mutation.
- CI follow-up found missing changeset package entries and patch coverage gaps; this pass adds targeted tests plus Python manifest-name validation for configured Python packages.
