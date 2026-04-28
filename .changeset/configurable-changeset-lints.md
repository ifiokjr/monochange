---
monochange: feat
monochange_config: feat
monochange_core: feat
"@monochange/skill": patch
---

#### Configure changeset lint rules

Add configurable changeset lint rules under `[lints.rules]` for summaries, section headings, bump-specific requirements, and changelog-type-specific requirements.

Rules can target built-in or custom changeset types with dynamic ids like `changesets/types/breaking` and `changesets/types/unicorns`, while unknown type ids are rejected during configuration loading.
