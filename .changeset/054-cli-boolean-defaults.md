---
monochange: patch
monochange_config: patch
monochange_core: patch
---

CLI boolean inputs now accept TOML boolean defaults such as `default = true` and `default = false` in addition to string defaults.

This also preserves numeric truthiness for conditional evaluation, so values like `"1"` still resolve as truthy while `"0"` resolves as false.
