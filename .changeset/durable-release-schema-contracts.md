---
monochange:
  bump: major
  type: major
monochange_core:
  bump: major
  type: major
monochange_schema:
  bump: none
  type: none
  caused_by: ["monochange"]
---

# Publish durable release schema contracts

Impact: release records now use the first public durable schema header, `v = "0.1"`, and monochange rejects missing, invalid, old, or future durable schema versions instead of reading unsafe historical shapes. The new `monochange_schema` crate owns schema version parsing, release-record wire validation, committed schema assets, and the initially empty machine-readable migration changelog.

Usage: editors can use the hosted configuration schema once GitHub Pages publishes the docs, or the raw GitHub fallback immediately. Durable release records now embed the public version field instead of the internal Rust-only `schemaVersion` field:

```json
{
  "v": "0.1",
  "kind": "monochange.releaseRecord"
}
```

The `monochange_schema` package remains independently versioned at `0.1.0`; this changeset covers the new crate without forcing a patch bump for its first public schema release.
