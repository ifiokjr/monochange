---
monochange: minor
monochange_core: patch
---

#### add `--versions` output for `PrepareRelease`

`mc release` can now render a versions-only summary when you only need the planned package and group versions instead of the full release preview. The `--versions` flag now implies `--dry-run`, so the summary path stays non-mutating.

**Before:**

```bash
mc release --dry-run --format text
```

Rendered the full release summary, including release targets, changed files, and other follow-up details.

**After:**

```bash
mc release --versions --format text
mc release --versions --format markdown
mc release --versions --format json
```

This trims the output down to package and group version summaries only, without mutating manifests, changelogs, or consuming changesets.

You can also expose the same behavior from custom commands that use `PrepareRelease`:

```toml
[cli.release-versions]
help_text = "Print only the planned release versions"
inputs = [
	{ name = "format", type = "choice", choices = ["markdown", "text", "json"], default = "markdown" },
	{ name = "versions", type = "boolean", default = "false" },
]
steps = [{ type = "PrepareRelease" }]
```
