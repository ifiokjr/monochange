# `monochange`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeCrateDocs} -->

`monochange` is the top-level entry point for the workspace.

Reach for this crate when you want one API and CLI surface that discovers packages across Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter workspaces, exposes top-level commands from `monochange.toml`, and runs configured release workflows from those definitions.

## Why use it?

- coordinate one workflow-defined CLI across several package ecosystems
- expose discovery, change creation, and release preparation as both commands and library calls
- connect configuration loading, package discovery, graph propagation, and semver evidence in one place

## Best for

- shipping the `mc` CLI in CI or local release tooling
- embedding the full end-to-end planner instead of wiring the lower-level crates together yourself
- generating starter config with `mc init` and then evolving the workflow surface over time

## Key commands

```bash
mc init
mc discover --format json
mc change --package crates/monochange --bump patch --reason "describe the change"
mc release --dry-run --format json
```

## Responsibilities

- aggregate all supported ecosystem adapters
- load `monochange.toml`
- synthesize default workflows when config does not declare any
- resolve change input files
- render discovery and release workflow output in text or JSON
- execute configured release workflows

<!-- {/monochangeCrateDocs} -->

<!-- {=monochangeBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange
[docs-image]: https://img.shields.io/badge/docs.rs-monochange-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange/

<!-- {/monochangeBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
