---
monochange: minor
monochange_config: minor
monochange_core: minor
---

#### add stable release manifest output and workflow rendering

Add a stable release-manifest JSON model that captures release targets, rendered changelog payloads, changed files, deleted changesets, and the synchronized release plan. The `release --format json` output now uses that shared manifest contract, and workflows can persist the same artifact with the new `RenderReleaseManifest` step.

This update also expands CLI snapshot coverage, configuration parsing coverage, and MDT-driven docs for manifest-oriented automation flows.
