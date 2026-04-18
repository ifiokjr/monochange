# Harness engineering improvements

## Goal

Make `monochange` more legible to agents by improving the repo-local system of record, expanding machine-readable tooling, and adding mechanical checks that prevent architecture and documentation drift.

## Scope

- repo map and planning docs
- MCP surfaces for diagnostics and lint metadata
- documentation freshness checks
- architecture boundary checks
- agent-style eval coverage for release-planning workflows

## Checklist

- [x] add a top-level `ARCHITECTURE.md` with crate placement rules and explicit dispatch points
- [x] add `docs/plans/` with active, completed, and tech-debt locations
- [x] link architecture and plans from `AGENTS.md` and the agent workflow docs
- [x] expose changeset diagnostics through MCP
- [x] expose lint catalog and lint explanation data through MCP
- [x] update assistant-facing docs and skill docs to reflect the live MCP tool surface
- [x] add a docs freshness check for the agent-facing repo map and MCP tool list
- [x] add an architecture boundary check for new provider/ecosystem dispatch points
- [x] add agent-style eval tests that exercise the machine-readable release workflow

## Validation

- `docs:check`
- `lint:architecture`
- `cargo test --package monochange --all-features mcp::`
- `test:agent-evals`

## Notes

- `docs/exec-plans` was intentionally renamed to `docs/plans` for this repository.
- The architecture check uses an allowlist of explicit dispatch points so new exceptions stay visible in review.
