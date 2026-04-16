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
- `ci` — optional `github-actions` or `gitlab-ci` snippet renderer

## Produces

A structured publish-rate-limit report containing:

- registry windows
- batch counts
- explicit package ids per batch
- evidence and confidence metadata for each built-in policy

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

[[cli.publish-plan.steps]]
name = "plan publish rate limits"
type = "PlanPublishRateLimits"
```

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
