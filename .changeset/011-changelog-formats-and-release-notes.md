---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_cargo: minor
monochange_github: minor
monochange_graph: minor
---

#### add structured changelog formats and shared release-note rendering

Changelogs can now use either the built-in `monochange` format or the `keep_a_changelog` format. The format is configured per-workspace default, per-package, or per-group:

```toml
[defaults.changelog]
format = "keep_a_changelog"

[package.core.changelog]
path = "crates/core/CHANGELOG.md"
format = "monochange" # overrides the default for this package
```

Both formats share the same structured release-note model, so GitHub release bodies and package changelogs stay aligned. The `extra_changelog_sections` key lets packages and groups inject additional sections into their changelog output.

`mc release` writes changelog entries through the new renderer. Legacy changelog config (`changelog = true`, `changelog = false`, `changelog = "path/to/CHANGELOG.md"`) is still accepted.

**`monochange_config`** exposes `ChangelogConfig` with `path` and `format` fields and the `ChangelogFormat` enum (`Monochange`, `KeepAChangelog`). **`monochange_core`** adds `ChangelogFormat` and the shared `ReleaseNoteRenderer` trait used by both format implementations. **`monochange_github`** uses the same renderer to build GitHub release bodies from the prepared manifest.
