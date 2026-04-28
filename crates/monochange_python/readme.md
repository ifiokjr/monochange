# `monochange_python`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_python"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange__python-orange?logo=rust)](https://crates.io/crates/monochange_python) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__python-1f425f?logo=docs.rs)](https://docs.rs/monochange_python/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_python)](https://codecov.io/gh/monochange/monochange?flag=monochange_python) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangePythonCrateDocs} -->

`monochange_python` discovers Python packages for the shared planner.

Reach for this crate when you need to scan uv workspaces, Poetry projects, and standalone `pyproject.toml` packages, then normalize package metadata and dependency edges into `monochange_core` records.

## Why use it?

- cover uv workspaces, Poetry projects, and standalone PEP 621 packages with one adapter
- normalize Python names and dependency edges for shared release planning
- infer package-manager lockfile refresh commands without directly mutating fragile lockfiles

## Best for

- scanning Python monorepos into normalized workspace records
- adding Python package versions and dependency edges to a mixed-language release plan
- refreshing `uv.lock` or `poetry.lock` through native package-manager commands after manifest updates

## Public entry points

- `discover_python_packages(root)` discovers uv workspace members plus standalone Python packages
- `PythonAdapter` exposes the shared adapter interface

## Scope

- uv workspace member expansion
- `pyproject.toml` parsing for PEP 621 `[project]` and Poetry `[tool.poetry]` metadata
- PEP 503-style dependency name normalization
- PEP 440 version parsing into the shared semantic-version model when possible
- dependency extraction from PEP 621 runtime and optional dependencies
- dependency extraction from Poetry runtime dependencies and dependency groups
- version and internal dependency rewrites for `pyproject.toml`
- lockfile command inference for `uv.lock` and `poetry.lock`

<!-- {/monochangePythonCrateDocs} -->
