---
monochange: minor
monochange_config: patch
monochange_core: minor
---

# Expose built-in CLI steps as commands

Expose built-in CLI steps as immutable `step:*` commands and move default workflows into generated config.

Rename the `AffectedPackages` revision input from `since` to `from`, so the generated command now accepts `mc step:affected-packages --from <ref>`.
