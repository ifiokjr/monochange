---
"monochange": patch
"monochange_core": patch
---

# Fix release record reformatting after dprint

Prevent `commit_release` from rewriting `release.json` after `dprint fmt` has already formatted it. The validation now compares parsed JSON values instead of raw strings, so formatting-only differences (such as tab vs space indentation) no longer trigger a rewrite.

The `CommitRelease` step now accepts an `update_release_json` input (default `false`). When `true`, the step will create or overwrite the `release.json` file if it is missing or mismatched. When `false`, a mismatch produces a clear error asking the user to set `update_release_json = true`.
