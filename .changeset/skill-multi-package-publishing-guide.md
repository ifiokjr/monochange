---
"@monochange/skill": patch
---

#### add a multi-package publishing guide to the packaged skill

The packaged skill now includes a dedicated `MULTI-PACKAGE-PUBLISHING.md` guide for repositories that publish multiple public packages from one workspace.

It explains:

- when one shared post-merge `mc publish` job is a good fit
- when package-specific jobs or fully external workflows are clearer
- how to keep tags, workflows, environments, and working directories aligned per package
- when to use package-level publishing overrides in `monochange.toml`

The skill `README.md`, `SKILL.md`, `REFERENCE.md`, and `skills/configuration.md` now point agents to the new guide when publishing strategy depends on monorepo shape rather than only on per-registry trusted-publishing setup.
