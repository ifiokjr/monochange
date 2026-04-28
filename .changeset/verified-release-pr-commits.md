---
monochange: patch
monochange_github: patch
---

#### prefer verified GitHub release PR commits in Actions

When `mc release-pr` publishes a GitHub release pull request from GitHub Actions, monochange now asks the GitHub provider to recreate the release branch commit through the Git Database API and only moves the branch when GitHub marks the replacement commit as verified.

If the API commit cannot be created, is not verified, or the branch changes before the replacement lands, monochange leaves the normal pushed git commit in place and continues with the release PR flow.
