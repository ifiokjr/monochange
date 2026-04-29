---
monochange: patch
monochange_config: patch
monochange_core: major
monochange_gitea: test
monochange_github: test
monochange_gitlab: test
monochange_hosting: test
---

# Add changelog section thresholds

`monochange` changelog rendering can now hide or collapse sections based on each section's configured priority. This lets you keep high-signal sections expanded while moving low-priority notes into collapsible markdown blocks or omitting them entirely.

Add the new workspace setting under `[changelog.section_thresholds]`:

```toml
[changelog.section_thresholds]
collapse = 50
ignored = 100
```

With that configuration:

- sections with `priority < 50` stay fully expanded
- sections with `priority >= 50` render inside markdown `<details>` blocks
- sections with `priority > 100` are omitted from the rendered changelog

**Before:** every configured `changelog.sections` entry rendered normally once it had entries.

```toml
[changelog.sections]
feat = { heading = "Added", priority = 20 }
docs = { heading = "Documentation", priority = 40 }
other = { heading = "Other", priority = 50 }
```

```md
## 1.2.3

### Added

- ship a new release workflow

### Other

- internal cleanup note
```

**After:** lower-priority sections can collapse automatically.

```toml
[changelog.sections]
feat = { heading = "Added", priority = 20 }
docs = { heading = "Documentation", priority = 40 }
other = { heading = "Other", priority = 50 }

[changelog.section_thresholds]
collapse = 50
ignored = 100
```

```md
## 1.2.3

### Added

- ship a new release workflow

<details>
<summary><strong>Other</strong></summary>

- internal cleanup note

</details>
```

This release also updates the generated init config and workspace config annotations so the new thresholds are documented where `monochange.toml` is authored.

> **Breaking change for Rust library consumers** — `monochange_core::ReleaseNotesSection` and `monochange_core::ChangelogSettings` now carry the new changelog-threshold metadata, so manual struct literals must include the added fields.

**Before (`monochange_core`):**

```rust
ReleaseNotesSection {
    title: "Documentation".to_string(),
    entries: vec!["- update migration guide".to_string()],
}

ChangelogSettings {
    templates,
    sections,
    types,
}
```

**After:**

```rust
ReleaseNotesSection {
    title: "Documentation".to_string(),
    collapsed: true,
    entries: vec!["- update migration guide".to_string()],
}

ChangelogSettings {
    templates,
    sections,
    section_thresholds,
    types,
}
```
