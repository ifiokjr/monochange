---
"monochange": patch
"monochange_cargo": patch
"monochange_core": patch
---

Preserve Cargo TOML formatting during `mc release` instead of rewriting manifests into `toml::to_string_pretty(...)` output.

Before, releasing a workspace could rewrite files like this even when only version fields changed:

```toml
[dependencies]
rmcp = { workspace = true, features = ["server", "transport-io", "macros"], default-features = true }
```

became

```toml
[dependencies.rmcp]
default-features = true
features = [
    "server",
    "transport-io",
    "macros",
]
workspace = true
```

After this change, MonoChange updates only the relevant Cargo version values in place:

- `package.version`
- `workspace.package.version`
- dependency `version` fields in `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`
- matching `[workspace.dependencies]` entries for released workspace crates

Keep-a-changelog release headings also stop double-wrapping markdown-linked titles. For example:

```md
## [0.1.0](https://github.com/ifiokjr/monochange/releases/tag/v0.1.0) (2026-04-09)
```

now renders without an extra outer pair of brackets.
