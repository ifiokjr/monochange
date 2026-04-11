---
monochange: patch
monochange_cargo: patch
monochange_core: patch
---

`versioned_files` typed manifest entries now update arbitrary TOML and JSON string fields in addition to dependency sections.

Before:

```toml
versioned_files = [
	{ path = "Cargo.toml", type = "cargo", fields = ["workspace.package.version", "workspace.metadata.bin.monochange.version"], prefix = "" },
]
```

Only `workspace.package.version` changed during release preparation.

After:

```toml
versioned_files = [
	{ path = "Cargo.toml", type = "cargo", fields = ["workspace.package.version", "workspace.metadata.bin.monochange.version"], prefix = "" },
]
```

Both `workspace.package.version` and `workspace.metadata.bin.monochange.version` update to the planned release version.

JSON manifests now support the same style of field path updates, for example:

```toml
versioned_files = [
	{ path = "package.json", type = "npm", fields = ["metadata.bin.monochange.version"] },
]
```
