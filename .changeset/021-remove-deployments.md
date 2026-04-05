---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_gitea: minor
monochange_github: minor
monochange_gitlab: minor
---

#### remove deployments feature

The `[[deployments]]` configuration section, the `Deploy` workflow step, and all related types are removed. CI deployment orchestration belongs in native CI workflow triggers (e.g. GitHub Actions `workflow_run` or tag-push events), not in a release-planning tool.

**Before:**

```toml
[[deployments]]
name = "production"
trigger = "on_tag"

[[cli.release.steps]]
type = "Deploy"
name = "production"
```

**`mc release` would error** if a `Deploy` step referenced an undefined deployment, adding friction for teams that simply wanted `PrepareRelease` without deployment tracking.

**After:** Remove `[[deployments]]` and any `type = "Deploy"` steps from your config. The release manifest no longer includes a `deployments` array.

**Removed from `monochange_core`:** `DeploymentDefinition`, `DeploymentTrigger`, `ReleaseDeploymentIntent`, and the `Deploy` variant of `CliStepDefinition`. **`monochange_config`**, **`monochange_github`**, **`monochange_gitlab`**, and **`monochange_gitea`** no longer reference these types.
