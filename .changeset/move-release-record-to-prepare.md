---
monochange: minor
monochange_core: minor
monochange_github: patch
monochange_hosting: patch
---

# Move release record generation from commit_release to prepare_release

The release record JSON is now written during the `PrepareRelease` CLI step instead of the `CommitRelease` step. This gives users a formatting preview and the opportunity to review or edit the record before it is committed.

## What changed

- `ReleaseManifest` and `PreparedRelease` gained an optional `release_record_path` field that tracks where the record was written.
- The `PrepareRelease` step calls `write_release_record_file` and stores the resulting path on both the prepared release and the manifest.
- `commit_release` now validates the pre-written record with `validate_release_record_file` instead of generating it.
- `git_stage_paths_command` adds `-f` so that ignored `.monochange/releases/` files can still be staged.
- `release_path_requires_staging` explicitly allows `.monochange/releases/` paths even when gitignored.
- `write_release_record_file` skips overwriting an existing record file so that subsequent `PrepareRelease` steps (for example during `mc release-pr`) do not dirty the working tree.
