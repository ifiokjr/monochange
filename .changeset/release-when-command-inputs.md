---
"monochange": patch
---

# Fix command input references in CLI step conditions

Allow `when` conditions to read command-level inputs while preserving step-level input overrides, so release automation can gate commit and pull request steps on `--commit` and `--create-pr`.
