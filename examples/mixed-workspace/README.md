# Mixed workspace

This is a repo-shaped example for a cross-ecosystem monorepo that manages Cargo and npm packages together under one monochange configuration.

## What this example includes

- `monochange.toml` with mixed package definitions, a shared release group, and top-level `[lints]`
- a Cargo crate and an npm package in the same repository
- a sample group-targeted `.changeset/*.md` file
- a private root `package.json` plus a Cargo workspace manifest

## Recommended validation flow

```bash
mc validate
mc check
mc release --dry-run --diff
```

## Why this example is opinionated

- it keeps the top-level repo private while still modeling public child packages
- it uses both `cargo/recommended` and `npm/recommended` presets from one shared `[lints]` section
- it uses a group to model one outward release identity across ecosystems
