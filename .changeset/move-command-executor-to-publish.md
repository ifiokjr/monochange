---
monochange: patch
monochange_publish: minor
---

# Move CommandExecutor and command rendering into monochange_publish

Extract `CommandOutput`, `CommandExecutor`, `ProcessCommandExecutor`, and the helper functions `render_command` and `render_command_error` from `monochange::package_publish` into `monochange_publish`. This continues the Phase 2 crate boundary cleanup by ensuring the publish crate owns all command execution infrastructure used during publishing.
