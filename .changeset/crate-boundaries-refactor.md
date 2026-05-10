---
monochange: patch
monochange_changelog: patch
monochange_config: patch
monochange_publish: patch
---

# Split crate boundaries for changelog, config, and publish behavior

Move changelog rendering into `monochange_changelog`, shift publish planning and execution helpers into `monochange_publish`, and reduce direct concrete ecosystem/provider dependencies in `monochange_config`.
