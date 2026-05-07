# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.5.1](https://github.com/monochange/monochange/releases/tag/v0.5.1) (2026-05-07)

### Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.5.1 as part of group `main`.

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-07)

### Breaking Change

#### Extract publish support into a dedicated crate

Move the publish support surface out of the top-level `monochange` crate and into the new `monochange_publish` crate. The extracted crate now owns the publish report/request models, trusted-publishing capability detection, provider/registry capability messages, and built-in publish command builders for npm, pnpm, Cargo, Dart, Flutter, JSR, PyPI, and Go proxy releases.

This keeps `monochange` focused on orchestration while giving publish integrations a dedicated crate boundary for future registry checks, readiness logic, and provider-specific publishing workflows.

```text
monochange_publish owns reusable publish capabilities and command construction.
monochange wires those capabilities into CLI workflows and release orchestration.
```

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #397](https://github.com/monochange/monochange/pull/397) _Introduced in:_ [`fa78e4d`](https://github.com/monochange/monochange/commit/fa78e4db56fd3a6897896c6e1b1c62ea2d8e46b9)
