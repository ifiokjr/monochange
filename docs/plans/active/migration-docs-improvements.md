# Migration Documentation & Skill Improvements

> Status: **Active** Created: 2025-05-15

## Problem

Creating migration plans for 9 real-world repositories revealed significant gaps in monochange's migration documentation and skill:

1. **No ecosystem-specific migration guides** — The existing `10-migrating-from-knope.md` only covers Rust/cargo config translation. There are no guides for Dart/Flutter, npm, Deno, or Python ecosystems.
2. **No CI workflow templates** — The docs show `mc release` but not complete GitHub Actions workflows for release PRs, binary cross-compilation, Sigstore attestation, or trusted publishing.
3. **No binary release guide** — Rust CLI projects need cross-compilation matrices, npm platform packages, and asset attestation. No docs exist for this.
4. **Skill `adoption.md` is too brief** — 44 lines, just a checklist. Missing: ecosystem-specific steps, CI workflow templates, changeset format migration examples for scoped NOPE changesets vs knope changesets.
5. **No `mc agent` integration** — The skill doesn't describe how an AI agent should create migration PRs or what steps to follow.
6. **Missing trusted publishing setup per-registry** — crates.io OIDC, npm provenance, pub.dev OIDC all require different setup that isn't documented.

## Plan

### 1. Expand `adoption.md` in the skill (HIGH PRIORITY)

The skill's adoption module needs:

- Ecosystem-specific migration paths (cargo, npm, dart, flutter, deno, python, go)
- CI workflow templates (release.yml, publish.yml, changeset-policy.yml, setup-mc action)
- Binary release guidance for Rust CLI projects
- Trusted publishing setup per-registry
- Changeset format migration examples for knope scoped format, NOPE format, and lockstep format

### 2. Add `migration-workflows.md` to the skill (NEW MODULE)

A new skill module focused on complete, copy-pasteable CI workflow templates:

- `changeset-policy.yml` — PR changeset requirement enforcement
- `release.yml` — Release PR + optional cross-compile + attestation
- `publish.yml` — Trusted publishing (crates.io OIDC, npm provenance, pub.dev OIDC)
- `setup-mc/action.yml` — Reusable composite action

Per-ecosystem variants:

- Rust CLI with cross-compile (8-target matrix + attestation)
- Rust library (crates.io OIDC only)
- npm monorepo (npm provenance)
- Dart/Flutter monorepo (pub.dev OIDC + melos)
- Mixed ecosystem (cargo + npm, dart + npm)

### 3. Expand book `10-migrating-from-knope.md` (HIGH PRIORITY)

Add sections:

- Dart/Flutter migration (pubspec.yaml workspace → monochange.toml)
- Lockstep versioning migration (knope `default:` → monochange groups)
- Scoped changeset migration (knope `app:`, `server:` → monochange package IDs)
- NOPE changeset migration (YAML frontmatter → monochange heading format)
- Complete GitHub Actions workflow migration (not just `knope release` → `mc release-pr`)

### 4. Add new book page: `16-migration-catalog.md` (NEW PAGE)

A catalog of verified migration patterns organized by ecosystem:

- Rust workspace with CLI binary (e.g., mdt, lspee, pina)
- Rust library-only (e.g., wasm_solana)
- npm monorepo with lockstep versioning (e.g., oh-pi)
- Dart/Flutter monorepo with melos (e.g., openbudget, solana_kit, skribble, verily)
- Mixed Rust + npm (e.g., mdt, pina)

Each pattern includes: monochange.toml template, changeset examples, CI workflow, and migration checklist.

### 5. Add `mc agent` context to the skill

The skill should describe the recommended agent workflow for creating migration PRs:

1. Clone repo → create branch
2. Run `mc init --provider github`
3. Translate knope.toml/NOPE config → monochange.toml
4. Migrate changesets (format conversion)
5. Create/update GitHub Actions workflows
6. Remove old tooling
7. Run `mc step:validate`
8. Create PR with migration checklist

### 6. Update `trusted-publishing.md` skill (MEDIUM)

Add per-registry OIDC setup instructions:

- crates.io: `rust-lang/crates-io-auth-action@v1`
- npm: `NPM_CONFIG_PROVENANCE=true` + OIDC publisher setup
- pub.dev: GitHub Actions OIDC publisher in pub.dev dashboard

## Improvements outside documentation

These aren't doc changes — they're product/improvement suggestions:

1. **`mc init` should detect knope.toml** — When `mc init` finds a `knope.toml`, it should offer to auto-translate it to `monochange.toml` instead of generating from scratch.
2. **`mc init` should detect ecosystem** — Instead of requiring `--ecosystem`, auto-detect from `Cargo.toml` workspace, `package.json`, `pubspec.yaml`, or `deno.json`.
3. **`mc migrate knope` subcommand** — A dedicated subcommand that reads `knope.toml` and outputs the equivalent `monochange.toml`, including groups, changelogs, and workflows.
4. **Changeset converter** — A tool to batch-convert `.changeset/*.md` from knope/NOPE format to monochange format.
5. **CI workflow generator** — `mc init --ci` that generates `release.yml`, `publish.yml`, and `changeset-policy.yml` based on detected ecosystem.
6. **Better error messages** — When `mc step:validate` fails on a knope-style changeset (e.g., `default: minor`), suggest the monochange equivalent.
