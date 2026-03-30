---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_github: minor
---

#### add github release payload rendering and customizable release notes

Add typed GitHub release configuration, a dedicated `monochange_github` crate, and a `PublishGitHubRelease` workflow step that reuses prepared release manifests and shared changelog rendering. Dry-run workflows now preview grouped and package-owned GitHub releases as structured JSON, and live publication uses the `gh` CLI to create or update releases.

This update also adds configurable release-note templates through `[release_notes].change_templates`, per-package and per-group `extra_changelog_sections`, and optional change-file `type` / `details` fields so changelogs, release manifests, and GitHub release bodies stay aligned.

It also expands MDT-driven docs, doctests, and CLI integration coverage for GitHub release automation.
