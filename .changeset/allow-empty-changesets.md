---
monochange: minor
monochange_core: minor
---

# Allow `PrepareRelease` to succeed with no changesets

`PrepareRelease` steps in CLI commands now support an `allow_empty_changesets` option. When enabled, missing or empty `.changeset` directories no longer cause errors — instead the step succeeds with an empty release plan, and downstream steps can gate on `number_of_changesets` in their `when` conditions.

This enables `[cli.release-pr]` workflows to run against repos with no pending changes without failing the CI job.
