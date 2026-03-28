# Implementation Plan: Cross-Ecosystem Release Planning Foundation

**Branch**: `001-first-step-port` | **Date**: 2026-03-25 | **Spec**: [`specs/001-first-step-port/spec.md`](./spec.md)\
**Input**: Feature specification from `/specs/001-first-step-port/spec.md`

## Summary

Port the first major layer of knope-like release planning into monochange by building a reusable Rust workspace architecture for cross-ecosystem package discovery, dependency graph construction, version-group coordination, and semver-aware release propagation. The technical approach is to keep `crates/monochange` as the CLI aggregator, expand shared planning logic into focused reusable crates, add ecosystem adapters for Cargo, npm-family, Deno, and Dart/Flutter, and document the product through the repository mdBook in `docs/`. GitHub bot automation is explicitly deferred.

## Technical Context

**Language/Version**: Rust edition 2024, workspace MSRV 1.90.0, toolchain 1.93.1 for development\
**Primary Dependencies**: `clap`, `serde`, `semver`, `cargo_metadata`, `serde_json`, `typed-builder`, `thiserror`, planned TOML/config parsing support, planned glob/path-matching support\
**Storage**: Local repository files and native manifest/config files (`monochange.toml`, Cargo manifests, package manifests, workspace definitions)\
**Testing**: `cargo nextest`, `cargo test --doc`, `rstest`, `insta`, `similar-asserts`, fixture repositories for integration coverage\
**Target Platform**: Cross-platform CLI and reusable Rust libraries for macOS, Linux, and Windows\
**Project Type**: Rust workspace containing reusable libraries, ecosystem adapters, CLI binaries, and mdBook documentation\
**Performance Goals**: Discover and plan releases for representative multi-package repositories in one local CLI pass; keep graph/planning operations fast enough for interactive developer usage and CI execution\
**Constraints**: No GitHub bot automation in this milestone; equal core capability across supported ecosystems; glob-based workspace discovery required; default parent propagation is patch unless compatibility evidence escalates it; docs book must remain in scope; no unsafe code; all work must pass current workspace quality gates\
**Scale/Scope**: First milestone covers Cargo, npm, pnpm, Bun, Deno, Dart, and Flutter package discovery plus mixed-repository release planning, version groups, and transitive dependency propagation

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

### Pre-Design Gate Review

- **Test-First Development**: PASS — implementation will begin with fixture-driven and algorithm-focused failing tests for discovery, graph propagation, version groups, and semver escalation.
- **Workspace-First Modular Architecture**: PASS — the design uses focused shared crates and per-ecosystem adapter crates rather than a monolithic port.
- **Documentation Is a Product Surface**: PASS — the feature includes mdBook updates, crate README updates, spec artifacts, and quickstart guidance.
- **Strict Quality Gates, Formatting, and Safety**: PASS — the plan uses the existing `devenv`, `dprint`, `cargo clippy`, `cargo nextest`, and docs build workflow already established in the repository.
- **Release Discipline and SemVer Integrity**: PASS — this feature introduces semver-aware planning and will require release-note discipline when implemented, while deferring only bot automation.

### Post-Design Gate Review

- **Test strategy remains fixture-first and realistic**: PASS
- **Crate boundaries remain focused and reusable**: PASS
- **Documentation outputs are included in the design artifacts**: PASS
- **No constitution violations require justification**: PASS

## Project Structure

### Documentation (this feature)

```text
specs/001-first-step-port/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── cli.md
│   └── configuration.md
└── tasks.md
```

### Source Code (repository root)

```text
Cargo.toml
crates/
├── monochange/              # CLI entrypoints (`monochange`, `mc`) and adapter aggregation
├── monochange_core/         # shared domain traits, normalized package/release types
├── monochange_config/       # monochange.toml parsing, defaults, validation
├── monochange_graph/        # normalized dependency graph + transitive propagation rules
├── monochange_semver/       # compatibility provider abstraction and escalation policy
├── monochange_cargo/        # Cargo workspace + crate discovery and Rust semver provider
├── monochange_npm/          # npm, pnpm, and Bun discovery via package/workspace manifests
├── monochange_deno/         # Deno workspace and standalone package discovery
└── monochange_dart/         # Dart and Flutter workspace/package discovery via pubspec

docs/
├── Cargo.toml
├── book.toml
└── src/

fixtures/
├── cargo/
├── npm/
├── deno/
├── dart/
├── flutter/
└── mixed/

setup/
└── editors/
```

**Structure Decision**: Keep the current Rust workspace root and mdBook layout, preserve `crates/monochange` as the CLI package, keep `crates/monochange_core` as the shared domain surface, and add new focused crates for config, graph, semver, Deno, and Dart/Flutter support. Use root-level fixture repositories to validate discovery parity and mixed-ecosystem propagation behavior.

## Complexity Tracking

| Violation                                                      | Why Needed                                                                                      | Simpler Alternative Rejected Because                                                                                                   |
| -------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Additional focused crates beyond the current minimal workspace | Cross-ecosystem parity, reusable adapters, and semver-aware planning require separable concerns | A smaller monolithic port would violate the workspace-first principle and make ecosystem support harder to reuse or test independently |
