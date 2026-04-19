# `monochange_graph`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeGraphCrateDocs} -->

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

<!-- {=crateBadgeLinks:"monochange_graph"} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__graph-orange?logo=rust [crate-link]: https://crates.io/crates/monochange_graph [docs-image]: https://img.shields.io/badge/docs.rs-monochange__graph-1f425f?logo=docs.rs [docs-link]: https://docs.rs/monochange_graph/ [coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg?flag=monochange_graph [coverage-link]: https://codecov.io/gh/ifiokjr/monochange?flag=monochange_graph

<!-- {/crateBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
