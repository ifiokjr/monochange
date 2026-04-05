---
monochange: minor
monochange_core: minor
monochange_config: minor
---

#### replace legacy config with package/group release model

`monochange.toml` now uses explicit `[package.<id>]` and `[group.<id>]` declarations instead of the legacy `version_groups` and `package_overrides` arrays:

```toml
# before (legacy – no longer supported)
[[version_groups]]
name = "sdk"
packages = ["cargo:core", "npm:web"]

# after
[package.core]
path = "crates/core"
type = "cargo"

[group.sdk]
packages = ["core", "web"]
```

`mc check` was added to validate configuration and changesets against the declared package and group ids before any release steps run:

```bash
mc check
# error: changeset targets unknown package "crates/core" - use package id "core" instead
```

Changesets must now target a declared package id or group id. Group-owned releases carry the group identity through changelogs, versioned files, and release tags.

**`monochange_config`** gains the `PackageDefinition`, `GroupDefinition`, and source-span-aware diagnostics that power the validation. **`monochange_core`** gains the corresponding `PackageDefinition` and `GroupDefinition` types.
