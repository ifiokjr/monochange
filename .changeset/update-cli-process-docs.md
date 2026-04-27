---
monochange: patch
"@monochange/cli": patch
"@monochange/skill": patch
---

Document the new CLI process where `mc init` generates editable workflow commands in `monochange.toml` and every built-in step is available directly through immutable `mc step:*` commands. The docs now clarify reserved command names such as `validate` and recommend `mc step:affected-packages --verify` for direct changeset-policy checks.
