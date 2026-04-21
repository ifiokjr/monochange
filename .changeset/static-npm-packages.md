---
"@monochange/cli": change
"@monochange/cli-darwin-arm64": change
"@monochange/cli-darwin-x64": change
"@monochange/cli-linux-arm64-gnu": change
"@monochange/cli-linux-arm64-musl": change
"@monochange/cli-linux-x64-gnu": change
"@monochange/cli-linux-x64-musl": change
"@monochange/cli-win32-x64-msvc": change
"@monochange/cli-win32-arm64-msvc": change
"@monochange/skill": change
---

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
