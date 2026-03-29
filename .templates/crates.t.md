<!-- {@crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<!-- {@repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->

<!-- {@monochangeBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange
[docs-image]: https://img.shields.io/badge/docs.rs-monochange-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange/

<!-- {/monochangeBadgeLinks} -->

<!-- {@monochangeCoreBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__core-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_core
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__core-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_core/

<!-- {/monochangeCoreBadgeLinks} -->

<!-- {@monochangeCargoBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__cargo-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_cargo
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__cargo-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_cargo/

<!-- {/monochangeCargoBadgeLinks} -->

<!-- {@monochangeConfigBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__config-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_config
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__config-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_config/

<!-- {/monochangeConfigBadgeLinks} -->

<!-- {@monochangeGraphBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__graph-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_graph
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__graph-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_graph/

<!-- {/monochangeGraphBadgeLinks} -->

<!-- {@monochangeNpmBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__npm-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_npm
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__npm-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_npm/

<!-- {/monochangeNpmBadgeLinks} -->

<!-- {@monochangeDenoBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__deno-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_deno
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__deno-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_deno/

<!-- {/monochangeDenoBadgeLinks} -->

<!-- {@monochangeDartBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__dart-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_dart
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__dart-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_dart/

<!-- {/monochangeDartBadgeLinks} -->

<!-- {@monochangeSemverBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__semver-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_semver
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__semver-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_semver/

<!-- {/monochangeSemverBadgeLinks} -->

<!-- {@monochangeCrateDocs} -->

`monochange` is the top-level entry point for the workspace.

Reach for this crate when you want one API and CLI surface that can discover packages across Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter workspaces, turn explicit change files into a release plan, and run configured release workflows from that plan.

## Why use it?

- coordinate one release workflow across several package ecosystems
- expose discovery and release planning as either CLI commands or library calls
- connect configuration loading, package discovery, graph propagation, and semver evidence in one place

## Best for

- shipping the `mc` CLI in CI or local release tooling
- embedding the full end-to-end planner instead of wiring the lower-level crates together yourself
- rendering discovery or release-plan output in text or JSON

## Key commands

```bash
mc workspace discover --root . --format json
mc changes add --root . --package crates/monochange --bump patch --reason "describe the change"
mc plan release --root . --changes .changeset/1234567890-crates-monochange.md --format json
mc release --dry-run
```

## Responsibilities

- aggregate all supported ecosystem adapters
- load `monochange.toml`
- resolve change input files
- render discovery and release-plan output in text or JSON
- execute configured release workflows

<!-- {/monochangeCrateDocs} -->

<!-- {@monochangeCoreCrateDocs} -->

`monochange_core` is the shared vocabulary for the `monochange` workspace.

Reach for this crate when you are building ecosystem adapters, release planners, or custom automation and need one set of types for packages, dependency edges, version groups, change signals, and release plans.

## Why use it?

- avoid redefining package and release domain models in each crate
- share one error and result surface across discovery, planning, and workflow layers
- pass normalized workspace data between adapters and planners without extra translation

## Best for

- implementing new ecosystem adapters against the shared `EcosystemAdapter` contract
- moving normalized package or release data between crates without custom conversion code
- depending on the workspace domain model without pulling in discovery or planning behavior

## What it provides

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

`monochange_cargo` discovers Cargo packages and surfaces Rust-specific release evidence.

Reach for this crate when you want to scan Cargo workspaces into normalized `monochange_core` records and optionally feed Rust semver evidence into release planning.

## Why use it?

- discover Cargo workspaces and standalone crates with one adapter
- normalize crate manifests and dependency edges for the shared planner
- attach Rust semver evidence through `RustSemverProvider`

## Best for

- building Cargo-aware discovery flows without the full CLI
- feeding Rust semver evidence into release planning
- converting Cargo workspace structure into shared `monochange_core` records

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

`monochange_config` parses and validates the inputs that drive planning and release workflows.

Reach for this crate when you need to load `monochange.toml`, resolve package references, or turn `.changeset/*.md` files into validated change signals for the planner.

## Why use it?

- centralize config parsing and validation rules in one place
- resolve package references against discovered workspace packages
- keep workflow definitions, version groups, and change files aligned with the planner's expectations

## Best for

- validating configuration before handing it to planning code
- parsing and resolving change files in custom automation
- keeping package-reference rules consistent across tools

## Public entry points

- `load_workspace_configuration(root)` loads and validates `monochange.toml`
- `load_change_signals(root, changes_dir, packages)` parses markdown change files into change signals
- `resolve_package_reference(reference, workspace_root, packages)` maps package names, ids, and paths to discovered packages
- `apply_version_groups(packages, configuration)` attaches configured version groups to discovered packages

## Responsibilities

- load `monochange.toml`
- validate version groups and workflows
- resolve package references against discovered packages
- parse change-input files, evidence, and changelog overrides

<!-- {/monochangeConfigCrateDocs} -->

<!-- {@monochangeGraphCrateDocs} -->

`monochange_graph` turns normalized workspace data into release decisions.

Reach for this crate when you already have discovered packages, dependency edges, configuration, and change signals and need to calculate propagated bumps, synchronized version groups, and final release-plan output.

## Why use it?

- calculate release impact across direct and transitive dependents
- keep version groups synchronized during planning
- produce one deterministic release plan from normalized input data

## Best for

- embedding release-planning logic in custom automation or other tools
- computing the exact set of packages that need to move after a change
- separating planning logic from ecosystem-specific discovery code

## Public entry points

- `NormalizedGraph` builds adjacency and reverse-dependency views over package data
- `build_release_plan(workspace_root, packages, dependency_edges, defaults, version_groups, change_signals, providers)` computes the release plan

## Responsibilities

- build reverse dependency views
- propagate release impact across direct and transitive dependents
- synchronize version groups
- calculate planned group versions

<!-- {/monochangeGraphCrateDocs} -->

<!-- {@monochangeNpmCrateDocs} -->

`monochange_npm` discovers npm-family packages and normalizes them for shared planning.

Reach for this crate when you want one adapter for npm, pnpm, and Bun workspaces that emits `monochange_core` package and dependency records.

## Why use it?

- discover several JavaScript package-manager layouts with one crate
- normalize workspace metadata into the same graph used by the rest of `monochange`
- capture dependency edges from `package.json` and `pnpm-workspace.yaml`

## Best for

- scanning JavaScript or TypeScript monorepos into normalized package records
- supporting npm, pnpm, and Bun with one discovery surface
- feeding JS workspace topology into shared planning code

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

`monochange_deno` discovers Deno packages and workspace members for the shared planner.

Reach for this crate when you need to scan `deno.json` or `deno.jsonc` files, expand Deno workspaces, and normalize Deno dependencies into `monochange_core` records.

## Why use it?

- discover Deno workspaces and standalone packages with one adapter
- normalize manifest and dependency data for cross-ecosystem release planning
- include Deno-specific import and dependency extraction in the shared graph

## Best for

- scanning Deno repos without adopting the full workspace CLI
- turning `deno.json` metadata into shared package and dependency records
- mixing Deno packages into a broader cross-ecosystem monorepo plan

## Public entry points

- `discover_deno_packages(root)` discovers Deno workspaces and standalone packages
- `DenoAdapter` exposes the shared adapter interface

## Scope

- `deno.json` and `deno.jsonc`
- workspace glob expansion
- normalized dependency and import extraction

<!-- {/monochangeDenoCrateDocs} -->

<!-- {@monochangeDartCrateDocs} -->

`monochange_dart` discovers Dart and Flutter packages for the shared planner.

Reach for this crate when you need to scan `pubspec.yaml` files, expand Dart or Flutter workspaces, and normalize package metadata into `monochange_core` records.

## Why use it?

- cover both pure Dart and Flutter package layouts with one adapter
- normalize pubspec metadata and dependency edges for shared release planning
- detect Flutter packages without maintaining a separate discovery path

## Best for

- scanning Dart or Flutter monorepos into normalized workspace records
- reusing the same planning pipeline for mobile and non-mobile packages
- discovering Flutter packages without a dedicated Flutter-only adapter layer

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

`monochange_semver` merges requested bumps with compatibility evidence.

Reach for this crate when you need deterministic severity calculations for direct changes, propagated dependent changes, or ecosystem-specific compatibility providers.

## Why use it?

- combine manual change requests with provider-generated compatibility assessments
- share one bump-merging strategy across the workspace
- implement custom `CompatibilityProvider` integrations for ecosystem-specific evidence

## Best for

- computing release severities outside the full planner
- plugging ecosystem-specific compatibility logic into shared planning
- reusing the workspace's bump-merging rules in custom tools

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
