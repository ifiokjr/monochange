# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.4.1](https://github.com/monochange/monochange/releases/tag/v0.4.1) (2026-05-10)

### Fixed

#### Split crate boundaries for changelog, config, and publish behavior

Move changelog rendering into `monochange_changelog`, shift publish planning and execution helpers into `monochange_publish`, and reduce direct concrete ecosystem/provider dependencies in `monochange_config`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #441](https://github.com/monochange/monochange/pull/441) _Introduced in:_ [`ae8ea56`](https://github.com/monochange/monochange/commit/ae8ea563ae95c6cc4e8d3d1acdc5303069ea44cf)

## [0.4.0](https://github.com/monochange/monochange/releases/tag/v0.4.0) (2026-05-09)

### Breaking Change

#### Extract publish support into a dedicated crate

Move the publish support surface out of the top-level `monochange` crate and into the new `monochange_publish` crate. The extracted crate now owns the publish report/request models, trusted-publishing capability detection, provider/registry capability messages, and built-in publish command builders for npm, pnpm, Cargo, Dart, Flutter, JSR, PyPI, and Go proxy releases.

This keeps `monochange` focused on orchestration while giving publish integrations a dedicated crate boundary for future registry checks, readiness logic, and provider-specific publishing workflows.

```text
monochange_publish owns reusable publish capabilities and command construction.
monochange wires those capabilities into CLI workflows and release orchestration.
```

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #397](https://github.com/monochange/monochange/pull/397) _Introduced in:_ [`fa78e4d`](https://github.com/monochange/monochange/commit/fa78e4db56fd3a6897896c6e1b1c62ea2d8e46b9) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

### Added

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #429](https://github.com/monochange/monochange/pull/429) _Introduced in:_ [`271e554`](https://github.com/monochange/monochange/commit/271e55420154265e798a0de3adf26a64faba66c8)

#### Move CommandExecutor and command rendering into monochange_publish

Extract `CommandOutput`, `CommandExecutor`, `ProcessCommandExecutor`, and the helper functions `render_command` and `render_command_error` from `monochange::package_publish` into `monochange_publish`. This continues the Phase 2 crate boundary cleanup by ensuring the publish crate owns all command execution infrastructure used during publishing.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #409](https://github.com/monochange/monochange/pull/409) _Introduced in:_ [`f08c48b`](https://github.com/monochange/monochange/commit/f08c48be727539436ba7d839fa93a6ca5df7d0bb)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #404](https://github.com/monochange/monochange/pull/404) _Introduced in:_ [`7b09570`](https://github.com/monochange/monochange/commit/7b09570cd076b97c49210b6f3e1aeb33fb7eaf68)

#### Move resume and dependency ordering to monochange_publish

Move resume/artifact logic (`read_publish_report_artifact`, `write_publish_report_artifact`, `ensure_publish_report_succeeded`, `resume_publish_requests`, `merge_publish_resume_report`) and dependency ordering (`order_release_requests_by_publish_dependencies`, `render_publish_dependency_cycle`) from `monochange` into `monochange_publish`.

This continues the Phase 2 crate boundary audit by removing more publish-orchestration helpers from the top-level `monochange` crate into the dedicated `monochange_publish` crate where they belong.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #412](https://github.com/monochange/monochange/pull/412) _Introduced in:_ [`86cbd66`](https://github.com/monochange/monochange/commit/86cbd668fbbd1ce20154a7b3102eed18e26209a8)

### Fixed

#### Move Cargo publish readiness blockers into monochange_cargo

Move `cargo_publish_readiness_blockers` and workspace package table helpers (`read_workspace_package_table`, `maybe_read_workspace_manifest_contents`, `parse_workspace_manifest_value`, `extract_workspace_package_table`) from the top-level `monochange` crate into `monochange_cargo`.

Also fixes a clippy `indexing_slicing` lint in `monochange_publish` that was introduced by the previous resume/dependency-ordering extraction.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #413](https://github.com/monochange/monochange/pull/413) _Introduced in:_ [`904ba37`](https://github.com/monochange/monochange/commit/904ba37962c1fb2db7af87ebfa2ef80230c780a5)

#### Remove grouped release member summaries

Grouped release notes no longer include generated changed or synchronized member lists, keeping the release note summary focused on the group release itself.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #395](https://github.com/monochange/monochange/pull/395) _Introduced in:_ [`2d012ff`](https://github.com/monochange/monochange/commit/2d012ff900a612f4aed6e4d7034c8c876f50aeae) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

### Testing

#### Extract inline test modules into separate files

Move all inline `#[cfg(test)] mod tests { ... }` blocks out of source files into dedicated test files. This reduces source file sizes and keeps test code in a consistent `__tests/` directory structure next to the module it tests.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #416](https://github.com/monochange/monochange/pull/416) _Introduced in:_ [`3535c88`](https://github.com/monochange/monochange/commit/3535c887c46d66db2768377cb5f01406f6e9a8b6)

#### Normalize Rust unit test file layout

Move Rust unit tests into colocated `__tests__/` directories and name each file after the module under test with a `_tests.rs` suffix.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #428](https://github.com/monochange/monochange/pull/428) _Introduced in:_ [`b61cc3e`](https://github.com/monochange/monochange/commit/b61cc3e66989fd83ffb16a31568d2f46d7075216)
