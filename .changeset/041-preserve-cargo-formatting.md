---
"monochange": patch
"monochange_cargo": patch
"monochange_core": patch
"monochange_dart": patch
"monochange_deno": patch
"monochange_npm": patch
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

Cargo versioned-file updates also support nested field paths and `{{ name }}` expansion for workspace dependency entries. That means config like:

```toml
[package.monochange]
path = "crates/monochange"
versioned_files = ["Cargo.toml"]
```

continues to update the matching root `workspace.dependencies.monochange.version` entry automatically, and explicit typed entries can now target object-style fields such as:

```toml
{ path = "Cargo.toml", type = "cargo", fields = ["workspace.dependencies.{{ name }}.version"] }
```

The same release flow now preserves existing formatting for non-TOML manifests and npm-family lockfiles too:

- `package.json`
- `pnpm-lock.yaml`
- `deno.json` and `deno.jsonc`
- `pubspec.yaml`

Those files now keep their original spacing, ordering, and comments while only changing the targeted version string.

Keep-a-changelog release headings also stop double-wrapping markdown-linked titles. For example:

```md
## [0.1.0](https://github.com/ifiokjr/monochange/releases/tag/v0.1.0) (2026-04-09)
```

now renders without an extra outer pair of brackets.
