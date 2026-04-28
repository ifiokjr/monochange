# `monochange`

<br />

<!-- {=crateReadmeBadgeRow:"monochange"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange-orange?logo=rust)](https://crates.io/crates/monochange) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange-1f425f?logo=docs.rs)](https://docs.rs/monochange/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange)](https://codecov.io/gh/monochange/monochange?flag=monochange) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeCrateDocs} -->

`monochange` is the top-level entry point for the workspace.

Reach for this crate when you want one API and CLI surface that discovers packages across Cargo, npm/pnpm/Bun, Deno, Dart/Flutter, and Python workspaces, exposes top-level commands from `monochange.toml`, and runs configured CLI commands from those definitions.

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
mc skill -a pi -y
mc discover --format json
mc change --package monochange --bump patch --reason "describe the change"
mc release --dry-run --format json
mc mcp
```

## Responsibilities

- aggregate all supported ecosystem adapters
- load `monochange.toml`
- load config-defined `[cli.*]` workflow commands from `monochange.toml`
- expose hardcoded binary commands such as `init`, `validate`, `check`, `analyze`, `mcp`, `help`, and `version`
- generate immutable `mc step:*` commands from the built-in step schemas
- resolve change input files
- render discovery and release command output in text or JSON
- execute configured workflow commands plus built-in MCP commands
- preview or publish provider releases from prepared release data
- evaluate pull-request changeset policy from CI-supplied changed paths and labels
- expose JSON-first MCP tools for assistant workflows

<!-- {/monochangeCrateDocs} -->
