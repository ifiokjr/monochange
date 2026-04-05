---
monochange: major
monochange_config: major
monochange_core: major
---

#### replace built-in command structure with workflow-defined top-level commands

The CLI surface is now driven entirely by `[cli.<command>]` entries in `monochange.toml`. Top-level commands are no longer hard-coded into the binary.

**Before:**

```bash
mc workspace discover
mc changes add --package core --bump patch --reason "fix"
mc plan release
mc release
```

**After:**

```bash
mc discover        # top-level, defined by [cli.discover] in monochange.toml
mc change --package core --bump patch --reason "fix"
mc release --dry-run
mc validate
```

Running `mc init` writes a starter `monochange.toml` with sensible default commands so projects without an existing config still get the expected surface:

```bash
mc init             # generates monochange.toml with [cli.release], [cli.validate], etc.
```

The internal validation step was renamed from `Check` to `Validate`, and the old nested sub-command model (`mc workspace discover`, `mc plan release`) was removed.

**Breaking changes for `monochange_config` callers:**

- `WorkflowDefinition` is now `CliCommandDefinition`
- `WorkflowStepDefinition` is now `CliStepDefinition`
- `WorkflowInputKind` is now `CliInputKind`
- `default_workflows()` is now `default_cli_commands()`

**Breaking changes for `monochange_core` callers:**

- `default_workflows()` → `default_cli_commands()` (same rename)
- `WorkflowDefinition`, `WorkflowStepDefinition`, `WorkflowInputKind` types renamed accordingly
