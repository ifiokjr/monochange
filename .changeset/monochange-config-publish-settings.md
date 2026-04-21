---
monochange_config: feat
---

#### add publish configuration parsing for registries, trust, and placeholders

`monochange_config` now understands package and ecosystem publish settings, so registry publication can be configured in `monochange.toml` instead of being bolted on outside the workspace model.

**Before (`monochange.toml`):**

```toml
[package.core]
path = "crates/core"
type = "cargo"
```

**After:**

```toml
[ecosystems.cargo.publish]
mode = "builtin"
trusted_publishing = true

[package.core]
path = "crates/core"
type = "cargo"

[package.core.publish.placeholder]
readme_file = "docs/core-placeholder.md"
```

New parsed configuration includes:

- `publish.enabled`
- `publish.mode`
- `publish.registry`
- `publish.trusted_publishing`
- `publish.placeholder.readme`
- `publish.placeholder.readme_file`

Validation now also rejects ambiguous or unsupported publish configuration.

**Before:**

```toml
[package.core.publish.placeholder]
readme = "placeholder"
readme_file = "docs/core-placeholder.md"
```

This combination was not modeled directly.

**After:**

```text
package `core` publish.placeholder cannot set both `readme` and `readme_file`
```

Built-in publish mode also rejects custom/private registry overrides and tells callers to switch that package to `mode = "external"` instead.
