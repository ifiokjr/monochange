---
"monochange": patch
"monochange_core": patch
"monochange_schema": major
---

# keep release-record discovery compatible across schema upgrades

Merged release commits that embed an older public release-record `schemaVersion` can now be read by newer monochange binaries. This lets commands such as:

```bash
mc step:release-record --from HEAD --format json
```

recognize an existing release commit after monochange itself has moved to a newer schema version, instead of reporting the older record as unsupported.

The GitHub Actions CI workflow now also includes a pull-request `release-records` preflight. For normal PRs, it creates the same local release commit used by the release test/lint preflights and verifies that `mc step:release-record` can read it and confirm that the generated commit is the resolved release-record commit.

Generated current and versioned release-record artifact fixtures are also checked into the schema crate. Schema checks ensure the current fixtures are regenerated during release planning, and integration tests load every checked-in release-record artifact through the real parser/migration path.
