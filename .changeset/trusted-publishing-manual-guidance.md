---
monochange: fix
---

#### make manual trusted-publishing guidance more actionable

Improves CLI guidance for registries that still require manual trusted-publishing setup.

**Updated behavior:**

- manual trusted-publishing messages now point users to open the registry setup URL and match repository, workflow, and environment to the current GitHub context
- package-publish text and markdown output now include a concrete next step telling users to finish registry setup and rerun `mc publish`
- built-in publish prerequisite failures now tell users to complete registry setup and rerun the publish command
