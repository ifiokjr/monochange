# Research: Cross-Ecosystem Release Planning Foundation

## Decision 1: Use trait-based ecosystem adapters behind a shared planning core

**Decision**: Model each supported ecosystem as a focused adapter crate that implements a shared discovery and dependency interface consumed by shared planning crates.

**Rationale**:

- The feature requires equal support across Cargo, npm-family, Deno, Dart, and Flutter.
- The user explicitly wants reusable support crates rather than a monolithic direct port of knope.
- A shared adapter contract allows the CLI to aggregate all ecosystems while still letting downstream projects depend on just the adapters they need.

## Decision 2: Split shared functionality into focused domain crates

**Decision**: Add dedicated crates for configuration, graph construction, and semver-aware impact evaluation, while keeping `crates/monochange` as the CLI aggregator and `crates/monochange_core` as the shared domain surface.

**Rationale**:

- Configuration parsing, graph propagation, and semver escalation are reusable concerns.
- The constitution prefers small, independently testable modules.
- A focused crate split makes it easier to test core algorithms with fixtures that mix ecosystems.

## Decision 3: Normalize workspace discovery around manifest readers plus glob expansion

**Decision**: Implement workspace discovery as native manifest parsing per ecosystem followed by normalization into a unified package graph.

**Rationale**:

- The feature requires first-class glob handling across ecosystems.
- Normalizing after native parsing preserves ecosystem-specific behavior while still producing one graph.
- The structure supports mixed repositories containing both workspaces and standalone packages.

## Decision 4: Represent release planning as iterative graph propagation with explicit escalation hooks

**Decision**: Compute release plans by starting with direct changes, propagating default parent bumps through direct dependents, synchronizing version groups, and continuing propagation until the graph stabilizes.

**Rationale**:

- Group synchronization can introduce new releasing packages whose dependents must also be updated.
- A queue-based propagation model handles cycles safely because severities only move upward.
- Compatibility evidence remains explicit and provider-driven.

## Decision 5: Deliver Rust semver analysis first via provider abstraction

**Decision**: Introduce a compatibility provider abstraction and implement the first concrete provider for Rust using explicit `evidence` entries in change files.

**Rationale**:

- The user explicitly called out semver checking for Rust.
- An explicit first-step contract keeps the planner deterministic and testable.
- A provider abstraction avoids hard-coding Rust semantics into the shared planner.

## Decision 6: Use fixture-driven integration tests for discovery and propagation

**Decision**: Validate the feature primarily with repository fixtures that cover mixed ecosystems, glob-based workspaces, version groups, change-input files, and transitive graphs.

**Rationale**:

- The constitution requires realistic, purpose-driven tests.
- The feature depends heavily on reading native manifests and modeling real dependency graphs.
- Fixture repositories make ecosystem parity gaps visible.

## Decision 7: Keep documentation and docs book in scope from the first implementation step

**Decision**: Treat the mdBook under `docs/` as a first-class artifact and document configuration, grouping, discovery, and planning behavior alongside the code.

## Implementation notes

- Change files resolve package references by id, package name, relative manifest path, or relative package directory.
- Discovery adapters remain reusable public APIs through each ecosystem crate.
- The initial Rust semver provider consumes explicit evidence strings such as `rust-semver:major:public API break detected`.

## Migration considerations

- Repositories can adopt discovery first and add release planning later without changing native workspace manifests.
- Version groups are additive and can be introduced package-by-package.
- Later bot automation can call the shared planner without changing the current release-plan data model.
