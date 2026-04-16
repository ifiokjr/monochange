# Internal-only workspace

This is a repo-shaped example for teams that want discovery, changelogs, changesets, and linting without public package publishing.

## What this example includes

- `monochange.toml` with npm discovery, publishing disabled, and top-level `[lints]`
- a small private npm workspace with two internal packages
- a sample `.changeset/*.md` file
- no source-provider automation, because the focus is local validation and release planning discipline

## Recommended validation flow

```bash
mc validate
mc check
mc release --dry-run --diff
```

## Why this example is opinionated

- it keeps packages private to emphasize internal-only usage
- it demonstrates that monochange still adds value even when no public registry is involved
- it uses `[lints].use = ["npm/recommended"]` while keeping publishing disabled
