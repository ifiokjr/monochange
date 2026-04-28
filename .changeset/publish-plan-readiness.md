---
"monochange": minor
"monochange_core": patch
"@monochange/cli": patch
"@monochange/skill": patch
---

#### Add readiness-backed publish planning

`mc publish-plan` now accepts `--readiness <path>` for normal package publish planning. The plan validates that the `mc publish-readiness` artifact matches the current release record and covers the selected package set, then limits rate-limit batches to package ids that are ready in both the artifact and a fresh local readiness check.

Placeholder publish planning continues to reject readiness artifacts and should be run with `mc publish-plan --mode placeholder`.
