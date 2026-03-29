# Tasks: Package/Group Configuration API

## Phase 0 — Contract and fixtures

- [x] T001 Draft design note for package/group configuration API.
- [x] T002 Draft implementation plan for package/group configuration API.
- [x] T003 Add task checklist for the feature slice.
- [x] T004 Add representative config and changeset fixtures for package ids, group ids, and invalid overlap cases.

## Phase 1 — Tests first

- [x] T005 Add `monochange_core` tests for new package/group config model helpers.
- [x] T006 Add `monochange_config` tests for parsing `[package.<id>]` declarations.
- [x] T007 Add `monochange_config` tests for parsing `[group.<id>]` declarations.
- [x] T008 Add `monochange_config` tests for invalid package/group namespace collisions.
- [x] T009 Add `monochange_config` tests for invalid duplicate `version_format = "primary"`.
- [x] T010 Add `monochange_config` tests for invalid unknown group member references.
- [x] T011 Add `monochange_config` tests for invalid multi-group membership.
- [x] T012 Add `monochange_config` tests for valid and invalid `versioned_files` entries.
- [x] T013 Add `monochange_config` tests for changeset validation against configured ids.
- [x] T014 Add `monochange_config` tests for invalid changeset group/member overlap.
- [x] T015 Add `monochange` CLI tests for `mc check --root .` success and failure cases.

## Phase 2 — Core models

- [x] T016 Add `PackageType` and `VersionFormat` enums in `monochange_core`.
- [x] T017 Add `VersionedFileDefinition`, `PackageDefinition`, and `GroupDefinition` models.
- [x] T018 Replace `version_groups` and `package_overrides` in `WorkspaceConfiguration` with package/group declarations.
- [x] T019 Add helper APIs for computing effective group membership and release identity.

## Phase 3 — Config parsing and validation

- [x] T020 Rewrite `monochange.toml` parsing to support `[package.<id>]` and `[group.<id>]`.
- [x] T021 Validate required package fields: `path` and `type`.
- [x] T022 Validate expected manifest existence by package type.
- [x] T023 Validate shared namespace uniqueness across package and group ids.
- [x] T024 Validate unique package paths and single-group membership.
- [x] T025 Validate only one `version_format = "primary"` across packages/groups.
- [x] T026 Introduce `miette` diagnostics for config validation errors.

## Phase 4 — Changeset validation and resolution

- [x] T027 Update changeset parsing to resolve only configured package/group ids.
- [x] T028 Reject changesets that reference both a group and one of its member packages.
- [x] T029 Add source-aware `miette` diagnostics for changeset errors.
- [x] T030 Expand group-targeted changesets into package-level signals for planning.

## Phase 5 — CLI validation command

- [x] T031 Add `check` as a top-level built-in CLI command.
- [x] T032 Implement `check_workspace` for config + changeset validation.
- [x] T033 Render user-friendly success output for `mc check`.
- [x] T034 Wire `miette` failure output into the CLI path.

## Phase 6 — Runtime integration

- [x] T035 Derive runtime version groups from configured groups for planning.
- [x] T036 Update release preparation to use group release identity precedence.
- [x] T037 Apply package changelogs for grouped packages.
- [x] T038 Apply package and group `versioned_files` during release preparation.
- [x] T039 Carry `tag`, `release`, and `version_format` through effective release metadata.

## Phase 7 — Repo migration and docs

- [x] T040 Migrate repo `monochange.toml` to the new package/group model.
- [x] T041 Update docs and spec contracts to document package/group config and `mc check`.
- [x] T042 Add migration guidance from `version_groups`/`package_overrides` to `package`/`group`.

## Phase 8 — Validation

- [x] T043 Run targeted crate tests while iterating.
- [x] T044 Run `cargo test --workspace --all-features`.
- [x] T045 Run `devenv shell -- lint:all`.
- [x] T046 Run `devenv shell -- build:book`.
