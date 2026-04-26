# `monochange_cargo`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_cargo"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange__cargo-orange?logo=rust)](https://crates.io/crates/monochange_cargo) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__cargo-1f425f?logo=docs.rs)](https://docs.rs/monochange_cargo/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_cargo)](https://codecov.io/gh/monochange/monochange?flag=monochange_cargo) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeCargoCrateDocs} -->

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
