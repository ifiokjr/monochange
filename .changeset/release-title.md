---
main: minor
---

#### Add configurable `release_title` and `changelog_version_title` templates

Two new template fields control how release names and changelog version headings render.

**`release_title`** — plain text title for provider releases (GitHub, GitLab, Gitea).
**`changelog_version_title`** — markdown-capable title for changelog version headings.

Both are configurable at `[defaults]`, `[package.*]`, and `[group.*]` levels.

**Default `release_title`:**

- Primary versioning: `{{ version }} ({{ date }})`
- Namespaced versioning: `{{ id }} {{ version }} ({{ date }})`

**Default `changelog_version_title`:**

- Primary: `[{{ version }}]({{ tag_url }}) ({{ date }})` (linked when source configured)
- Namespaced: `{{ id }} [{{ version }}]({{ tag_url }}) ({{ date }})`

**Available template variables:**

| Variable | Description |
|---|---|
| `version` | Version string (`1.2.3`) |
| `id` | Package or group id |
| `date` | Release date (YYYY-MM-DD) |
| `time` | Time (HH:MM:SS) |
| `datetime` | Full ISO 8601 datetime |
| `changes_count` | Number of changesets |
| `tag_url` | Link to the tag on the source provider |
| `compare_url` | Link comparing this tag to the previous version's tag |

**Configuration example:**

```toml
[defaults]
release_title = "{{ version }} ({{ date }})"
changelog_version_title = "[{{ version }}]({{ tag_url }}) ({{ date }})"

[group.main]
release_title = "v{{ version }} — released {{ date }}"
```

> **Breaking change** — changelog version headings now include the release date
> by default (e.g. `## 1.2.3 (2026-04-06)` instead of `## 1.2.3`). Namespaced
> packages also include the package/group id prefix. To restore the previous
> format, set `changelog_version_title = "{{ version }}"` in `[defaults]`.
