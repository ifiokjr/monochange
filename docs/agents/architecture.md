# Architecture boundaries

## Core vs adapters

- `crates/monochange_core`, `crates/monochange_graph`, and `crates/monochange_semver` define shared domain models, plans, and capability contracts.
- Ecosystem crates (`monochange_cargo`, `monochange_npm`, `monochange_deno`, `monochange_dart`) own ecosystem-specific discovery, validation, manifest updates, and lockfile updates.
- Source crates (`monochange_github`, `monochange_gitlab`, `monochange_gitea`) own provider-specific capability declarations, validation, request shaping, and publishing.
- `crates/monochange` orchestrates commands by dispatching to adapters. It should not own adapter implementation details.
- `crates/monochange_config` parses and normalizes configuration. When behavior depends on a specific ecosystem or provider, validation should be delegated to that adapter crate.

## Review checklist

Before merging architecture-sensitive work, check:

1. Did shared crates gain provider- or ecosystem-specific file names, payload shapes, or support matrices?
2. Did `crates/monochange` add adapter-specific mutation logic instead of dispatching to an adapter?
3. Did `crates/monochange_config` add implementation-specific validation that belongs in an adapter crate?
4. Did the change add fixtures and integration tests that exercise the adapter boundary from the public CLI/API surface?
5. Is patch coverage for executable changed lines still at 100%?

## Preferred direction

- Introduce new capabilities in core first.
- Implement those capabilities in the relevant adapter crate.
- Reject unsupported configuration shapes at the parse boundary rather than leaking compatibility concerns into the shared domain model.

## Explicit dispatch points

- Provider dispatch in `crates/monochange` should stay concentrated in small orchestration modules such as `src/hosted_sources.rs`, `src/release_artifacts.rs`, and narrowly-scoped release helpers.
- Ecosystem dispatch in `crates/monochange` should stay concentrated in shared orchestration helpers such as `src/versioned_files.rs` until the behavior can move behind adapter contracts.
- `crates/monochange_config` may validate configuration against adapter capabilities, but it should not reimplement adapter behavior.
- New direct `SourceProvider` or `EcosystemType` branching outside those explicit files is a review blocker unless the exception is documented in `ARCHITECTURE.md` and covered by tests.

## Mechanical enforcement

- `docs:check` verifies that agent-facing documentation stays aligned with the exported MCP surface and repo map.
- `lint:architecture` verifies that new provider/ecosystem dispatch points do not quietly spread beyond the documented allowlist.
- When either check needs a new exception, update the docs and the check in the same change so architecture drift stays reviewable.
