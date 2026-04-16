# `monochange_deno`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeDenoCrateDocs} -->

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

<!-- {=crateBadgeLinks:"monochange_deno"} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__deno-orange?logo=rust [crate-link]: https://crates.io/crates/monochange_deno [docs-image]: https://img.shields.io/badge/docs.rs-monochange__deno-1f425f?logo=docs.rs [docs-link]: https://docs.rs/monochange_deno/

<!-- {/crateBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
