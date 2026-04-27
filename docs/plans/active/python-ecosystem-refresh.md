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
- [ ] run repository lint/quality validation (`devenv shell lint:all`)
- [x] run workspace config validation (`mc validate`)
- [ ] update PR branch or open replacement PR after validation

## Validation

- [x] `cargo check -p monochange --features python`
- [x] `cargo test -p monochange_python`
- [x] `cargo fmt --all`
- [ ] `devenv shell lint:all` (Rust, architecture, docs, JS lint, deny, and validation phases passed; timed out during publish dry-run packaging)
- [x] `mc validate`

## Notes

- Worktree: `/Users/ifiokjr/.pi/agent/worktrees/root/root/Users/ifiokjr/Developer/projects/monochange/monochange/worktrees/feat-python-ecosystem-refresh`
- Source PR: #152 (`worktree-feat-python-ecosystem`)
- Refresh branch: `feat/python-ecosystem-refresh`
- Python lockfiles are handled through inferred commands (`uv lock`, `poetry lock --no-update`) instead of direct lockfile mutation.
