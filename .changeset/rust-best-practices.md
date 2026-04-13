---
monochange: minor
monochange_core: minor
monochange_config: minor
monochange_hosting: minor
monochange_cargo: patch
monochange_npm: patch
monochange_deno: patch
monochange_dart: patch
monochange_github: patch
monochange_gitlab: patch
monochange_gitea: patch
monochange_graph: patch
---

#### apply Rust best practices across all crates

- fix duplicate `"context"` entry in `SUPPORTED_CHANGE_TEMPLATE_VARIABLES` and changelog template rendering
- add `#[non_exhaustive]` to 11 public enums with catch-all match arms
- add `#[must_use]` to 40+ `Result`-returning public functions
- fix `parse_selected_bump` wildcard mapping unknown input to `Patch`
- add structured error variants to `MonochangeError` (`IoSource`, `Parse`, `HttpRequest`, `Interactive`, `Cancelled`)
- add `map_inquire_error()` helper for interactive cancellation
- eliminate `SourceDiagnostic`/`Report` waste allocation in config diagnostics
- implement `Hash`/`Eq` on `VersionedFileDefinition`, use `HashSet` for dedup
- extract `monochange_hosting` shared crate from gitlab/gitea duplication
- extract `execute_create_change_file_step` from cli_runtime
- replace `RawProvider*Settings` with serde defaults on core types
- add feature flags for ecosystem adapters (cargo, npm, deno, dart, github, gitlab, gitea)
- add 9 new interactive wizard unit tests
- move workspace_ops test helpers to monochange_test_helpers crate
- remove empty `get_packages.rs` and `get_dependents_graph.rs` stubs
- document `leak_string()` as intentional for CLI args
- add doc comments on `VersionedFileUpdateContext` ownership model
- scope `#[allow(unused_assignments)]` from module level to specific site
