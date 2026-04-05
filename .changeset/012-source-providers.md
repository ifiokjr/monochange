---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_gitea: minor
monochange_github: minor
monochange_gitlab: minor
---

#### add configurable source providers for release automation

Repository automation is now driven through a `[source]` section rather than the legacy `[github]` section. Using `[github]` alongside `[source]` is rejected with a configuration error.

```toml
# GitHub (default provider)
[source]
provider = "github"
owner = "acme"
repo = "monorepo"

# GitLab
[source]
provider = "gitlab"
owner = "acme"
repo = "monorepo"
host = "gitlab.example.com" # self-hosted

# Gitea
[source]
provider = "gitea"
owner = "acme"
repo = "monorepo"
host = "gitea.example.com"
```

The workflow step names are now provider-neutral. Legacy GitHub-specific step names are kept as migration aliases:

```toml
[[cli.release.steps]]
type = "PublishRelease" # replaces "PublishGitHubRelease"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest" # replaces "OpenReleasePullRequest"
```

**`monochange_core`** gains `SourceConfiguration` and the `SourceProvider` enum (`GitHub`, `GitLab`, `Gitea`). **`monochange_config`** parses `[source]` and migrates the legacy `[github]` block through the same `SourceConfiguration` type. **`monochange_gitea`** and **`monochange_gitlab`** are new crates that implement `PublishRelease` and `OpenReleaseRequest` for their respective providers.
