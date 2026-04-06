---
monochange: minor
monochange_config: minor
monochange_core: minor
---

#### add structured step outputs and cross-step template interpolation

CLI steps now expose structured outputs that later steps can reference via Jinja template expressions.

**Command step `id` and output capture:**

- Command steps accept an optional `id` field. When set, stdout and stderr are captured and available to later steps via `{{ steps.<id>.stdout }}` and `{{ steps.<id>.stderr }}`.
- Duplicate `id` values within the same CLI command are rejected at config validation time.

**Structured `release.*` namespace:**

- After `PrepareRelease`, Command steps can reference `{{ release.version }}`, `{{ release.updated_changelogs }}`, `{{ release.changed_files }}`, `{{ release.released_packages }}`, `{{ release.targets }}`, and other release fields.
- Example: `dprint fmt {{ release.updated_changelogs | join(' ') }}` formats only the changelog files that were written during release preparation.

**Additional namespaces:**

- `{{ manifest.path }}` — path written by `RenderReleaseManifest`
- `{{ affected.status }}`, `{{ affected.summary }}` — from `AffectedPackages`

**`shell` accepts a string:**

- `shell = true` still means `sh -c` (backward compatible)
- `shell = "bash"` means `bash -c`, `shell = "zsh"` means `zsh -c`
- Omitting `shell` or `shell = false` keeps direct exec behavior

**Backward compatibility:**

- Legacy flat variables (`{{ version }}`, `{{ changed_files }}`, etc.) remain available alongside the new structured namespaces.
