---
monochange: minor
monochange_config: minor
monochange_core: minor
---

#### add structured changelog formats and shared release-note rendering

Add first-class changelog format support with `monochange` and `keep_a_changelog` renderers backed by a shared structured release-note model. Workspace defaults, packages, and groups can now configure changelog tables with explicit `path` and `format` settings while preserving legacy boolean and string changelog forms.

Release preparation now renders changelog files through the shared release-note model instead of appending raw markdown directly. The update also expands doctests, configuration coverage, and end-to-end integration tests for default and overridden changelog formats.
