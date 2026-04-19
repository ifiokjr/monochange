# `monochange_cargo`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

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

<!-- {=crateBadgeLinks:"monochange_cargo"} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__cargo-orange?logo=rust [crate-link]: https://crates.io/crates/monochange_cargo [docs-image]: https://img.shields.io/badge/docs.rs-monochange__cargo-1f425f?logo=docs.rs [docs-link]: https://docs.rs/monochange_cargo/ [coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg?flag=monochange_cargo [coverage-link]: https://codecov.io/gh/ifiokjr/monochange?flag=monochange_cargo

<!-- {/crateBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
