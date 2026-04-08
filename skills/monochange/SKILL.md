---
name: monochange
description: Guides agents through monochange discovery, changesets, release planning, and provider-aware release workflows. Use when working on `monochange.toml`, `.changeset/*.md`, release automation, grouped versions, or cross-ecosystem monorepo releases.
---

# monochange

## Quick start

1. Read `monochange.toml` first.
2. Run `mc validate` before making release-affecting edits.
3. Use `mc discover --format json` to inspect the workspace model.
4. Use `mc change` and `mc release --dry-run --format json` before mutating release state.

## Working rules

- Treat `monochange.toml` as the source of truth for packages, groups, source providers, and `[cli.<command>]` entries.
- Prefer configured package or group ids over guessing manifest names.
- Use `.changeset/*.md` files for explicit release intent.
- Run dry-run flows before real release commands.
- Keep docs, templates, and changelog behavior aligned with config changes.

## Release workflow

- Validate with `mc validate`.
- Inspect workspace state with `mc discover --format json`.
- Add or update changesets with `mc change`.
- Preview release effects with `mc release --dry-run --format json`.
- Use `mc release-manifest --dry-run` for downstream automation inputs.
- Only use source-provider publishing after reviewing prepared release data.

## Guidance

See [REFERENCE.md](REFERENCE.md) for install steps, command selection, grouped release rules, and assistant setup guidance.
