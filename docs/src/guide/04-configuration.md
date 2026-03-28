# Configuration

Repository configuration lives in `monochange.toml`.

## Defaults

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true
```

## Version groups

```toml
[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk", "packages/mobile-sdk"]
```

## Ecosystem switches

```toml
[ecosystems.cargo]
enabled = true

[ecosystems.npm]
enabled = true
roots = ["packages/*"]

[ecosystems.deno]
enabled = true

[ecosystems.dart]
enabled = true
```

Package references may use package ids, package names, manifest-relative paths, or manifest-directory paths.
