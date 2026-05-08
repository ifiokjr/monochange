---
monochange: test
---

# Redact schema crate version in snapshot to survive release bumps

Stop hardcoding `monochange_schema` crate version in integration test assertions. Use insta redaction for `schemaCrateVersion` in the schema asset inventory snapshot, and read the expected version from the crate's `Cargo.toml` at runtime.
