---
monochange_github: patch
monochange_gitlab: patch
monochange_gitea: patch
---

# Ensure draft releases use proper title fallback

When a release manifest is reconstructed from git history (e.g. during `release-post-merge`), `rendered_title` may be empty. In that case, `build_release_requests` now falls back to `tag_name` for the release name across all providers (GitHub, GitLab, Gitea). This prevents draft releases from appearing with a generic "Draft" title and ensures they display the actual version tag instead.
