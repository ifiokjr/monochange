---
monochange: patch
---

# Remove indexing/slicing lint allowances

Remove crate-level `clippy::indexing_slicing` allowances and replace production indexing/slicing call sites with safe accessors.
