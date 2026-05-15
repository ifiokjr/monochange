# `monochange_test_helpers`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_test_helpers"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange**test**helpers-orange?logo=rust)](https://crates.io/crates/monochange_test_helpers) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange**test**helpers-1f425f?logo=docs.rs)](https://docs.rs/monochange_test_helpers/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_test_helpers)](https://codecov.io/gh/monochange/monochange?flag=monochange_test_helpers) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeTestHelpersCrateDocs} -->

`monochange_test_helpers` packages the shared fixture, snapshot, git, and RMCP helpers used across the workspace test suite.

Reach for this crate when you are writing integration or fixture-heavy tests that need scenario workspaces, command snapshots, or temporary git repositories.

## Why use it?

- keep tests focused on behavior instead of tempdir and setup boilerplate
- share consistent fixture loading across crates
- reuse snapshot and git helpers in integration suites

## Best for

- copying fixture workspaces into temp directories
- writing git-backed integration tests
- configuring `insta` snapshots and RMCP content assertions

## Public entry points

- `copy_directory` and `copy_directory_skip_git` clone fixture trees into temp workspaces
- `git`, `git_output`, and `git_output_trimmed` run test git commands
- `snapshot_settings()` configures shared snapshot behavior
- `fixture_path!`, `setup_fixture!`, and `setup_scenario_workspace!` locate and materialize test fixtures

<!-- {/monochangeTestHelpersCrateDocs} -->
