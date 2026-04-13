# Rust Best Practices Improvements

This document tracks 21 improvements identified during a Rust best practices review.

## Priority Order

| #  | Issue                                                                                            | Category         | Effort | Status |
| -- | ------------------------------------------------------------------------------------------------ | ---------------- | ------ | ------ |
| 1  | Duplicate `"context"` in `SUPPORTED_CHANGE_TEMPLATE_VARIABLES` and changelog template rendering  | Bug              | Low    |        |
| 2  | Add `#[non_exhaustive]` to public enums (`Ecosystem`, `BumpSeverity`, `CliStepDefinition`, etc.) | API Design       | Low    |        |
| 3  | Add `#[must_use]` to `Result`-returning public functions                                         | API Design       | Low    |        |
| 4  | Fix `parse_selected_bump` wildcard mapping unknown input to `Patch`                              | Correctness      | Low    |        |
| 5  | Remove duplicate `_with_context` / non-context parsing functions in config crate                 | Code Duplication | Medium |        |
| 6  | Restructure `MonochangeError` with typed variants (preserve source errors)                       | Error Handling   | High   |        |
| 7  | Add `MonochangeError::Interactive` variant for user-cancellation errors                          | Error Handling   | Medium |        |
| 8  | Eliminate `SourceDiagnostic` waste allocation in config (don't build Report just to drop it)     | Memory           | Medium |        |
| 9  | Implement `Hash`/`Eq` on `VersionedFileDefinition`, use `HashSet` for dedup                      | Memory           | Medium |        |
| 10 | Extract `monochange_hosting` shared crate from GitLab/Gitea duplication                          | Code Duplication | High   |        |
| 11 | Break up giant functions (`cli_runtime`, `config`, `workspace_ops`)                              | Maintainability  | Medium |        |
| 12 | Replace `RawProvider*Settings` with serde defaults on core types                                 | API Design       | Medium |        |
| 13 | Add feature flags for ecosystem adapters (cargo, npm, deno, dart)                                | Build            | High   |        |
| 14 | Add interactive wizard tests (extract prompt logic into injectable trait)                        | Testing          | Medium |        |
| 15 | Move `#[cfg(test)]` helpers from `workspace_ops.rs` to `test_helpers` crate                      | Testing          | Low    |        |
| 16 | Remove empty `get_packages.rs` and `get_dependents_graph.rs` stubs in cargo crate                | Code Quality     | Low    |        |
| 17 | Document `leak_string()` in `cli.rs` as intentional for CLI args                                 | Documentation    | Low    |        |
| 18 | Fix `VersionedFileUpdateContext` mixed ownership model                                           | Ownership        | Medium |        |
| 19 | Scope `#[allow(unused_assignments)]` to specific function in config                              | Linting          | Low    |        |
| 20 | Feature flags for ecosystem adapters (same as #13)                                               | Build            | High   |        |
| 21 | (Merged with #13)                                                                                |                  |        |        |

## Notes

- Issues #13 and #21 are the same (feature flags for ecosystem adapters).
- Issue #6 (restructure MonochangeError) is the highest-impact but also highest-effort change. It touches every crate.
