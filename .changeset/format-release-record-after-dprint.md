---
"monochange": patch
---

# Fix release record reformatting after dprint

Prevent `commit_release` from rewriting `release.json` after `dprint fmt` has already formatted it. The validation now compares parsed JSON values instead of raw strings, so formatting-only differences (such as tab vs space indentation) no longer trigger a rewrite.
