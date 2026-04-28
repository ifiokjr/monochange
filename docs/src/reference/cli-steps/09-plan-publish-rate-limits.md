# PlanPublishRateLimits

`PlanPublishRateLimits` inspects monochange's built-in ecosystem rate-limit catalog and renders a publish schedule before any registry mutation happens.

Use it when you want to answer questions like:

- how many package publishes fit in one registry window
- which packages should be split into later batches
- whether a filtered package set is safe to publish now
- what GitHub Actions or GitLab CI batch snippet should drive the publish run

## Inputs

- `format` — `text`, `markdown`, or `json`
- `mode` — `publish` (default) or `placeholder`
- `package` — optional repeated package ids used to filter the plan
- `readiness` — optional path to a JSON artifact from `mc publish-readiness`; only valid when `mode = "publish"`
- `ci` — optional `github-actions` or `gitlab-ci` snippet renderer

## Produces

A structured publish-rate-limit report containing:

- registry windows
- batch counts
- explicit package ids per batch
- evidence and confidence metadata for each built-in policy
- only versions that are still missing from their registries, so reruns reflect the remaining work
- when `readiness` is provided, only package ids ready in both the artifact and the fresh local readiness check

## Examples

### Plan a normal publish

```toml
[cli.publish-plan]
help_text = "Plan package-registry publish work against known ecosystem rate limits"

[[cli.publish-plan.inputs]]
name = "format"
type = "choice"
default = "json"
choices = ["text", "markdown", "json"]

[[cli.publish-plan.inputs]]
name = "readiness"
type = "path"
help_text = "JSON artifact from mc publish-readiness; limits publish plans to ready package work"

[[cli.publish-plan.steps]]
name = "plan publish rate limits"
type = "PlanPublishRateLimits"
```

A readiness-backed plan validates the artifact header, release record commit, selected package coverage, package-set fingerprint, and publish input fingerprint before planning. The artifact may contain non-ready packages, but those package ids are excluded from the plan. Rerun `mc publish-readiness` if workspace config, package manifests, lockfiles, or registry/tooling files changed after the artifact was written. Placeholder plans reject `readiness`; use `mode = "placeholder"` without an artifact for first-time bootstrap planning.

### Plan placeholder bootstrap publishing

```toml
[cli.placeholder-plan]
help_text = "Plan placeholder publishing batches"

[[cli.placeholder-plan.inputs]]
name = "mode"
type = "choice"
default = "placeholder"
choices = ["publish", "placeholder"]

[[cli.placeholder-plan.steps]]
name = "plan placeholder publish rate limits"
type = "PlanPublishRateLimits"
inputs = { mode = "placeholder" }
```

### Render a GitHub Actions snippet

```toml
[cli.publish-plan-github]
help_text = "Render a GitHub Actions batch snippet from the publish plan"

[[cli.publish-plan-github.steps]]
name = "plan publish rate limits"
type = "PlanPublishRateLimits"
inputs = { ci = "github-actions" }
```

## Notes

`PlanPublishRateLimits` is advisory by default. Built-in publish commands only become blocking when matching packages enable `publish.rate_limits.enforce = true`.

The step checks the target registries before counting pending work, so already-published versions and placeholder packages that already exist do not inflate the batch plan.
