# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.4.1](https://github.com/monochange/monochange/releases/tag/v0.4.1) (2026-05-10)

### Fixed

#### Split crate boundaries for changelog, config, and publish behavior

Move changelog rendering into `monochange_changelog`, shift publish planning and execution helpers into `monochange_publish`, and reduce direct concrete ecosystem/provider dependencies in `monochange_config`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #441](https://github.com/monochange/monochange/pull/441) _Introduced in:_ [`ae8ea56`](https://github.com/monochange/monochange/commit/ae8ea563ae95c6cc4e8d3d1acdc5303069ea44cf)
