# Implementation Plan: Package/Group Configuration API

**Branch**: `spike/group-config-api` | **Date**: 2026-03-28 | **Design**: [`specs/group-config-api-design.md`](./group-config-api-design.md)

## Summary

Replace the current `[[version_groups]]` and `[[package_overrides]]` configuration model with explicit `[package.<id>]` and `[group.<id>]` declarations, add `mc check` as the dedicated validation command, and move release preparation and changeset resolution onto configured package/group ids rather than implicit package-name/path matching.

This implementation should preserve the existing release-planning and workflow-driven release preparation capabilities while making configuration stricter, more expressive, and easier to validate. The main technical shifts are:

- introduce explicit config-owned identifiers for packages and groups
- add richer config models for changelogs, extra versioned files, and release identity settings
- make grouped packages inherit outward release identity from their parent group
- validate config and `.changeset/*.md` files through a new `mc check` entrypoint
- adopt `miette` for source-aware diagnostics with actionable remediation hints

## Objectives

1. Replace `version_groups` with `[group.<id>]`.
2. Replace `package_overrides` with `[package.<id>]`.
3. Support package ids and group ids in changeset frontmatter.
4. Reject changesets that reference both a group and one of its members.
5. Add `versioned_files` to both packages and groups.
6. Add `tag`, `release`, and `version_format` to both packages and groups.
7. Enforce exactly zero or one `version_format = "primary"` across all config items.
8. Add `mc check --root .` to validate config and changesets with `miette` diagnostics.
9. Update release preparation so grouped packages still apply package changelogs and package `versioned_files`, while group release identity takes precedence.

## Non-Goals

- broad plugin-style file update rules beyond the initial `versioned_files` model
- automatic backward compatibility with undeclared package references in groups
- preserving path-based changeset keys as a supported contract under the new config model
- non-Cargo manifest mutation beyond current adapter support in this slice
- GitHub bot automation or remote publishing execution in this slice

## Technical Context

**Language**: Rust workspace **Key crates involved**:

- `crates/monochange`
- `crates/monochange_core`
- `crates/monochange_config`
- `crates/monochange_cargo`

**New dependency direction**:

- add `miette` to config/CLI layers for diagnostics
- keep `thiserror` for core error representation where useful

**Primary surfaces to change**:

- `monochange.toml` parsing and validation
- changeset parsing and target resolution
- workflow release preparation behavior
- CLI command dispatch (`mc check`)
- docs/contracts/examples/tests

## Proposed Architecture Changes

## 1. Core data model changes

Replace the current config model in `monochange_core` with explicit package/group declarations.

### New/updated types

- `PackageType`
  - `cargo | npm | deno | dart | flutter`
- `VersionFormat`
  - `namespaced | primary`
- `VersionedFileDefinition`
  - string shorthand path
  - structured `{ path, dependency }`
- `PackageDefinition`
  - `id`
  - `path`
  - `type`
  - `changelog`
  - `versioned_files`
  - `tag`
  - `release`
  - `version_format`
- `GroupDefinition`
  - `id`
  - `packages`
  - `changelog`
  - `versioned_files`
  - `tag`
  - `release`
  - `version_format`
- `WorkspaceConfiguration`
  - replace `version_groups`
  - replace `package_overrides`
  - add `packages: Vec<PackageDefinition>`
  - add `groups: Vec<GroupDefinition>`

### Derived/effective state

Add an internal computed view for implementation code, for example:

- `EffectivePackageReleaseSettings`
- `EffectiveGroupMembership`

This lets runtime code ask:

- which group a package belongs to
- what release identity settings actually apply
- which changelogs and versioned files must be written

## 2. Config parser and validator changes

`monochange_config` should become the canonical validator for:

- config schema
- package/group relationships
- changeset target resolution

### Parsing changes

Add support for TOML tables of the form:

- `[package.<id>]`
- `[group.<id>]`

This likely means moving away from plain `Vec<T>` deserialization for these sections and instead reading TOML tables/maps, then normalizing them into strongly typed definitions.

### Validation responsibilities

Config validation should enforce:

- package ids are unique
- group ids are unique
- package and group ids share one namespace
- every package declares `path`
- every package declares `type`, unless `[defaults].package_type` is set
- package path exists
- expected manifest for package `type` exists
- package paths are unique
- every group member references an already declared package id
- no package belongs to more than one group
- only one config item may use `version_format = "primary"`
- `versioned_files` entries are structurally valid

