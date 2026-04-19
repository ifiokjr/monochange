# `monochange_npm`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeNpmCrateDocs} -->

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

<!-- {=crateBadgeLinks:"monochange_npm"} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__npm-orange?logo=rust [crate-link]: https://crates.io/crates/monochange_npm [docs-image]: https://img.shields.io/badge/docs.rs-monochange__npm-1f425f?logo=docs.rs [docs-link]: https://docs.rs/monochange_npm/ [coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg?flag=monochange_npm [coverage-link]: https://codecov.io/gh/ifiokjr/monochange?flag=monochange_npm

<!-- {/crateBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
