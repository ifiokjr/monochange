# Monochange Constitution

## Core Principles

### I. Test-First Development (NON-NEGOTIABLE)

Every non-trivial behavioral change MUST begin with an executable test or specification that fails for the right reason before implementation begins. The required workflow is Red → Green → Refactor.

This applies especially to version graph logic, release orchestration, dependency propagation, and cross-ecosystem package coordination. When a logic bug is discovered, it MUST be reproduced with a failing regression test before any fix is merged.

Tests MUST be purposeful, readable, and realistic. They MUST cover edge cases, error paths, and real-world monorepo scenarios. Snapshot tests are allowed only when they improve clarity, and snapshot updates MUST be intentionally reviewed rather than blindly accepted.

**Why this exists**: monochange manages release-critical behavior. Silent regressions in versioning or publishing workflows are unacceptable.

### II. Workspace-First Modular Architecture

Monochange MUST preserve a clear workspace-oriented structure with focused crates and explicit responsibilities.

The default project structure is:

- `crates/monochange_core` for shared domain logic and reusable change-planning behavior
- `crates/monochange_cli` for command-line UX only
- `crates/monochange_cargo` for Cargo ecosystem integration
- `crates/monochange_npm` for npm ecosystem integration
- `crates/monochange` for the top-level public facade where appropriate
- `docs/` for the mdBook documentation site
- `setup/` for editor and developer-environment bootstrap files

New capabilities MUST be added to the smallest sensible boundary. CLI crates MUST NOT become dumping grounds for business logic. Shared logic belongs in core crates; ecosystem-specific behavior belongs in integration crates. New language or package-manager support SHOULD be introduced as a new focused crate instead of being folded into unrelated modules.

Each crate MUST remain independently understandable, testable, and documented.

### III. Documentation Is a Product Surface

Documentation is not optional polish; it is part of the shipped product.

Every feature or behavior change MUST update the relevant documentation at the same time as the code, including as applicable:

- the root `readme.md`
- affected crate `readme.md` files
- `docs/` book content
- examples, quickstarts, and command usage text
- migration notes for behavior or compatibility changes
- spec artifacts generated through the `/spec` workflow

Public APIs, CLI flags, configuration behavior, and release semantics MUST be documented rigorously and with examples where ambiguity is possible. Specs and plans MUST be detailed enough that an engineer can implement safely without guessing intent.

If a change alters user-visible behavior and no documentation changes are needed, the PR MUST justify why.

### IV. Strict Quality Gates, Formatting, and Safety

Monochange MUST use a reproducible development environment and enforce strict local and CI quality gates.

The authoritative development environment is `devenv`. Formatting is enforced through `dprint`, not ad hoc formatter invocations. Rust formatting and imports MUST remain consistent and machine-enforced.

At minimum, changes MUST pass the project-standard verification suite before merge:

- `dprint check`
- `cargo clippy --all-features`
- `cargo check --all-features`
- `cargo nextest run --all-features`
- `cargo test --doc --all-features`
- any workspace build and docs checks required by CI

The codebase MUST maintain strict lint and safety posture modeled after `mdt`:

- `unsafe_code` MUST be denied workspace-wide
- `unstable_features` MUST be denied workspace-wide
- `clippy::correctness` MUST be denied
- `clippy::wildcard_dependencies` MUST be denied
- panicking shortcuts such as casual `expect` usage in production paths SHOULD be avoided in favor of explicit error handling or explicit panic context

No code may be merged with failing tests, failing format checks, or unresolved lint violations unless the constitution is explicitly amended.

### V. Release Discipline and SemVer Integrity

Monochange MUST use a rigorous, documented, changeset-driven release process.

Every pull request that changes code in a publishable crate MUST include a release note entry in `.changeset/` using the same disciplined style as `mdt`. Changesets MUST be specific, user-meaningful, and accurate about scope and impact.

Release preparation MUST be automated through a workflow equivalent to:

- `knope document-change`
- `knope release`
- `knope publish`

Breaking changes MUST be called out explicitly with a major change classification and accompanying migration guidance. Published crate compatibility SHOULD be protected with semver verification in CI for all public crates.

Versions, changelogs, and release artifacts MUST never be updated informally or retroactively without a traceable workflow.

## Technical and Structural Standards

Monochange is a Rust workspace for multi-language monorepo change management. The repository MUST remain organized around explicit package boundaries, reproducible tooling, and discoverable documentation.

Baseline standards:

- Rust workspace layout rooted at `Cargo.toml`
- publishable crates under `crates/`
- mdBook documentation under `docs/`
- reproducible development tooling via `devenv`, `direnv`, and pinned project scripts
- formatting via `dprint`
- fast test execution via `cargo-nextest`
- support for focused assertions, fixtures, and snapshots where they improve signal
- release metadata and changelog generation managed by an automated workflow

Repository-wide configuration files at the root MUST remain authoritative. New tools or workflows SHOULD be introduced only when they simplify or strengthen the developer experience without duplicating existing responsibilities.

## Development Workflow and Review Gates

Non-trivial work MUST follow the spec workflow:

1. `/spec specify` to define the user-facing outcome
2. `/spec clarify` when requirements or constraints are ambiguous
3. `/spec plan` to capture technical approach and constitution compliance
4. `/spec tasks` to break work into independently verifiable units

All changes MUST be made on feature branches and merged through pull requests. Direct commits to `main` are prohibited.

Before requesting review, a change MUST:

- satisfy the test-first rule
- include required documentation updates
- include required changeset entries for publishable changes
- pass local quality gates
- remain consistent with the workspace structure
- note any compatibility or migration impact

A review MUST block on constitution violations, not merely note them. If a simpler design can satisfy the same requirement, the simpler design wins unless the PR documents why it is insufficient.

## Governance

This constitution supersedes local habit, temporary convenience, and undocumented team preference.

All implementation plans MUST include a Constitution Check against this document before design and again before implementation. Runtime guidance for agents and contributors MAY live in `.specify/memory/pi-agent.md`, but that guidance MUST NOT contradict this constitution.

Amendments require a pull request that:

- explains the reason for the change
- identifies affected workflows, tooling, or documentation
- includes any migration or adoption plan
- updates the constitution version appropriately

Versioning rules for this constitution:

- **MAJOR**: removes or materially redefines a core principle
- **MINOR**: adds a new principle or materially expands governance
- **PATCH**: clarifies wording without changing intent

**Version**: 1.0.0 | **Ratified**: 2026-03-25 | **Last Amended**: 2026-03-25
