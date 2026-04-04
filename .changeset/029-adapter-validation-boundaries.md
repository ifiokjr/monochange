---
monochange_core: patch
monochange_config: patch
monochange_github: patch
monochange_gitlab: patch
monochange_gitea: patch
---

#### move source-provider validation behind provider crates

Introduce shared source capability metadata in `monochange_core`, move provider-specific source validation into the GitHub, GitLab, and Gitea crates, and add fixture-backed integration coverage plus architecture guardrails for keeping adapter implementation details out of shared orchestration code.
