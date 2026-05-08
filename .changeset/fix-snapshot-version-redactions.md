---
monochange_test_helpers: patch
monochange_core: patch
---

# Add schema version redaction to snapshot settings and release record tests

Stop hardcoding `monochange_schema` public schema version (`0.0`) in snapshot assertions and unit tests. Use insta redaction for the release record `"v"` wire-format field in multiline snapshots, and read the expected schema version from `monochange_schema::CURRENT_SCHEMA_VERSION_TEXT` at runtime in `monochange_core` unit tests.

This prevents failures after every release when the `monochange_schema` version bumps and `CURRENT_SCHEMA_VERSION_TEXT` changes.
