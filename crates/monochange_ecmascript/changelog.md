## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

### Added

#### improve npm and Deno semantic analysis with parser-backed JS/TS export extraction

`monochange_analyze_changes` and `monochange_validate_changeset` now use parser-backed JavaScript and TypeScript export extraction for npm and Deno packages instead of relying primarily on line-based export scanning.

This improves semantic diff accuracy for cases such as:

- multiline named export blocks
- namespace re-exports like `export * as toolkit from "./toolkit"`
- anonymous default exports
- more complex TypeScript and module export syntax

The MCP output shape stays the same, but the semantic evidence for npm and Deno packages is now more robust and closer to the actual module structure.

This work also extracts the shared JavaScript and TypeScript export-analysis logic into the new `monochange_ecmascript` crate, so npm and Deno keep their ecosystem-specific manifest analysis while reusing one parser-backed module analyzer.

The monochange skill documentation now also teaches the new-package rule: the first changeset for a newly introduced published package or crate should use a `major` bump for that new package entry.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #250](https://github.com/monochange/monochange/pull/250) _Introduced in:_ [`0dd8460`](https://github.com/monochange/monochange/commit/0dd846060614b2de9d3b2dfb5c1337075774b167) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Related issues:_ [#247](https://github.com/monochange/monochange/issues/247), [#249](https://github.com/monochange/monochange/issues/249)
