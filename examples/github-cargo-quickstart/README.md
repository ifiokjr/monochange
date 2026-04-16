# GitHub Cargo quickstart

This is a repo-shaped example for a greenfield Cargo workspace that wants GitHub-hosted release automation, top-level `[lints]`, and registry-native Cargo publishing.

## What this example includes

- `monochange.toml` with GitHub source config, Cargo discovery, changeset policy, and `[lints]`
- a small Cargo workspace with a library crate and a CLI crate
- a sample `.changeset/*.md` file
- `.github/workflows/release.yml` for release-PR refresh and post-merge tagging
- `.github/workflows/publish-cargo.yml` for a registry-native external Cargo publish job

## Recommended validation flow

```bash
mc validate
mc check
mc release --dry-run --diff
```

## Why this example is opinionated

- it prefers `mode = "external"` so the official crates.io auth action can own token exchange
- it uses `[lints].use = ["cargo/recommended"]` plus a small override instead of a handwritten full rule table
- it keeps publish execution tag-driven and separate from release planning
