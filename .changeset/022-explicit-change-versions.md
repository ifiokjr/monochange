---
monochange: minor
monochange_core: minor
monochange_config: minor
monochange_graph: minor
monochange_cargo: minor
---

#### support explicit versions in changesets

Allow changesets to pin an explicit `version`, add `mc change --version`, propagate grouped package pins to the owning version group, and make conflicting explicit versions pick the highest semver by default with an optional strict failure mode via `defaults.strict_version_conflicts`.
