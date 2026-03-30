---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_github: minor
---

#### add release pull request automation workflows

Add typed GitHub release pull request configuration through `[github.pull_requests]`, a first-class `OpenReleasePullRequest` workflow step, and deterministic release-PR branch/body rendering derived from the shared release manifest.

Dry-run workflows now preview release pull request payloads as structured JSON, while live runs can create or update a dedicated release branch, commit the prepared release changes, push that branch, and open or refresh the GitHub pull request through `git` and `gh`.
