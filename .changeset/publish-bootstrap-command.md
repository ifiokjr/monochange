---
"monochange": minor
"@monochange/cli": patch
"@monochange/skill": patch
---

Add `mc publish-bootstrap --from <ref>` for release-record-scoped first-time package setup.

The command uses the release record to choose package ids, runs placeholder publishing for that release package set, supports `--dry-run`, and can write a JSON bootstrap result artifact with `--output <path>`. Documentation now recommends rerunning `mc publish-readiness` after bootstrap before planning or publishing packages.
