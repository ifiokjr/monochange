---
"@monochange/skill": patch
monochange: minor
monochange_config: patch
monochange_core: minor
monochange_lint: patch
---

#### add lint include/exclude path filters with optional gitignore override

`[lints]` now supports workspace-level path filters for manifest linting.

**New options:**

- `include` — optional glob patterns that opt manifest paths into linting
- `exclude` — glob patterns that remove matching manifest paths from linting
- `disable_gitignore` — opt back into linting gitignored manifests when needed

By default, monochange now skips gitignored manifests during linting while still allowing repositories to explicitly opt them back in.

The repository config and bundled linting docs now show how to exclude `examples/**` when example manifests would otherwise trigger false positives.
