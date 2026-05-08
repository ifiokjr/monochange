---
monochange: breaking
monochange_core: minor
monochange_github: patch
---

# File-based release records

Store release records as committed JSON files under `.monochange/releases/` instead of embedding them in commit message bodies.

## Before

Release records were embedded inside the commit message body between HTML comment markers:

````markdown
chore(release): prepare release

## monochange Release Record

<!-- monochange:release-record:start -->

```json
{
	"schemaVersion": 1,
	"kind": "monochange.releaseRecord",
	"createdAt": "2026-05-08T08:00:00Z",
	"command": "release-pr",
	"releaseTargets": [
		{
			"id": "sdk",
			"kind": "group",
			"version": "1.2.3",
			"tag": true,
			"release": true
		}
	]
}
```
````

<!-- monochange:release-record:end -->

```
Discovery required parsing every commit message in first-parent ancestry with regex-based extraction.

## After

Release records are plain JSON files committed to the repository:
```

.monochange/ ├── local/ # gitignored — local artifacts │ ├── release-manifest.json │ └── prepared-release-cache.json └── releases/ └── <hash>/ # content-addressable directory └── release.json # the release record

```
The `<hash>` is derived from sorted `(package_id, version)` pairs via `DefaultHasher`. For a release targeting `sdk` at version `1.2.3` the hash might look like:
```

.monochange/releases/8f3e2a1b/c/release.json

````
(The exact hex value depends on the hasher state; it is always 16 hex characters.)

## Deduplication

When writing a new release record, any existing record that shares an overlapping `(package_id, version)` tag is automatically removed. This prevents stale records from accumulating when a release is retried or amended.

## Discovery

`mc release-record` now discovers files via `git diff-tree --no-commit-id --name-only -r` (falling back to `git ls-tree` for root commits) rather than parsing commit messages.

## CI detection

```bash
git diff-tree --no-commit-id --name-only -r HEAD |
  grep '^\.monochange/releases/.*/release\.json$'
````

## Breaking changes

- `.monochange/*` is no longer fully gitignored; only `.monochange/local/` is ignored.
- The `ReleaseRecord` JSON schema itself remains identical; only the storage location changes.
