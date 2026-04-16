---
monochange: patch
"@monochange/skill": patch
---

#### mark MCP analysis tools as experimental until semantic analysis lands

`monochange_analyze_changes` and `monochange_validate_changeset` now return explicit experimental / not-yet-implemented responses instead of empty success-looking payloads.

**Before:**

- callers could get placeholder responses that looked successful even though semantic analysis was not implemented
- skill and agent docs described these tools as if they were production-ready

**After:**

- both MCP tools return clear error payloads with `experimental = true`
- responses point users to manual alternatives such as `monochange_discover`, `monochange_change`, `mc validate`, and `mc diagnostics --format json`
- assistant and agent docs now link to issue `#243`, which tracks ecosystem-specific semantic analysis work

This keeps expectations aligned while the real analysis design is implemented in ecosystem crates.