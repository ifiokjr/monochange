---
monochange: patch
monochange_cargo: minor
monochange_publish: patch
---

# Move Cargo publish readiness blockers into monochange_cargo

Move `cargo_publish_readiness_blockers` and workspace package table helpers (`read_workspace_package_table`, `maybe_read_workspace_manifest_contents`, `parse_workspace_manifest_value`, `extract_workspace_package_table`) from the top-level `monochange` crate into `monochange_cargo`.

Also fixes a clippy `indexing_slicing` lint in `monochange_publish` that was introduced by the previous resume/dependency-ordering extraction.
