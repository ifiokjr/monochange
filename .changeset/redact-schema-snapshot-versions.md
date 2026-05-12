---
monochange_test_helpers: test
---

# Redact schema versions in snapshot helpers

Shared snapshot settings now redact release-record schema versions in JSON fields and diagnostics so release PR tests do not fail when the schema crate version changes.
