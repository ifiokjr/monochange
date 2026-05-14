---
monochange_schema: fix
monochange: fix
---

# Fix publish failures from missing packaged source files

Remove `build.rs` from `monochange_schema` and replace the generated `CURRENT_SCHEMA_VERSION_TEXT` with a direct `include_str!` + `trim_ascii_end()` compile-time embed, eliminating the `OUT_DIR` dependency that caused `cargo publish` verification to fail.

Add `src/**/*.template` to `monochange`'s `package.include` list so that `monochange.toml.template` (referenced via `include_str!`) is included in the published crate tarball.