### Diagnostics

Adopt `miette` and emit diagnostics with:

- source file context
- spans/labels
- `help:` suggestions

Use `NamedSource` and `SourceSpan` so errors point into:

- `monochange.toml`
- `.changeset/*.md`

## 3. Changeset resolution changes

Move changeset resolution to configured ids only.

### New rules

Frontmatter keys may refer to:

- package ids
- group ids

Frontmatter keys may not rely on:

- package directory paths
- manifest-relative paths
- raw discovered package names unless they are also configured ids

### Validation rules

Reject changesets when:

- key does not resolve to any configured package/group id
- a file references both a group and any of its member packages
- bump value is not `patch`, `minor`, or `major`

### Runtime output

The resulting `ChangeSignal` pipeline may still expand group targets into per-package signals internally, but source validation must happen against configured ids first.

## 4. Workspace discovery integration

Discovery remains ecosystem-driven, but config becomes the authority for which discovered packages are managed and how they are identified.

### Recommended integration rule

- discovery still finds native packages
- config validation cross-checks declared package `path` + `type` against discovered manifests
- runtime planning/release steps operate on declared package ids mapped onto discovered packages

This avoids changing the discovery adapters into config-first scanners while still allowing config to own the public identity model.

## 5. Release preparation changes

Update workflow-driven release preparation so grouped packages inherit version identity from the parent group while still applying package-level file updates.

### Group precedence

If package `P` belongs to group `G`:

- use `G.tag`
- use `G.release`
- use `G.version_format`
- ignore `P.tag/release/version_format` for outward release identity

### Still apply package-owned outputs

Even when grouped, package `P` still applies:

- native manifest updates
- package changelog
- package `versioned_files`

### Group-owned outputs

When group `G` changes:

- write group changelog if configured
- apply group `versioned_files`

### Tag/release planning representation

Add or extend release output models so planned release artifacts can later distinguish:

- package release artifacts
- group release artifacts
- chosen tag format
- chosen GitHub release target

Even if actual tag/release creation is deferred, the model should carry enough information for future implementation.

## 6. CLI changes

Add a top-level command:

```bash
mc check --root .
monochange check --root .
```

### Behavior

- load and validate `monochange.toml`
- inspect `.changeset/*.md`
- report success when both config and changesets are valid
- emit `miette` diagnostics on failure

### Optional future extensions

Not required in the first implementation slice:

- `--format json`
- `--strict`
- separate config-only or changeset-only modes

## Phased Implementation Plan

## Phase 0 — Contract lock and fixture design

1. Convert the design note into concrete parser/runtime acceptance criteria.
2. Create representative fixture configs for:
   - simple ungrouped packages
   - grouped Cargo workspace packages
   - cross-ecosystem id collisions (`npm:@scope/name` vs `deno:@scope/name`)
   - invalid duplicate primary version format
   - invalid unknown group member
   - invalid group/member overlap in changeset
3. Decide exact manifest expectations by package type:
   - cargo → `Cargo.toml`
   - npm → `package.json`
   - deno → supported Deno manifest (initially current supported default)
   - dart/flutter → `pubspec.yaml`

## Phase 1 — Failing tests first

### `monochange_config` tests

Add failing tests for:

- parsing `[package.<id>]`
- parsing `[group.<id>]`
- namespace collision between package and group ids
- package missing `path`
- package missing `type`
- unsupported package `type`
- missing expected manifest for declared type
- duplicate package paths
- unknown package referenced in group
- package belonging to multiple groups
- duplicate `version_format = "primary"`
- valid `versioned_files` forms
- invalid `versioned_files` forms

### `monochange` CLI tests

Add failing tests for:

- `mc check --root .` success path
- `mc check` fails on invalid config with readable diagnostic text
- `mc check` fails on invalid changeset target
- `mc check` fails on group/member overlap in one changeset
- `mc check` works through both `mc` and `monochange`

### release-preparation tests

Add failing tests for:

- grouped package inherits group release identity
- grouped package still applies package changelog
- grouped package still applies package `versioned_files`
- group changelog plus package changelog both write when configured
- changeset targeting a group expands correctly
- changeset targeting both group and member errors before mutation

## Phase 2 — Core model refactor

