---
"monochange": patch
"monochange_config": patch
"monochange_core": patch
"monochange_forgejo": patch
"monochange_gitea": patch
"monochange_github": patch
"monochange_gitlab": patch
"monochange_hosting": patch
"monochange_schema": patch
---

# Add optional full release staging

Release commit and release request steps now support a `stage_all` input/config field that defaults to `false`. When enabled, the release commit stages every non-ignored working tree change, so generated lockfile updates like `pnpm-lock.yaml` can be included alongside configured release manifests and changelogs.
