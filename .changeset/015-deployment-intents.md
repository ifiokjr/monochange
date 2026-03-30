---
monochange: minor
monochange_config: minor
monochange_core: minor
---

#### add deployment intent workflows and manifest support

Add typed `[[deployments]]` configuration, deployment trigger metadata, and a `Deploy` workflow step that turns configured deployment definitions into structured release-manifest intents.

Dry-run and manifest-oriented workflows can now surface deployment names, environments, required branches, target release identities, and arbitrary metadata so downstream CI or GitHub Actions can orchestrate deployment execution without hardcoding platform-specific providers into MonoChange.
