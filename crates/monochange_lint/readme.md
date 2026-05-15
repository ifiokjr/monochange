# `monochange_lint`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_lint"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange**lint-orange?logo=rust)](https://crates.io/crates/monochange_lint) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange**lint-1f425f?logo=docs.rs)](https://docs.rs/monochange_lint/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_lint)](https://codecov.io/gh/monochange/monochange?flag=monochange_lint) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeLintCrateDocs} -->

`monochange_lint` runs ecosystem-agnostic manifest lint suites for `monochange`.

Reach for this crate when you want to validate workspace manifests against configurable rules, discover preset and custom lint suites, or apply autofixes across packages.

## Why use it?

- centralize lint suite registration and execution in one engine
- merge workspace-scoped and package-scoped `[lints]` configuration
- run all registered suites in one pass instead of wiring each crate separately

## Best for

- enforcing manifest quality checks across multi-ecosystem monorepos
- building custom lint suites that plug into the shared lint pipeline
- applying autofixes for common manifest problems in CI

## Public entry points

- `lint_workspace(root, config)` runs all registered lint suites against the workspace
- `discover_lint_suites()` lists available preset and custom suites
- `apply_autofixes(root, diagnostics)` applies suggested fixes for reported diagnostics

## Scope

- lint suite registration and discovery
- workspace-wide and scoped configuration merging
- autofix application
- ecosystem-agnostic rule dispatch

<!-- {/monochangeLintCrateDocs} -->
