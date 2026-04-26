# `monochange_analysis`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_analysis"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange__analysis-orange?logo=rust)](https://crates.io/crates/monochange_analysis) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__analysis-1f425f?logo=docs.rs)](https://docs.rs/monochange_analysis/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_analysis)](https://codecov.io/gh/monochange/monochange?flag=monochange_analysis) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeAnalysisCrateDocs} -->

`monochange_analysis` turns git diff context into artifact-aware changeset suggestions.

Reach for this crate when you want to classify changed packages as libraries, applications, CLI tools, or mixed artifacts and then extract the most user-facing parts of the diff.

## Why use it?

- convert raw changed files into package-centric semantic summaries
- use different heuristics for libraries, applications, and CLI tools
- reuse one analysis pipeline across CLI, MCP, and CI automation

## Best for

- suggesting changeset boundaries before writing `.changeset/*.md` files
- analyzing pull-request or branch diffs in assistant workflows
- experimenting with artifact-aware release note generation

## Public entry points

- `ChangeFrame::detect(root)` selects the git frame to analyze
- `detect_artifact_type(package_path)` classifies a package as a library, application, CLI tool, or mixed artifact
- `analyze_changes(root, frame, config)` returns package analyses and suggested changesets

## Scope

- git-aware frame detection
- artifact classification
- semantic diff extraction
- adaptive suggestion grouping

<!-- {/monochangeAnalysisCrateDocs} -->
