---
monochange: patch
monochange_config: patch
monochange_core: patch
monochange_github: patch
---

#### enable live GitHub automation workflows for this repository

The MonoChange repository now uses its own release tooling end-to-end. The `monochange.toml` was expanded with:

- a `verify` CLI command that runs `EnforceChangesetPolicy` on every pull request
- a `release-pr` CLI command that opens a versioned release pull request from CI
- a `release` CLI command that applies the prepared release on merge

The new `.github/workflows/changeset-policy.yml` workflow runs `mc verify` on every PR, passing changed files and labels as CLI inputs so the bot comment is updated automatically.

A dedicated GitHub automation guide was also added to the mdBook covering how to wire these workflows into a new repository.
