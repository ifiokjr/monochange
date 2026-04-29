# `monochange_go`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_go"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange**go-orange?logo=rust)](https://crates.io/crates/monochange_go) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange**go-1f425f?logo=docs.rs)](https://docs.rs/monochange_go/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_go)](https://codecov.io/gh/monochange/monochange?flag=monochange_go) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeGoCrateDocs} -->

`monochange_go` discovers Go modules for the shared planner.

Reach for this crate when you need to scan standalone `go.mod` files, parse module metadata and `require` dependencies, infer `go mod tidy`, and preserve Go's tag-based versioning model in mixed-language release plans.

## Why use it?

- discover root and multi-module Go repositories through `go.mod` manifests
- normalize Go module paths and `require` edges for shared release planning
- update internal dependency requirements without treating `go.sum` as a lockfile
- model Go releases as VCS tags, including path-prefixed tags for submodules

## Best for

- scanning Go modules into normalized workspace records
- adding Go dependency edges to a mixed-language release plan
- refreshing `go.mod` and `go.sum` through `go mod tidy` after manifest rewrites
- publishing Go modules by creating tags such as `v1.2.3` or `api/v1.2.3`

## Public entry points

- `discover_go_modules(root)` discovers `go.mod` modules under a repository root
- `parse_go_module(path, root)` parses one module manifest into package metadata
- `update_go_mod_text(contents, dependencies)` rewrites matching `require` directives
- `discover_lockfiles(package)` reports Go checksum artifacts for command-based refreshes

## Scope

- `go.mod` module directive parsing
- direct and grouped `require` dependency extraction
- Go semantic import version handling through module paths
- command inference for `go mod tidy`
- metadata used by publishing for Go proxy lookup and path-prefixed VCS tags

<!-- {/monochangeGoCrateDocs} -->
