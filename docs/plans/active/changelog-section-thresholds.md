# Changelog section thresholds

## Problem

`monochange` currently renders every configured `changelog.sections` entry into markdown changelogs once that section has entries. There is no way to automatically collapse low-priority sections or suppress them entirely.

## Scope

- add workspace-level `[changelog.section_thresholds]`
- support `collapse` and `ignored` thresholds based on section priority
- collapse matching sections into markdown `<details>` blocks
- omit ignored sections entirely from rendered changelog output
- preserve existing behavior by default
- update config docs/comments and tests

## Non-goals

- per-package threshold overrides
- changing how changeset types map to sections
- changing release-note entry templates

## Affected files

- `crates/monochange_core/src/lib.rs`
- `crates/monochange/src/changelog.rs`
- `crates/monochange_config/src/lib.rs`
- `crates/monochange_core/src/__tests.rs`
- `crates/monochange/src/changelog.rs` tests
- `monochange.toml`
- `crates/monochange/src/monochange.toml.template`

## Plan

1. Extend changelog config models with section thresholds and safe defaults.
2. Apply thresholds while building rendered release-note sections.
3. Render collapsed sections as markdown `<details>` blocks.
4. Add validation/tests for threshold behavior and rendering.
5. Update config comments/docs.

## Validation

- `devenv shell cargo test -q -p monochange_core -p monochange_config -p monochange changelog`
- `devenv shell cargo test -q -p monochange_core -p monochange_config`
- `devenv shell mc validate`
