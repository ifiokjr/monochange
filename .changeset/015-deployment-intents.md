---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_cargo: minor
monochange_github: minor
monochange_graph: minor
---

#### add deployment intent workflows and manifest support

> **Note:** This feature was later removed in `021-remove-deployments`. The `[[deployments]]` section and `Deploy` step are no longer available.

A `Deploy` workflow step and `[[deployments]]` configuration section were introduced to surface structured deployment intents through the release manifest, allowing downstream CI to orchestrate deployments without hardcoding provider logic into MonoChange.

```toml
[[deployments]]
name = "production"
environment = "prod"
trigger = "on_tag"
branch = "main"

[cli.release]
[[cli.release.steps]]
type = "PrepareRelease"

[[cli.release.steps]]
type = "Deploy"
name = "production"
```

```bash
mc release --dry-run --format json
# manifest includes:
# "deployments": [{ "name": "production", "environment": "prod", "trigger": "on_tag" }]
```

**`monochange_core`** added `DeploymentDefinition`, `DeploymentTrigger`, and `ReleaseDeploymentIntent`. **`monochange_config`** parsed `[[deployments]]` and validated trigger constraints. **`monochange_github`** and **`monochange_graph`** were updated to include deployment intents in release manifests.
