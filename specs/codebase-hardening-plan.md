# Codebase hardening plan

## Goal

Deepen `monochange` correctness, stability, and maintainability across output rendering, release workflows, diagnostics, changelog handling, documentation sync, and snapshot-based CLI coverage.

## Stage 1 — Stabilize identities and output ergonomics

### Objectives

1. Normalize package ids and path rendering so relative and absolute repository roots produce stable package identities and CLI output.
2. Improve release-plan text rendering so output ordering and path presentation are deterministic and snapshot-friendly.
3. Harden release-target and tag rendering behavior for grouped and ungrouped packages.

### Deliverables

- canonical path helpers shared by CLI/config/runtime code
- stable text rendering for discovery, planning, and workflow summaries
- tests for normalized ids, relative path rendering, and tag/release target behavior

## Stage 2 — Expand automated verification

### Objectives

4. Expand `insta-cmd` coverage across the CLI surface:
   - `mc check`
   - discovery output
   - plan output
   - change creation
   - release dry-runs and failure cases
5. Add broader workflow/release integration coverage for Cargo, npm, Deno, Dart/Flutter, and mixed fixtures.
6. Tighten changelog behavior coverage for package defaults, package overrides, grouped changelogs, and changelog exclusion.
7. Add a dedicated migration test suite for legacy-to-current config/documentation flows.
8. Convert high-value assertion-style CLI output tests to snapshots where snapshots provide better reviewability.

### Deliverables

- `insta-cmd` snapshot coverage for core commands
- ecosystem integration tests for propagated release behavior
- explicit changelog matrix tests
- migration validation tests

## Stage 3 — Improve diagnostics and release semantics

### Objectives

9. Improve diagnostics UX with richer rendering and better guidance for config, changeset, and workflow errors.
10. Strengthen outward release identity behavior around groups, tags, and release targets.

### Deliverables

- clearer CLI diagnostic rendering
- tests for improved error text and guidance
- stronger coverage for group-owned version/tag behavior

## Stage 4 — Harden documentation and drift detection

### Objectives

11. Update shared MDT template sources and generated docs to reflect the normalized behavior.
12. Add or strengthen documentation verification for examples that are easy to drift.
13. Re-run docs sync, linting, and full validation.

### Deliverables

- synced templates and mdBook pages
- expanded doc drift checks/tests where useful
- full green local validation

## Validation gates

Each stage should keep the repository green with:

- `cargo test --workspace --all-features`
- `devenv shell -- lint:all`
- `devenv shell -- build:book`
- `devenv shell -- docs:update`
- `devenv shell -- docs:verify`
