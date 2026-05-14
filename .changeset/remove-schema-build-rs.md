---
monochange_schema: patch
---

# Remove `build.rs` from `monochange_schema`

Replace the build.rs-generated `CURRENT_SCHEMA_VERSION_TEXT` with a direct `include_str!` + `trim_ascii_end()` compile-time embed, eliminating the OUT_DIR dependency that caused `cargo publish` verification to fail when `build.rs` was excluded from the crate package.
