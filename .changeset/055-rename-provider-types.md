---
monochange_core: minor
monochange_config: patch
monochange: patch
---

Rename GitHub-specific provider types to generic names in monochange_core. `GitHubReleaseSettings` becomes `ReleaseProviderSettings`, `GitHubPullRequestSettings` becomes `ChangeRequestSettings`, etc. Type aliases removed. This is a breaking change for downstream consumers of the public API.
