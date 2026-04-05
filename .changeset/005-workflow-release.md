---
monochange: minor
monochange_core: minor
monochange_config: minor
monochange_cargo: minor
monochange_dart: minor
monochange_deno: minor
monochange_graph: minor
monochange_npm: minor
monochange_semver: minor
---

#### add workflow-driven release preparation

Repositories can now declare a `release` workflow in `monochange.toml` and drive it entirely from `mc release`:

```toml
[cli.release]
[[cli.release.steps]]
type = "PrepareRelease"

[[cli.release.steps]]
type = "Command"
run = "cargo publish --workspace"
```

```bash
# preview what release preparation will do
mc release --dry-run

# apply the release: update manifests, write changelogs, delete consumed changesets
mc release
```

The `PrepareRelease` step discovers all `.changeset/*.md` files, computes the release plan (including cross-package propagation and version-group synchronization), updates every ecosystem manifest, appends changelog entries, and only removes consumed changeset files after all prior steps succeed.

**`monochange_core`** gains the `WorkflowDefinition`, `PrepareRelease`, and `Command` step types used to model these pipelines. **`monochange_config`** gains the parser and validator for `[cli.*]` workflow blocks.
