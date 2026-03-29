---
monochange: patch
monochange_config: patch
monochange_core: patch
---

#### support changelog defaults and package-level changelog policies

Allow package `changelog` values in `monochange.toml` to be configured as `true`, `false`, or an explicit string path. `true` resolves to `{path}/CHANGELOG.md`, `false` disables changelog output for that package, and string values keep using the provided path. The `[defaults]` section now also supports `changelog` with the same boolean forms plus string patterns such as `"{path}/changelog.md"`, which are expanded for each declared package.
