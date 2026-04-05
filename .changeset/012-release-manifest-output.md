---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_cargo: minor
monochange_github: minor
monochange_graph: minor
---

#### add stable release manifest output and workflow rendering

`mc release --format json` now returns a stable manifest contract instead of ad-hoc text. Downstream CI can parse this output reliably:

```bash
mc release --dry-run --format json | jq '.targets[].id'
# "core"
# "cli"
```

Workflows can also persist the manifest to disk for later steps using `RenderReleaseManifest`:

```toml
[cli.release]
[[cli.release.steps]]
type = "PrepareRelease"

[[cli.release.steps]]
type = "RenderReleaseManifest"
path = "release-manifest.json"
```

The manifest captures: release targets with their new versions and changelog payloads, the full release plan with bump decisions, changed file paths, deleted changeset paths, and (when applicable) GitHub release request payloads.

**`monochange_core`** exports `ReleaseManifest`, `ReleaseManifestTarget`, `ReleaseManifestPlan`, `ReleaseManifestPlanDecision`, `ReleaseManifestChangelog`, and `ReleaseManifestCompatibilityEvidence` — all serializable with `serde`. **`monochange_github`** uses the manifest to build GitHub release and pull-request payloads.
