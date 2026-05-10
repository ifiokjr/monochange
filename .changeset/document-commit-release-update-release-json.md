---
monochange: docs
monochange_schema: docs
---

# Document `CommitRelease.update_release_json` option

Add comprehensive documentation for the `update_release_json` step-level input on `CommitRelease`:

- Document the input in the `CommitRelease` CLI step reference with type, default, and description
- Explain semantic JSON comparison (formatting-only differences such as indentation or key ordering are ignored)
- Add a new composition example showing how to combine `dprint fmt` formatting with `CommitRelease` using `update_release_json = true`
- Add a new common-mistake entry about running formatters between `PrepareRelease` and `CommitRelease` without setting the input
- Document the field in the configuration guide's workflow variables section
- Regenerate JSON Schema assets to include the new `update_release_json` field in `CommitRelease` step definitions
