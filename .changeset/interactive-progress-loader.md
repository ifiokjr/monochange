---
monochange: patch
monochange_core: patch
---

Interactive change authoring no longer shows the CLI progress spinner while the user is choosing options in the terminal.

You can also disable progress output for interactive-capable steps explicitly:

```toml
[[cli.change-interactive.steps]]
type = "CreateChangeFile"
show_progress = false
inputs = { interactive = true }
```

```toml
[[cli.some-command.steps]]
type = "Command"
show_progress = false
command = "fzf"
shell = true
```
