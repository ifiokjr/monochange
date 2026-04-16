---
monochange: minor
monochange_config: patch
monochange_core: patch
---

#### add a dedicated `mc versions` command

monochange now ships a dedicated `mc versions` command and `DisplayVersions` CLI step for rendering planned package and group versions without the rest of the release preview.

**Before:**

```bash
mc release --dry-run --format text
```

Rendered the full release summary, including release targets, changed files, and other follow-up details.

**After:**

```bash
mc versions --format text
mc versions --format markdown
mc versions --format json
```

This trims the output down to package and group version summaries only, without mutating manifests, changelogs, or consuming changesets.

`monochange_config` also now includes the built-in `versions` command in the default CLI command set returned from workspace configuration loading, so embedded callers and config-driven integrations see the same built-in command list as the `mc` binary.

You can also expose the same behavior from custom commands with `DisplayVersions`:

```toml
[cli.versions]
help_text = "Display planned package and group versions"
inputs = [
	{ name = "format", type = "choice", choices = ["text", "markdown", "json"], default = "text" },
]
steps = [{ type = "DisplayVersions" }]
```
