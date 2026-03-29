---
monochange: patch
monochange_graph: patch
---

#### add CLI snapshots for dependency propagation and grouped release behavior

Add targeted unit and integration coverage for release propagation and grouped releases. The CLI test suite now uses `insta-cmd` snapshots to verify that dependents receive patch bumps when a changed package propagates upward, grouped releases emit shared changelogs, packages without changelogs are skipped, grouped versions are managed by the group release target, and invalid multi-group membership fails validation.
