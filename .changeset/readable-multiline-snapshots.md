---
monochange: test
monochange_schema: test
---

# Improve readability of multiline JSON snapshots

Redact multiline string fields inside JSON snapshots and assert their contents separately so release-planning test snapshots remain readable without escaped newline sequences.
