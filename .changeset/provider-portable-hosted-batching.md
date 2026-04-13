---
monochange: patch
monochange_core: patch
monochange_gitea: patch
monochange_github: patch
monochange_gitlab: patch
---

#### make hosted batching and release enrichment provider portable

Hosted changeset context, released-issue comments, and release retargeting now run through a shared provider adapter boundary instead of separate GitHub-only orchestration paths. GitHub keeps its batched enrichment fast path, while GitLab and Gitea now share the same capability-driven flow and diagnostics surface.
