---
"feat: file-based release records"
---

Store release records as committed JSON files under `.monochange/releases/` instead of embedding them in commit message bodies.

- Records live at `.monochange/releases/<hash>/release.json` where `<hash>` is derived from sorted `package_id:version` pairs.
- Local artifacts (manifest, cache) are written to `.monochange/local/`.
- `mc release-record` discovers files via `git ls-tree`/`git diff-tree` instead of parsing commit messages.
- The `ReleaseRecord` JSON schema remains identical; only the storage location changes.
- CI detection uses `git diff-tree --no-commit-id --name-only -r` on `.monochange/releases/` paths.
- Root commit handling falls back to `git ls-tree` when `diff-tree` cannot compare against a parent.
