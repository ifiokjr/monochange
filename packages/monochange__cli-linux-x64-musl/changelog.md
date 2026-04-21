## [0.2.0](https://github.com/ifiokjr/monochange/releases/tag/v0.2.0) (2026-04-21)

### Changed

#### static npm packages in packages/ directory

All npm packages now live as static directories under `packages/` instead of being dynamically generated during the release workflow.

**Before:**

The `@monochange/cli` and platform packages were generated on-the-fly by `build-packages.mjs` into a temporary directory, then published from there. `@monochange/skill` lived in `npm/skill`.

**After:**

Package directories are permanently present under `packages/` using the `@scope__name` convention:

```
packages/monochange__cli/              # @monochange/cli
packages/monochange__cli-darwin-arm64/  # @monochange/cli-darwin-arm64
packages/monochange__skill/            # @monochange/skill
...
```

`build-packages.mjs` still runs during release to populate platform binaries into `packages/*/bin/`, but it no longer generates the package structure from scratch. `publish-packages.mjs` now validates that each package has the expected binaries before publishing, preventing accidental empty publishes.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #204](https://github.com/ifiokjr/monochange/pull/204) _Introduced in:_ [`a90638b`](https://github.com/ifiokjr/monochange/commit/a90638b911d0aca00afcda8c5686da46ead14831) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)
