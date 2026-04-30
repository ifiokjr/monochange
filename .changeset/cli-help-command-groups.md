---
monochange: patch
---

# Group CLI help commands consistently

Make `mc -h`, `mc --help`, and `mc help` render the same command overview so users see consistent help no matter which entry point they use.

The overview now separates built-in commands, generated `step:*` commands, and user-defined `monochange.toml` commands. Generated step commands are always listed, and detailed command help includes richer descriptions for step commands such as `step:publish-release`.
