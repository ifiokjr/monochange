---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_lint: patch
---

# replace changelog_sections with [changelog] section

The `changelog_sections` array-based config and `[release_notes]` section have been replaced with a new top-level `[changelog]` section that separates concerns into types (changeset type → section + bump routing) and sections (display headings with priorities).

**Before** (`[release_notes]` and `changelog_sections`):

```toml
[release_notes]
change_templates = ["- {{ summary }}"]

[package.core]
changelog_sections = [
	{ name = "Security", types = ["security"], bump = "patch" },
]
```

**After** (`[changelog]` with sections and types):

```toml
[changelog]
templates = ["- {{ summary }}"]

[changelog.sections.security]
heading = "Security"
priority = 40

[changelog.types.security]
section = "security"
bump = "patch"

[package.core]
excluded_changelog_types = [] # filter inherited types instead of overriding
```

Key changes:

- `[release_notes]` → `[changelog]` with `templates` instead of `change_templates`
- `changelog_sections` arrays → `[changelog.sections]` (keyed map) + `[changelog.types]` (keyed map)
- Per-package/group overrides → `excluded_changelog_types` (filters workspace defaults instead of complete replacement)
- Type keys must be lowercase, start with a letter, and contain at most one underscore
- Built-in defaults provide 9 sections and 12 types when `[changelog]` is empty
- Section priority ordering (lower = first) replaces positional array ordering
