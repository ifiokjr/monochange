<!-- {@monochangeCrateDocs} -->

# `monochange`

The `monochange` crate provides the end-user CLI.

## Commands

```bash
mc workspace discover --root . --format json
mc changes add --root . --package crates/monochange --bump patch --reason "describe the change"
mc plan release --root . --changes .changeset/1234567890-crates-monochange.md --format json
```

## Responsibilities

- aggregate all supported ecosystem adapters
- load `monochange.toml`
- resolve change input files
- render discovery and release-plan output in text or JSON

<!-- {/monochangeCrateDocs} -->

<!-- {@monochangeCoreCrateDocs} -->

# `monochange_core`

Shared domain types for `monochange`.

This crate defines:

- normalized package and dependency records
- version-group definitions and planned group outcomes
- change signals and compatibility assessments
- release-plan domain types
- shared error and result types

## Example

```rust
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use std::path::PathBuf;

let package = PackageRecord::new(
    Ecosystem::Cargo,
    "demo",
    PathBuf::from("crates/demo/Cargo.toml"),
    PathBuf::from("."),
    Some(Version::new(1, 2, 3)),
    PublishState::Public,
);

assert_eq!(package.name, "demo");
assert_eq!(package.current_version, Some(Version::new(1, 2, 3)));
```

<!-- {/monochangeCoreCrateDocs} -->

<!-- {@monochangeCargoCrateDocs} -->

# `monochange_cargo`

Cargo ecosystem support for `monochange`.

## Public entry points

- `discover_cargo_packages(root)` discovers Cargo workspaces and standalone crates
- `CargoAdapter` exposes the shared adapter interface
- `RustSemverProvider` parses explicit Rust semver evidence from change input

## Scope

- Cargo workspace glob expansion
- crate manifest parsing
- normalized dependency extraction
- Rust semver provider integration for release planning

<!-- {/monochangeCargoCrateDocs} -->

<!-- {@monochangeConfigCrateDocs} -->

# `monochange_config`

Configuration and change-input parsing for `monochange`.

## Responsibilities

- load `monochange.toml`
- validate version groups and workflows
- resolve package references against discovered packages
- parse change-input files, evidence, and changelog overrides

<!-- {/monochangeConfigCrateDocs} -->

<!-- {@monochangeGraphCrateDocs} -->

# `monochange_graph`

Dependency-graph traversal and release propagation for `monochange`.

## Responsibilities

- build reverse dependency views
- propagate release impact across direct and transitive dependents
- synchronize version groups
- calculate planned group versions

<!-- {/monochangeGraphCrateDocs} -->

<!-- {@monochangeNpmCrateDocs} -->

# `monochange_npm`

npm-family ecosystem support for `monochange`.

## Public entry points

- `discover_npm_packages(root)` discovers npm, pnpm, and Bun workspaces plus standalone packages
- `NpmAdapter` exposes the shared adapter interface

## Scope

- `package.json` workspaces
- `pnpm-workspace.yaml`
- Bun lockfile detection
- normalized dependency extraction

<!-- {/monochangeNpmCrateDocs} -->

<!-- {@monochangeDenoCrateDocs} -->

# `monochange_deno`

Deno ecosystem support for `monochange`.

## Public entry points

- `discover_deno_packages(root)` discovers Deno workspaces and standalone packages
- `DenoAdapter` exposes the shared adapter interface

## Scope

- `deno.json` and `deno.jsonc`
- workspace glob expansion
- normalized dependency and import extraction

<!-- {/monochangeDenoCrateDocs} -->

<!-- {@monochangeDartCrateDocs} -->

# `monochange_dart`

Dart and Flutter ecosystem support for `monochange`.

## Public entry points

- `discover_dart_packages(root)` discovers Dart and Flutter workspaces plus standalone packages
- `DartAdapter` exposes the shared adapter interface

## Scope

- `pubspec.yaml` workspace expansion
- Dart package parsing
- Flutter package detection
- normalized dependency extraction

<!-- {/monochangeDartCrateDocs} -->

<!-- {@monochangeSemverCrateDocs} -->

# `monochange_semver`

Semver and compatibility helpers for `monochange`.

## Responsibilities

- collect compatibility assessments from providers
- merge bump severities deterministically
- calculate direct and propagated bump severities
- provide a shared abstraction for ecosystem-specific compatibility providers

## Example

```rust
use monochange_core::BumpSeverity;
use monochange_semver::direct_release_severity;
use monochange_semver::merge_severities;

let merged = merge_severities(BumpSeverity::Patch, BumpSeverity::Minor);
let direct = direct_release_severity(Some(BumpSeverity::Minor), None);

assert_eq!(merged, BumpSeverity::Minor);
assert_eq!(direct, BumpSeverity::Minor);
```

<!-- {/monochangeSemverCrateDocs} -->
