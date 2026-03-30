---
monochange: minor
monochange_core: minor
monochange_config: minor
---

#### replace `workflows` config with command-keyed `cli` commands

Replace the `[[workflows]]` configuration surface with a command-keyed `[cli.<command>]` namespace. MonoChange now models top-level configured commands directly, emits `[cli.<command>]` entries from `mc init`, renames `dry_run` to `dry_run_command` for `Command` steps, and rejects legacy `[[workflows]]` config with a migration error.
