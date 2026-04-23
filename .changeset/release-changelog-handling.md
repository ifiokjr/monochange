---
monochange: patch
---

#### ignore configured changelog files in affected-package verification and keep newest changelog entries first

Release automation now treats configured changelog targets as release metadata instead of as ordinary package source changes. That means changelog-only updates no longer make `mc affected --verify` fail with an uncovered package error, and newly generated release notes are inserted above older release headings so the latest release stays at the top of each changelog.

Configured changelog targets are unchanged:

```toml
[package.core.changelog]
path = "crates/core/changelog.md"
```

Command used by CI and local verification:

```bash
mc affected --format json --verify --changed-paths crates/core/changelog.md
```

**Before (output):**

```json
{
	"status": "failed",
	"affectedPackageIds": ["core"],
	"matchedPaths": ["crates/core/changelog.md"],
	"uncoveredPackageIds": ["core"]
}
```

**After (output):**

```json
{
	"status": "not_required",
	"affectedPackageIds": [],
	"ignoredPaths": ["crates/core/changelog.md"],
	"matchedPaths": [],
	"uncoveredPackageIds": []
}
```

Generated changelog sections also stay in reverse-chronological order:

```md
# Changelog

## [0.3.0] - 2026-04-23

- latest release notes

## [0.2.0] - 2026-03-01

- previous release notes
```
