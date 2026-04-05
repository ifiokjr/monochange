---
monochange: minor
monochange_cargo: minor
---

#### validate that cargo workspace-versioned packages share the same group

`mc validate` now errors when Cargo packages that share `version.workspace = true` are placed in different version groups or left ungrouped. All workspace-versioned packages must belong to the same group because they share a single `[workspace.package].version` field — putting them in separate groups would cause version drift.

```bash
mc validate
# error: cargo workspace-versioned packages must belong to the same group
#   "core" → group "sdk"
#   "cli"  → group "tools"   ← conflict
```

```toml
# correct – all workspace-versioned crates in the same group
[group.sdk]
packages = ["core", "cli"]
```

**`monochange_cargo`** marks each discovered package with `uses_workspace_version = true` when its `Cargo.toml` contains `version.workspace = true`. **`monochange`** (the top-level crate) reads this metadata during `mc validate` to identify the constraint violation without re-parsing manifests.
