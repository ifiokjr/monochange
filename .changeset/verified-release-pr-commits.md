---
monochange: patch
monochange_core: minor
monochange_github: patch
"@monochange/cli": patch
---

# Prefer verified GitHub release PR commits

When `[source.pull_requests].verified_commits = true`, `mc release-pr` publishes a GitHub release pull request from GitHub Actions by asking the GitHub provider to recreate the release branch commit through the Git Database API and only moves the branch when GitHub marks the replacement commit as verified.

The setting defaults to `false`. If the API commit cannot be created, is not verified, or the branch changes before the replacement lands, monochange leaves the normal pushed git commit in place and continues with the release PR flow.
