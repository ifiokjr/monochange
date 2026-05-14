---
monochange_schema: fix
monochange: fix
monochange_core: fix
monochange_github: fix
monochange_go: fix
---

# Fix publish failures and cargo package warnings

Remove `build.rs` from `monochange_schema` and replace the generated `CURRENT_SCHEMA_VERSION_TEXT` with a direct `include_str!` + `trim_ascii_end()` compile-time embed, eliminating the `OUT_DIR` dependency that caused `cargo publish` verification to fail.

Add `src/**/*.template` to `monochange`'s `package.include` list so that `monochange.toml.template` (referenced via `include_str!`) is included in the published crate tarball.

Add `tests/**/*.rs` to `monochange_core` and `monochange_github` package include lists to suppress cargo package "ignoring test" warnings.

Fix `monochange_go` readme from workspace-inherited path (`../../readme.md`) to local path (`readme.md`) to suppress the "readme outside package" cargo package warning.

Remove `doc-comment` dependency and replace `doc_comment::doctest!` with `#[doc = include_str!(...)]` in the book crate. Only keep local file references that resolve within the package directory.

Add rustdoc backticks around `PyPI` and `pub.dev` in configuration guide markdown to satisfy the missing-backticks lint.
