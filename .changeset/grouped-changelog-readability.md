---
monochange: patch
monochange_cargo: patch
monochange_config: patch
monochange_core: patch
monochange_graph: patch
---

#### improve grouped changelog readability

Grouped changelog entries now distinguish between members that directly changed and those that were synchronized because of group membership:

**Before:**

```markdown
## sdk 0.4.0

Members: core, app

- add streaming API (core)
```

**After:**

```markdown
## sdk 0.4.0

Changed members: core Synchronized members: app

- add streaming API (core)
```

Multi-package or multiline changeset sub-entries are wrapped in GitHub alert syntax so they visually separate from surrounding text in both the rendered changelog and the GitHub release body:

```markdown
> [!NOTE]
> **core, app** — add streaming API
>
> Full description of the change that spans multiple lines.
```

These changes affect the output of `mc release` (changelog files) and `mc release --dry-run --format json` (the `changelog_payload` field inside each `ReleaseManifestTarget`). No configuration changes are required.
