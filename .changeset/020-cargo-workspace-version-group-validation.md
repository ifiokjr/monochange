---
main: minor
---

#### validate that cargo workspace-versioned packages share the same group

`mc validate` now checks that all Cargo packages using `version.workspace = true` within the same Cargo workspace are assigned to the same version group. This prevents configuration mistakes where workspace-versioned packages are split across different groups or left ungrouped, which would cause version drift since they share a single `[workspace.package].version` field.

The cargo adapter now marks discovered packages with `uses_workspace_version` metadata when they inherit their version from the workspace root, enabling the validation step to identify them without re-parsing manifests.
