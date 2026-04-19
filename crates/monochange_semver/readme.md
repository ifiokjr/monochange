# `monochange_semver`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_semver"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange__semver-orange?logo=rust)](https://crates.io/crates/monochange_semver) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__semver-1f425f?logo=docs.rs)](https://docs.rs/monochange_semver/) [![CI](https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/ifiokjr/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg?flag=monochange_semver)](https://codecov.io/gh/ifiokjr/monochange?flag=monochange_semver) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeSemverCrateDocs} -->

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
