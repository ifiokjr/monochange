---
monochange_analysis: minor
---

`monochange_analysis` can now return release-aware semantic analysis across three explicit frames:

- `release -> main`
- `main -> head`
- `release -> head`

This adds a first multi-frame API surface for issue #249, including explicit ref-based entry points plus automatic baseline resolution that uses the latest workspace-style release tag and the detected default branch.
