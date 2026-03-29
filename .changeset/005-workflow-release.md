---
monochange: minor
monochange_core: minor
monochange_config: minor
monochange_cargo: minor
---

#### add workflow-driven release preparation

Introduce config-defined workflows and the first built-in release-preparation workflow for `monochange`. Repositories can now declare a `release` workflow in `monochange.toml` and run it directly through `mc release` or `monochange release`, including support for `--dry-run` execution.

The new workflow engine adds typed `PrepareRelease` and `Command` steps. The release preparation step discovers `.changeset/*.md`, computes the synced release plan, updates Cargo manifests and workspace versions, appends package changelog entries, and removes consumed changesets only after a successful run. Supporting docs and tests were updated so the workflow-driven flow is now the primary documented release path.