1. Add new config types to `monochange_core`.
2. Remove or deprecate old config-only types:
   - `VersionGroupDefinition`
   - `PackageOverride`
3. Introduce helper enums/structs:
   - `PackageType`
   - `VersionFormat`
   - `VersionedFileDefinition`
4. Update `WorkspaceConfiguration` to store package/group declarations.

## Phase 3 — Parser and validator rewrite

1. Rewrite config deserialization in `monochange_config` for table-based package/group sections.
2. Add source-aware validation pipeline returning `miette` diagnostics.
3. Add a config normalization stage that produces:
   - declared packages by id
   - groups by id
   - package → group membership map
   - effective version-format ownership info
4. Preserve existing defaults handling for `[defaults]`, workflows, and ecosystem sections.

## Phase 4 — Changeset resolution update

1. Replace reference resolution logic so changesets resolve only against declared ids.
2. Add group/member overlap detection during changeset load.
3. Expand group-targeted changesets into package-level change signals for planning.
4. Ensure diagnostics include the changeset file path and frontmatter source span.

## Phase 5 — `mc check` implementation

1. Add CLI command wiring in `crates/monochange/src/lib.rs`.
2. Implement a reusable validation function, for example:
   - `check_workspace(root: &Path) -> MonochangeResult<CheckReport>`
3. Run config validation first, then changeset validation.
4. Render concise success output and `miette` failures.

## Phase 6 — Release preparation integration

1. Refactor release-preparation code to derive effective release identity from group membership.
2. Ensure package changelogs still write for grouped packages.
3. Apply package `versioned_files` and group `versioned_files` in a stable order.
4. Add model hooks for group/package tag + release planning, even if actual remote publishing/tagging remains deferred.

## Phase 7 — Docs and migration guidance

Update:

- `readme.md`
- `docs/src/guide/04-configuration.md`
- `docs/src/guide/05-version-groups.md` (likely rename or repurpose)
- `docs/src/guide/06-release-planning.md`
- quickstart/spec/contracts

Add migration guidance from:

- `[[version_groups]]` → `[group.<id>]`
- `[[package_overrides]]` → `[package.<id>]`

Document:

- configured ids vs native names
- `mc check`
- group changesets vs package changesets
- grouped package precedence rules

## Phase 8 — Full validation and rollout

Run:

- `cargo test --workspace --all-features`
- `devenv shell -- lint:all`
- `devenv shell -- build:book`

Then dogfood on the repository’s own `monochange.toml` by migrating it to the new model and using `mc check` to validate it.

## File-Level Change Map

### Likely source files to update

- `crates/monochange_core/src/lib.rs`
- `crates/monochange_config/src/lib.rs`
- `crates/monochange_config/src/__tests.rs`
- `crates/monochange/src/lib.rs`
- `crates/monochange/src/__tests.rs`
- `monochange.toml`
- docs/spec files

### Likely new helper modules

Potentially split out of `monochange_config` or `monochange`:

- config validation diagnostics
- versioned-file parsing
- check command/report rendering
- effective release-identity resolution

## Risks and Mitigations

### Risk: Config migration breaks current repo behavior

Mitigation:

- implement migration incrementally behind tests
- migrate the repo config only after `mc check` is stable

### Risk: Changeset ids become harder for users to author

Mitigation:

- document ids clearly
- keep ids human-readable
- provide excellent diagnostics for unknown ids

### Risk: `miette` integration becomes invasive

Mitigation:

- keep core domain errors simple
- concentrate source-aware diagnostics in config/check layers first

### Risk: Group/package precedence gets confusing

Mitigation:

- encode precedence rules in a dedicated helper and test them directly
- document grouped-package effective behavior explicitly

## Acceptance Criteria

This feature slice is complete when:

1. `monochange.toml` can declare packages via `[package.<id>]` and groups via `[group.<id>]`.
2. `mc check --root .` validates config and `.changeset/*.md` files.
3. invalid config and changesets produce actionable `miette` diagnostics.
4. group-targeted changesets are supported.
5. group/member overlap in a single changeset is rejected.
6. grouped packages inherit release identity from the group.
7. grouped packages still apply package changelogs and package `versioned_files`.
8. package/group `version_format` supports `namespaced` and `primary`.
9. only one config item may use `version_format = "primary"`.
10. docs and migration guidance are updated to match the new API.
