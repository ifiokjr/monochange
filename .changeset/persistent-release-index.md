---
monochange: minor
---

# Add persistent deduplication index and content-hash fast path for release records

Introduce a JSONL index at `.monochange/local/release-index.jsonl` that survives across CLI invocations, eliminating repeated directory scans when checking for duplicate release records. A fast path in `validate_release_record_file` now compares the `releaseTargets` identity of an existing file against the manifest before rebuilding the full `ReleaseRecord`, skipping unnecessary I/O when the targets match.
