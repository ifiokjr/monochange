# monochange examples

This folder is the top-level index for repo-shaped monochange example setups.

Use these examples when you want something larger and more concrete than the bundled skill examples in `packages/monochange__skill/examples/`. The goal is to give users and agents a place they can be pointed to for realistic setup patterns, migration shapes, and CI layouts.

## Design goals

- cover both greenfield and migration adoption
- show GitHub and GitLab release flows side by side
- separate release planning from package publishing and hosted/provider releases
- make builtin vs external publishing recommendations explicit
- keep examples small enough to understand, but structured enough to test end to end over time

## Current example tracks

- [github-npm-quickstart](./github-npm-quickstart/README.md) — greenfield GitHub Actions setup for npm or pnpm workspaces
- [github-cargo-quickstart](./github-cargo-quickstart/README.md) — greenfield GitHub Actions setup for Cargo workspaces
- [mixed-workspace](./mixed-workspace/README.md) — cross-ecosystem monorepo with mixed package types and shared discovery concerns
- [gitlab-migration](./gitlab-migration/README.md) — adopting monochange into an existing GitLab-based repository
- [public-packages-placeholder-publish](./public-packages-placeholder-publish/README.md) — reserving package names before the real release flow is ready
- [internal-only-workspace](./internal-only-workspace/README.md) — release planning, linting, and changesets without public package publishing
- [release-pr-workflow](./release-pr-workflow/README.md) — long-running release PR branch automation
- [publishing-test-lab](./publishing-test-lab/README.md) — follow-up plan for real publishing verification outside this repository

## Validation policy

The long-term intent is:

- config, discovery, linting, and dry-run release flows should be validated end to end inside this repository
- real registry publishing should be validated in a separate test repository so package names, auth, and rate limits can be managed safely
- examples in this folder should grow into repo-shaped test fixtures as the product surface stabilizes

Today:

- `github-cargo-quickstart`, `github-npm-quickstart`, `gitlab-migration`, `internal-only-workspace`, `mixed-workspace`, `public-packages-placeholder-publish`, and `release-pr-workflow` are repo-shaped examples with actual workspace and config files
- `publishing-test-lab` remains an issue-driven planning directory because real publish verification belongs in a separate repository
- `./validate-examples.sh` runs `mc validate`, `mc check`, and `mc release --dry-run --diff` against every repo-shaped example in this folder

See also [publishing-test-lab/ISSUE.md](./publishing-test-lab/ISSUE.md) for a draft GitHub issue that scopes the external publishing-verification work.

## Publishing test-lab recommendation

Publishing is the one area that should not rely only on local fixtures.

Recommended follow-up:

1. create a dedicated external test repository for publish verification
2. mirror changes from monochange into that repository on a controlled cadence
3. use isolated namespaces, throwaway package names, or sandbox registries where the ecosystem supports them
4. explicitly test rate limits, delayed publish windows, and multi-package ordering behavior
5. keep production package names and production registries out of the test loop whenever possible

Where ecosystems differ, prefer the safest option available:

- use alternate or sandbox registries when they exist
- otherwise use dedicated public test namespaces that are clearly non-production
- for ecosystems with hard global naming constraints, keep a stable set of dedicated test packages and document ownership clearly
