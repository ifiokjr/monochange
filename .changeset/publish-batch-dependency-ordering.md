---
"monochange": patch
"monochange_core": patch
"monochange_schema": patch
---

# Release PR formatting, schema version, and publish batch ordering

1. Format generated `.monochange/releases/` manifests via `dprint fmt` in `[cli.release-pr]`.
2. Derive expected schema versions in snapshots and tests from the actual `Cargo.toml` version instead of hardcoding `0.0`.
3. Topologically sort publish requests by both runtime and development dependencies before batching so dependencies are published before dependents.
