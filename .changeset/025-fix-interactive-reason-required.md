---
monochange_core: patch
---

#### make `--reason` optional at the clap level for interactive mode

`mc change` no longer requires `--reason` to be passed on the command line when running interactively. Previously, omitting `--reason` caused clap to exit with a missing-argument error before the interactive prompt could appear.

**Before:**

```bash
mc change --package core --bump minor
# error: the following required arguments were not provided:
#   --reason <REASON>
```

**After:**

```bash
mc change --package core --bump minor
# > Reason: ▌   ← interactive prompt shown instead
```

`--reason` remains fully supported as a non-interactive shortcut:

```bash
mc change --package core --bump minor --reason "add streaming support"
```

The fix is in `monochange_core`'s clap argument definition for the `change` command, where `--reason` is now declared `required(false)` with interactive fallback handling.
