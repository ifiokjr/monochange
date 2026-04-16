# GitHub npm quickstart

This is a repo-shaped example for a greenfield npm or pnpm workspace that wants GitHub-hosted release automation, top-level `[lints]` configuration, and built-in npm publishing.

## What this example includes

- `monochange.toml` with GitHub source config, npm publishing defaults, changeset policy, and `[lints]`
- a small pnpm workspace with two public packages
- a sample `.changeset/*.md` file
- `.github/workflows/release.yml` for release-PR refresh and post-merge publish
- `.github/workflows/changeset-policy.yml` for pull-request policy checks

## File layout

```text
.github/workflows/
.changeset/
packages/
  shared/
  web/
monochange.toml
package.json
pnpm-workspace.yaml
```

## Recommended validation flow

From this directory, run:

```bash
mc validate
mc check
mc release --dry-run --diff
```

## Why this example is opinionated

- it uses `pnpm` so the workspace protocol story is explicit
- it enables built-in npm publishing because GitHub + npm is the most automated monochange publishing path today
- it uses `[lints].use = ["npm/recommended"]` and a small scoped override instead of hand-writing every lint rule
- it keeps publishing in CI, while local work stops at validation, linting, and dry-run release planning

## Notes

- the packages are intentionally small so the example stays readable
- the workflow files are examples, not production secrets-management advice
- placeholder publishing is not enabled here; see `../public-packages-placeholder-publish/` when name reservation matters
