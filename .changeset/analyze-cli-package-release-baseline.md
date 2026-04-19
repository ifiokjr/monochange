---
monochange: minor
monochange_config: patch
---

Add a package-scoped `mc analyze` CLI command for release-aware semantic analysis.

The command defaults `--release-ref` to the most recent tag for the selected package or its owning version group, compares `main -> head` for first releases when no prior tag exists, and supports text or JSON output for package-focused review workflows.

`monochange_config` now reserves the built-in `analyze` command name so workspace CLI definitions cannot collide with the new built-in subcommand.
