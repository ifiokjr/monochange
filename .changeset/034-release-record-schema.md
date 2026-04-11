---
monochange_core: minor
---

#### add commit-embedded ReleaseRecord schema helpers

`monochange_core` now exposes a first-class `ReleaseRecord` contract for storing release declarations in commit bodies.

Before, downstream tooling only had the runtime `ReleaseManifest` types:

```rust
use monochange_core::ReleaseManifest;
```

After, downstream tooling can also work with durable commit-history records:

```rust
use monochange_core::ReleaseRecord;
use monochange_core::parse_release_record_block;
use monochange_core::render_release_record_block;
```

This release-record contract includes:

- `ReleaseRecord`, `ReleaseRecordTarget`, and `ReleaseRecordProvider`
- reserved commit-body markers such as `RELEASE_RECORD_START_MARKER`
- parsing and rendering helpers for the fenced JSON commit-body block
- validation errors for missing blocks, malformed payloads, unsupported `kind`, and unsupported `schemaVersion`
