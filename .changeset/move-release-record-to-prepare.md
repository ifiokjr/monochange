---
monochange: minor
monochange_core: minor
monochange_github: patch
monochange_hosting: patch
---

# Move release record generation from commit_release to prepare_release

The release record JSON is now written during the `PrepareRelease` CLI step instead of the `CommitRelease` step. This gives users a formatting preview and the opportunity to review or edit the record before it is committed.

## What changed

- `ReleaseManifest` and `PreparedRelease` no longer store a `release_record_path` field. The path is derived on demand via the new `ReleasePaths` helper, which computes the hash, relative path, and absolute path from the manifest's `release_targets`.
- A new `ReleasePaths` runtime helper provides `hash`, `relative`, and `absolute` paths for any release record. Steps that need the path can call `ReleasePaths::from_manifest` or `ReleasePaths::from_record` instead of reading a cached field.
- The `PrepareRelease` step calls `write_release_record_file` to write the record to `.monochange/releases/<hash>/release.json`. The file is left unstaged for user review.
- `commit_release` now validates the pre-written record with `validate_release_record_file` instead of generating it.
- `deduplicate_overlapping_release_records` is now cached per-process to avoid redundant filesystem scans when both `write_release_record_file` and `validate_release_record_file` run in the same CLI invocation.
- `git_stage_paths_command` adds `-f` so that ignored `.monochange/releases/` files can still be staged.
- `release_path_requires_staging` explicitly allows `.monochange/releases/` paths even when gitignored.
- `write_release_record_file` skips overwriting an existing record file so that subsequent `PrepareRelease` steps (for example during `mc release-pr`) do not dirty the working tree.
