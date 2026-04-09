---
"monochange": patch
"monochange_cargo": patch
"monochange_core": patch
"monochange_dart": patch
"monochange_deno": patch
---

Preserve release-time manifest formatting across ecosystems instead of rewriting files through pretty-printers.

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
- `Cargo.lock` package versions without reformatting the rest of the lockfile

The same release flow now preserves existing formatting for non-TOML manifests too:

- `package.json`
- `deno.json` and `deno.jsonc`
- `pubspec.yaml`

Those files now keep their original spacing, ordering, and comments while only changing the targeted version string.

Keep-a-changelog release headings also stop double-wrapping markdown-linked titles. For example:

```md
## [0.1.0](https://github.com/ifiokjr/monochange/releases/tag/v0.1.0) (2026-04-09)
```

now renders without an extra outer pair of brackets.
