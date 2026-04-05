---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_github: minor
monochange_cargo: minor
monochange_graph: minor
---

#### add release pull request automation workflows

The `OpenReleasePullRequest` step (formerly `OpenGitHubReleasePullRequest`) automates the release PR lifecycle from a single workflow:

```toml
[cli.release-pr]
[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleasePullRequest"
```

```bash
mc release-pr --dry-run --format json   # preview branch name, title, and PR body
mc release-pr                           # commit prepared changes, push branch, open/refresh PR
```

Live runs perform all steps in sequence: commit the prepared release changes to a release branch (e.g. `release/next`), push the branch, and call `gh pr create` (or update if one already exists). The PR body is rendered deterministically from the shared release manifest, so re-running `mc release-pr` on an existing PR refreshes the body without creating duplicates.

Pull-request behaviour is configured through `[source.pull_requests]`:

```toml
[source.pull_requests]
enabled = true
branch_prefix = "release/"
base = "main"
title = "chore: release {{ version }}"
labels = ["release"]
```

**`monochange_github`** owns the PR-request building and `gh` wrapper. **`monochange_core`** adds the graph-level `source_path` field on `ChangeSignal` that lets the step track which changeset files belong to the prepared commit.
