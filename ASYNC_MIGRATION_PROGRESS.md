# Async Migration Progress

## Status

In Progress

## Foundation (Completed)
- [x] Workspace tokio features updated to include "process", "fs", "macros", "rt-multi-thread"
- [x] monochange_core/src/git.rs functions are now async
- [x] monochange entrypoint is async (#[tokio::main], run_from_env, run_with_args)

## Phase 1: monochange CLI crate
- [ ] cli_runtime.rs
- [ ] workspace_ops.rs
- [ ] skill.rs
- [ ] release_record.rs
- [ ] Other source files in crates/monochange/src/

## Phase 2: Provider crates
- [ ] monochange_hosting
- [ ] monochange_github
- [ ] monochange_gitlab
- [ ] monochange_gitea
- [ ] monochange_forgejo
- [ ] monochange_publish

## Phase 3: Ecosystem crates
- [ ] monochange_cargo
- [ ] monochange_npm
- [ ] monochange_python
- [ ] monochange_dart
- [ ] monochange_deno
- [ ] monochange_go
- [ ] monochange_ecmascript

## Phase 4: Internal crates
- [ ] monochange_analysis
- [ ] monochange_graph
- [ ] monochange_config
- [ ] monochange_semver
- [ ] monochange_lint
- [ ] monochange_linting
- [ ] monochange_telemetry
- [ ] monochange_test_helpers

## Notes
- monochange_hosting is blocking compilation of downstream crates
- Need to remove reqwest blocking feature from workspace Cargo.toml
