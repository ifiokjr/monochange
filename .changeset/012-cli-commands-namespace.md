---
monochange: minor
monochange_core: minor
monochange_config: minor
monochange_github: minor
---

#### replace `workflows` config with command-keyed `cli` commands

The `[[workflows]]` array is replaced by a `[cli.<command>]` table. Existing `[[workflows]]` config is rejected with a migration error pointing to the new syntax.

**Before (rejected with error):**

```toml
[[workflows]]
name = "release"
[[workflows.steps]]
type = "PrepareRelease"
```

**After:**

```toml
[cli.release]
[[cli.release.steps]]
type = "PrepareRelease"
```

Each command can declare typed inputs that become CLI flags:

```toml
[cli.release]
[[cli.release.inputs]]
name = "format"
kind = "string"

[[cli.release.steps]]
type = "PrepareRelease"
```

```bash
mc release --format json
```

The `dry_run` field on `Command` steps was also renamed to `dry_run_command` to avoid ambiguity with the top-level `--dry-run` flag.

**`monochange_config`** replaces `WorkflowDefinition` with `CliCommandDefinition` and validates the new config shape. Running `mc init` now emits `[cli.<command>]` entries. **`monochange_github`** step names are unchanged at the step-type level.
