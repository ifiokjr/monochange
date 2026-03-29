# `monochange`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeCrateDocs} -->

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
