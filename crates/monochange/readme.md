# `monochange`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeCrateDocs} -->

`monochange` is the top-level entry point for the workspace.

Reach for this crate when you want one API and CLI surface that discovers packages across Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter workspaces, exposes top-level commands from `monochange.toml`, and runs configured CLI commands from those definitions.

## Why use it?

- coordinate one config-defined CLI across several package ecosystems
- expose discovery, change creation, and release preparation as both commands and library calls
- connect configuration loading, package discovery, graph propagation, and semver evidence in one place

## Best for

- shipping the `mc` CLI in CI or local release tooling
- embedding the full end-to-end planner instead of wiring the lower-level crates together yourself
- generating starter config with `mc init` and then evolving the CLI command surface over time

## Key commands

```bash
mc init
mc assist pi
mc discover --format json
mc change --package monochange --bump patch --reason "describe the change"
mc release --dry-run --format json
mc mcp
```

## Responsibilities

- aggregate all supported ecosystem adapters
- load `monochange.toml`
- start from the built-in default CLI commands and let matching config entries replace them
- resolve change input files
- render discovery and release command output in text or JSON
- execute configured CLI commands plus built-in assistant setup and MCP commands
- preview or publish provider releases from prepared release data
- evaluate pull-request changeset policy from CI-supplied changed paths and labels
- expose JSON-first MCP tools for assistant workflows

<!-- {/monochangeCrateDocs} -->

<!-- {=crateBadgeLinks:"monochange":"monochange"} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange-orange?logo=rust [crate-link]: https://crates.io/crates/monochange [docs-image]: https://img.shields.io/badge/docs.rs-monochange-1f425f?logo=docs.rs [docs-link]: https://docs.rs/monochange/

<!-- {/crateBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
