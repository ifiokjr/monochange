---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_github: minor
monochange_cargo: minor
monochange_graph: minor
---

#### add github release payload rendering and customizable release notes

Add the `PublishGitHubRelease` workflow step and the `monochange_github` crate. Workflows can now publish GitHub releases from the prepared manifest:

```toml
[cli.release]
[[cli.release.steps]]
type = "PrepareRelease"

[[cli.release.steps]]
type = "PublishGitHubRelease"
```

```bash
mc release --dry-run --format json   # preview the GitHub release payloads
mc release                           # create/update releases via `gh`
```

Release-note bodies are customizable through `[release_notes]` in `monochange.toml`:

```toml
[release_notes]
[release_notes.change_templates]
feat = "### Features\n{{ notes }}"
fix = "### Fixes\n{{ notes }}"
```

Change files can now carry `type` and `details` fields so the same categorization drives both the changelog and the GitHub release body:

```markdown
---
core: minor
type: feat
details: |
  Long-form description that appears in the GitHub release body.
---

Short heading for the changelog.
```

Packages and groups can also declare `extra_changelog_sections` to inject additional sections.

**`monochange_github`** is a new crate that owns `GitHubConfiguration`, release-request building, and the `gh` CLI wrapper. **`monochange_core`** adds the `ChangeSignal.change_type` and `ChangeSignal.details` fields consumed by the renderer. **`monochange_graph`** propagates `source_path` through `ChangeSignal` so manifests can trace each change back to its `.changeset` file.
