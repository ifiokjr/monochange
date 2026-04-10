---
monochange_core: minor
monochange_config: patch
monochange_github: patch
monochange_gitea: patch
monochange_gitlab: patch
monochange: patch
---

Rename GitHub-specific provider types to provider-prefixed generic names in monochange_core. `GitHubReleaseSettings` → `ProviderReleaseSettings`, `GitHubPullRequestSettings` → `ProviderMergeRequestSettings`, `GitHubBotSettings` → `ProviderBotSettings`, etc. Type aliases removed. This is a breaking change for downstream consumers of the public API.
