# Tasks: Cross-Ecosystem Release Planning Foundation

**Input**: Design documents from `/specs/001-first-step-port/`\
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Tests are required for this feature because the constitution mandates test-first development for non-trivial behavior and the plan explicitly calls for fixture-first validation.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this belongs to (e.g. `[US1]`, `[US2]`, `[US3]`)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Establish the expanded workspace layout, shared tooling, and fixture scaffolding required by all user stories.

- [x] T001 Update workspace membership and internal dependencies in `/Users/ifiokjr/Developer/projects/monochange/Cargo.toml` for `crates/monochange_config`, `crates/monochange_graph`, `crates/monochange_semver`, `crates/monochange_deno`, and `crates/monochange_dart`
- [x] T002 Create crate scaffolds and `Cargo.toml` files for `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_config`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_graph`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_semver`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_deno`, and `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_dart`
- [x] T003 [P] Create shared fixture directories and placeholder README files under `/Users/ifiokjr/Developer/projects/monochange/fixtures/cargo`, `/Users/ifiokjr/Developer/projects/monochange/fixtures/npm`, `/Users/ifiokjr/Developer/projects/monochange/fixtures/deno`, `/Users/ifiokjr/Developer/projects/monochange/fixtures/dart`, `/Users/ifiokjr/Developer/projects/monochange/fixtures/flutter`, and `/Users/ifiokjr/Developer/projects/monochange/fixtures/mixed`
- [x] T004 [P] Add crate-level doctest/readme scaffolds in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_config/readme.md`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_graph/readme.md`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_semver/readme.md`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_deno/readme.md`, and `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_dart/readme.md`
- [x] T005 [P] Wire baseline library entrypoints in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_config/src/lib.rs`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_graph/src/lib.rs`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_semver/src/lib.rs`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_deno/src/lib.rs`, and `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_dart/src/lib.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build the shared domain model, adapter contracts, configuration surface, and CLI plumbing that all user stories depend on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T006 Add normalized domain types for `WorkspaceConfiguration`, `PackageRecord`, `DependencyEdge`, `VersionGroup`, `ChangeSignal`, `CompatibilityAssessment`, `ReleaseDecision`, and `ReleasePlan` in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_core/src/lib.rs`
- [x] T007 [P] Add unit tests for the shared domain invariants in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_core/src/__tests.rs`
- [x] T008 Implement `monochange.toml` parsing, defaults, and validation in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_config/src/lib.rs`
- [x] T009 [P] Add configuration parsing and validation tests in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_config/src/__tests.rs`
- [x] T010 Implement the ecosystem adapter traits and normalized discovery contract in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_core/src/lib.rs`
- [x] T011 Implement the normalized dependency graph builder and cycle-safe traversal primitives in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_graph/src/lib.rs`
- [x] T012 [P] Add graph construction and traversal tests in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_graph/src/__tests.rs`
- [x] T013 Implement the compatibility provider abstraction and bump-severity comparison rules in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_semver/src/lib.rs`
- [x] T014 [P] Add compatibility-provider unit tests in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_semver/src/__tests.rs`
- [x] T015 Add CLI command skeletons for `discover` and `release --dry-run` in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/main.rs` and `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/bin/mc.rs`
- [x] T016 [P] Add CLI smoke tests for command parsing and help output in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/__tests.rs`

**Checkpoint**: Shared domain, config, graph, compatibility, and CLI command surfaces are ready for story implementation.

---

## Phase 3: User Story 1 - Discover a mixed-ecosystem workspace (Priority: P1) 🎯 MVP

**Goal**: Discover supported packages, native workspace membership, dependency links, glob matches, and version groups across mixed repositories.

**Independent Test**: Point `mc discover --format json` from the fixture root at representative Cargo, npm-family, Deno, Dart/Flutter, and mixed fixtures and verify that packages, dependency edges, version groups, and warnings are correct without manual package enumeration.

### Tests for User Story 1 ⚠️

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [x] T017 [P] [US1] Add Cargo discovery fixture tests in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_cargo/src/__tests.rs`
- [x] T018 [P] [US1] Add npm/pnpm/Bun workspace and glob discovery fixture tests in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_npm/src/__tests.rs`
- [x] T019 [P] [US1] Add Deno workspace and standalone discovery fixture tests in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_deno/src/__tests.rs`
- [x] T020 [P] [US1] Add Dart/Flutter workspace and standalone discovery fixture tests in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_dart/src/__tests.rs`
- [x] T021 [P] [US1] Add mixed-repository discovery integration tests for `discover` in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/__tests.rs`
- [x] T022 [P] [US1] Add CLI contract snapshot tests for discovery JSON output in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/__tests.rs`

### Implementation for User Story 1

- [x] T023 [P] [US1] Implement Cargo workspace and standalone package discovery in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_cargo/src/lib.rs`
- [x] T024 [P] [US1] Implement npm, pnpm, and Bun workspace discovery with glob resolution in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_npm/src/lib.rs`
- [x] T025 [P] [US1] Implement Deno workspace and standalone package discovery in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_deno/src/lib.rs`
- [x] T026 [P] [US1] Implement Dart and Flutter workspace and standalone package discovery in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_dart/src/lib.rs`
- [x] T027 [US1] Implement version-group resolution and package normalization in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_config/src/lib.rs`
- [x] T028 [US1] Implement unified adapter aggregation and discovery orchestration in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/main.rs`
- [x] T029 [US1] Implement `discover` JSON/text rendering in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/main.rs`
- [x] T030 [US1] Add representative fixture manifests under `/Users/ifiokjr/Developer/projects/monochange/fixtures/cargo`, `/Users/ifiokjr/Developer/projects/monochange/fixtures/npm`, `/Users/ifiokjr/Developer/projects/monochange/fixtures/deno`, `/Users/ifiokjr/Developer/projects/monochange/fixtures/dart`, `/Users/ifiokjr/Developer/projects/monochange/fixtures/flutter`, and `/Users/ifiokjr/Developer/projects/monochange/fixtures/mixed`

