---
monochange_schema: patch
---

# harden release-record schema migrations

Release-record artifacts published with schema `0.0` are now treated as the legacy `v`-field shape and migrate through an explicit `0.0 -> 0.1 -> current` path. This keeps older commit-embedded records readable while preserving the future-version rejection behavior for artifacts newer than the current binary understands.

For example, callers can continue to load a legacy record like:

```json
{
	"v": "0.0",
	"kind": "monochange.releaseRecord",
	"createdAt": "2026-04-06T12:00:00Z",
	"command": "release-pr",
	"releaseTargets": [],
	"releasedPackages": [],
	"changedFiles": []
}
```

and receive the current `schemaVersion` field after migration. The release schema preflight now also checks committed schema assets, migration metadata, generated docs copies, and active schema changesets so release PRs fail before publishing inconsistent schema artifacts.
