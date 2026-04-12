---
monochange: patch
monochange_config: patch
---

Improve changeset authoring and validation ergonomics.

- quote generated changeset frontmatter keys when package or group ids contain YAML-sensitive characters such as `@` or `/`
- render validation errors with file, line, column, source snippets, and actionable fix hints for malformed changeset frontmatter
