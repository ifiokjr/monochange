# Release PR workflow

This is a repo-shaped example for a long-running release PR branch flow where release files are reviewed before merge and tags are created only after the release commit lands on the default branch.

## What this example includes

- `monochange.toml` with GitHub source config and release-request settings
- a small Cargo workspace with one publishable package
- a sample `.changeset/*.md` file
- `.github/workflows/release.yml` showing the release-PR refresh and post-merge tagging pattern

## Recommended validation flow

```bash
mc validate
mc check
mc release --dry-run --diff
```

## Why this example is opinionated

- it is optimized for human review before release files land on `main`
- it keeps tag creation after merge, not on the release branch
- it separates release planning from downstream publish jobs
