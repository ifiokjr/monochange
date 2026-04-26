# `monochange_dart`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_dart"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange__dart-orange?logo=rust)](https://crates.io/crates/monochange_dart) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__dart-1f425f?logo=docs.rs)](https://docs.rs/monochange_dart/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_dart)](https://codecov.io/gh/monochange/monochange?flag=monochange_dart) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeDartCrateDocs} -->

`monochange_dart` discovers Dart and Flutter packages for the shared planner.

Reach for this crate when you need to scan `pubspec.yaml` files, expand Dart or Flutter workspaces, and normalize package metadata into `monochange_core` records.

## Why use it?

- cover both pure Dart and Flutter package layouts with one adapter
- normalize pubspec metadata and dependency edges for shared release planning
- detect Flutter packages without maintaining a separate discovery path

## Best for

- scanning Dart or Flutter monorepos into normalized workspace records
- reusing the same planning pipeline for mobile and non-mobile packages
- discovering Flutter packages without a dedicated Flutter-only adapter layer

## Public entry points

- `discover_dart_packages(root)` discovers Dart and Flutter workspaces plus standalone packages
- `DartAdapter` exposes the shared adapter interface

## Scope

- `pubspec.yaml` workspace expansion
- Dart package parsing
- Flutter package detection
- normalized dependency extraction

<!-- {/monochangeDartCrateDocs} -->