**Checkpoint**: User Story 1 should now provide independently testable cross-ecosystem workspace discovery and grouping.

---

## Phase 4: User Story 2 - Plan coordinated version changes across transitive dependencies (Priority: P2)

**Goal**: Compute release plans that propagate changes through transitive dependencies, synchronize version groups, and apply semver-aware escalation when compatibility evidence exists.

**Independent Test**: Run `mc release --dry-run --format json` from the fixture root on graph fixtures and verify direct bumps, transitive patch propagation, group synchronization, and Rust semver-driven escalation.

### Tests for User Story 2 ⚠️

- [x] T031 [P] [US2] Add graph propagation tests for direct and transitive dependency impact in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_graph/src/__tests.rs`
- [x] T032 [P] [US2] Add version-group synchronization tests in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_graph/src/__tests.rs`
- [x] T033 [P] [US2] Add Rust semver escalation tests in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_semver/src/__tests.rs`
- [x] T034 [P] [US2] Add release-plan integration tests for mixed dependency graphs in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/__tests.rs`
- [x] T035 [P] [US2] Add CLI contract snapshot tests for `release --dry-run` output in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/__tests.rs`

### Implementation for User Story 2

- [x] T036 [P] [US2] Implement change-signal loading and validation in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_config/src/lib.rs`
- [x] T037 [US2] Implement release propagation, severity merging, and group synchronization in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_graph/src/lib.rs`
- [x] T038 [US2] Implement the first Rust compatibility provider in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_cargo/src/lib.rs` and `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_semver/src/lib.rs`
- [x] T039 [US2] Implement release planner orchestration in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/main.rs`
- [x] T040 [US2] Implement `release --dry-run` JSON/text rendering and warnings in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/main.rs`
- [x] T041 [US2] Add release-planning fixtures and change-input files under `/Users/ifiokjr/Developer/projects/monochange/fixtures/mixed` and `/Users/ifiokjr/Developer/projects/monochange/fixtures/cargo`

**Checkpoint**: User Stories 1 and 2 should now support discovery plus release planning with transitive and semver-aware behavior.

---

## Phase 5: User Story 3 - Use one tool consistently across ecosystems (Priority: P3)

**Goal**: Ensure the same core planning behavior and onboarding experience exists across supported ecosystems, with reusable crates and documentation that explain the system clearly.

**Independent Test**: Validate representative fixtures and documentation examples for Cargo, npm-family, Deno, Dart, and Flutter to confirm parity for discovery, grouping, defaults, and release planning behavior.

### Tests for User Story 3 ⚠️

- [x] T042 [P] [US3] Add parity tests that assert equivalent discovery and planning behavior across ecosystem fixtures in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/src/__tests.rs`
- [x] T043 [P] [US3] Add configuration-default tests for `monochange.toml` parity across ecosystems in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_config/src/__tests.rs`
- [x] T044 [P] [US3] Add quickstart validation tests or snapshots for documented commands in `/Users/ifiokjr/Developer/projects/monochange/docs/src/readme.md` and `/Users/ifiokjr/Developer/projects/monochange/specs/001-first-step-port/quickstart.md`

### Implementation for User Story 3

- [x] T045 [P] [US3] Expose reusable adapter APIs from `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_cargo/src/lib.rs`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_npm/src/lib.rs`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_deno/src/lib.rs`, and `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_dart/src/lib.rs`
- [x] T046 [US3] Update user-facing documentation in `/Users/ifiokjr/Developer/projects/monochange/readme.md` and `/Users/ifiokjr/Developer/projects/monochange/CONTRIBUTING.md` for configuration, grouping, release planning, and supported ecosystems
- [x] T047 [US3] Add mdBook chapters for discovery, configuration, version groups, and release planning in `/Users/ifiokjr/Developer/projects/monochange/docs/src/`
- [x] T048 [US3] Update crate READMEs in `/Users/ifiokjr/Developer/projects/monochange/crates/monochange/readme.md`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_core/readme.md`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_cargo/readme.md`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_npm/readme.md`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_deno/readme.md`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_dart/readme.md`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_config/readme.md`, `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_graph/readme.md`, and `/Users/ifiokjr/Developer/projects/monochange/crates/monochange_semver/readme.md`
- [x] T049 [US3] Align docs examples and fixture content for pnpm, Bun, Deno, Dart, and Flutter parity under `/Users/ifiokjr/Developer/projects/monochange/docs/src/` and `/Users/ifiokjr/Developer/projects/monochange/fixtures/`

