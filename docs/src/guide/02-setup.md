# Setting up a project

Add a `monochange.toml` file at the repository root.

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true

[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk"]

[ecosystems.cargo]
enabled = true

[ecosystems.npm]
enabled = true

[ecosystems.deno]
enabled = true

[ecosystems.dart]
enabled = true
```

Then verify discovery:

```bash
mc workspace discover --root . --format json
```
