# `monochange_core`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeCoreCrateDocs} -->

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

<!-- {=monochangeCoreBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__core-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_core
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__core-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_core/

<!-- {/monochangeCoreBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
