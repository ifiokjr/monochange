---
monochange: patch
---

Eliminate full workspace copy during release. Dry-run now skips lockfile command execution entirely. Non-dry-run applies version updates and runs lockfile commands in-place, snapshotting only lockfile directories for change detection. Reduces `mc release --dry-run` from 2+ minutes to seconds.
