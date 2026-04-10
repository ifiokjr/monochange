---
monochange: patch
---

# add mc populate for materializing default CLI commands into config

Before:

```bash
mc populate
```

The command did not exist, so users had to copy built-in `[cli.*]` definitions by hand when they wanted editable defaults in `monochange.toml`.

After:

```bash
mc populate
```

monochange appends only the missing built-in CLI command definitions to `monochange.toml`, leaving existing command overrides unchanged.
