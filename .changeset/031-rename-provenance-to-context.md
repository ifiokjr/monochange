---
main: patch
---

#### rename `provenance` to `context` throughout the codebase

Replace all uses of the word "provenance" with "context" across Rust types, test file names, fixture directories, documentation, and templates.

- `ChangesetProvenance` type alias removed; `ChangesetContext` is the canonical struct name in `monochange_core`
- `crates/monochange/tests/changeset_provenance.rs` renamed to `changeset_context.rs`
- `fixtures/tests/changeset-provenance/` renamed to `fixtures/tests/changeset-context/`
- All doc strings, help text, guide pages, and `monochange.toml` comments updated
