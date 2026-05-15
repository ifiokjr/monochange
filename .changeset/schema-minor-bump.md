---
monochange_schema: major
monochange_core: minor
monochange_config: minor
monochange_changelog: minor
monochange: minor
---

# Add configurable changelog rendering styles

Add configurable changelog and release-note rendering style options for section separators, package labels, metadata lines, and collapsed sections.

```toml
[changelog.style]
sectionSeparator = "blank_line"
packageLabelStyle = "inline"
packageLabelPlacement = "after_heading"
metadataStyle = "plain"
collapsedSectionStyle = "details"

[changelog.release_notes]
metadataStyle = "blockquote"
```

The config schema now includes `ChangelogStyle` and `ReleaseNotesStyleOverrides`, with release notes inheriting `[changelog.style]` unless a field-specific override is set.

Default section headings now include emoji in the `heading` string, while the stable section keys remain unchanged:

- `breaking`: `💥 Breaking Change`
- `feat`: `🚀 Feature`
- `change`: `📝 Changed`
- `fix`: `🐛 Fixed`
- `test`: `🧪 Testing`
- `refactor`: `🔨 Refactor`
- `docs`: `📖 Documentation`
- `security`: `🔒 Security`
- `perf`: `⚡ Performance`
- `none`: `🔖 None`

Semver level type aliases route to semantic sections: `major` to `breaking`, `minor` to `feat`, and `patch` to `fix`.