**Checkpoint**: All three user stories should now be independently functional and documented.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final workspace-wide refinements, validation, and cleanup.

- [x] T050 [P] Run and fix full validation in `/Users/ifiokjr/Developer/projects/monochange` via `lint:all`, `test:all`, `build:all`, and `build:book`
- [x] T051 [P] Review and tighten fixture coverage gaps in `/Users/ifiokjr/Developer/projects/monochange/fixtures/`
- [x] T052 Document implementation notes and migration considerations in `/Users/ifiokjr/Developer/projects/monochange/specs/001-first-step-port/quickstart.md` and `/Users/ifiokjr/Developer/projects/monochange/specs/001-first-step-port/research.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion — BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational completion — MVP
- **User Story 2 (Phase 4)**: Depends on Foundational completion and reuses discovery outputs from User Story 1 fixtures and adapter implementations
- **User Story 3 (Phase 5)**: Depends on Foundational completion and benefits from User Stories 1 and 2, but remains independently testable through parity and documentation validation
- **Polish (Phase 6)**: Depends on desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Starts after Foundational and delivers the first viable product slice
- **User Story 2 (P2)**: Starts after Foundational, but should build on the normalized package and graph model introduced for US1
- **User Story 3 (P3)**: Starts after Foundational; documentation and parity validation depend on the behavior delivered in US1 and US2

### Within Each User Story

- Tests MUST be written and fail before implementation
- Shared models/contracts before adapter/service behavior
- Adapter behavior before CLI orchestration
- CLI orchestration before snapshot and fixture validation
- Story completion before moving to the next validation checkpoint

### Parallel Opportunities

- Setup tasks T003–T005 can run in parallel after T001–T002
- Foundational tests T007, T009, T012, T014, and T016 can run in parallel with isolated files
- US1 adapter tests and implementations for Cargo, npm, Deno, and Dart/Flutter are parallelizable by crate
- US2 graph, semver, and CLI contract tests can run in parallel
- US3 parity tests, docs updates, and README updates can be split across contributors
- Final validation and fixture review can run in parallel once implementation stabilizes

---

## Parallel Example: User Story 1

```bash
# Launch adapter test tasks together:
Task: "T017 [US1] Add Cargo discovery fixture tests in crates/monochange_cargo/src/__tests.rs"
Task: "T018 [US1] Add npm/pnpm/Bun workspace and glob discovery fixture tests in crates/monochange_npm/src/__tests.rs"
Task: "T019 [US1] Add Deno workspace and standalone discovery fixture tests in crates/monochange_deno/src/__tests.rs"
Task: "T020 [US1] Add Dart/Flutter workspace and standalone discovery fixture tests in crates/monochange_dart/src/__tests.rs"

# Launch adapter implementation tasks together after tests fail:
Task: "T023 [US1] Implement Cargo workspace and standalone package discovery in crates/monochange_cargo/src/lib.rs"
Task: "T024 [US1] Implement npm, pnpm, and Bun workspace discovery with glob resolution in crates/monochange_npm/src/lib.rs"
Task: "T025 [US1] Implement Deno workspace and standalone package discovery in crates/monochange_deno/src/lib.rs"
Task: "T026 [US1] Implement Dart and Flutter workspace and standalone package discovery in crates/monochange_dart/src/lib.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: Run discovery fixtures and CLI snapshots for US1 independently
5. Demo mixed-ecosystem discovery before moving to release planning

### Incremental Delivery

1. Complete Setup + Foundational → stable shared model and adapter contract
2. Add User Story 1 → validate workspace discovery and version groups
3. Add User Story 2 → validate transitive propagation and semver escalation
4. Add User Story 3 → validate ecosystem parity and documentation
5. Finish with full workspace validation and docs-book verification

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together
2. Once Foundational is complete:
   - Developer A: Cargo + CLI discovery slice for US1
   - Developer B: npm/Deno/Dart adapter slices for US1
   - Developer C: Graph + semver planning slice for US2
3. Documentation and parity work for US3 begins once behavior stabilizes

---

## Notes

- [P] tasks touch separate files and can be worked in parallel
- [Story] labels map every story task directly back to the spec
- Every user story includes failing tests first to satisfy the constitution
- Each story has an independent validation checkpoint
- Avoid collapsing adapter logic back into monolithic crates; keep shared behavior in focused shared crates
