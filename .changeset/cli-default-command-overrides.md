---
monochange: patch
monochange_config: patch
---

# allow per-command CLI overrides without redefining every built-in command

Before:

```toml
[cli.release]
# custom release steps
```

Defining one CLI command replaced the entire built-in command set, so users had to copy every default command they still wanted.

After:

```toml
[cli.release]
# custom release steps
```

Only the built-in `release` command is replaced. Other built-in commands stay available automatically, and additional custom commands are still appended.
