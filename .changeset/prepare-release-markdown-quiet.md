---
monochange: patch
monochange_core: patch
---

#### improve release output readability and add `--quiet`

`mc release` now defaults to `--format markdown` so dry-run release output is easier to scan in terminals and logs. The markdown view keeps the same release information as `--format text`, but it groups the output into clearer sections and renders file diffs inside fenced `diff` blocks.

**Before:**

```bash
mc release --dry-run
```

```text
command `release` completed (dry-run)
released packages: workflow-app, workflow-core
release targets:
- package app -> app/v1.0.1 (tag: false, release: false)
- package core -> core/v1.1.0 (tag: false, release: false)
changed files:
- crates/app/Cargo.toml
- crates/core/Cargo.toml
```

**After:**

```bash
mc release --dry-run
```

```markdown
# `release` (dry-run)

## Summary

- **Released packages:** `workflow-app`, `workflow-core`

## Release targets

- **package `app`** → `app/v1.0.1`
  - tag: no · release: no
- **package `core`** → `core/v1.1.0`
  - tag: no · release: no

## Changed files

- `crates/app/Cargo.toml`
- `crates/core/Cargo.toml`
```

`--format text` and `--format json` still work the same, so existing automation can stay on the previous text or JSON contract when needed.

All CLI commands also now accept `--quiet` / `-q`. Quiet mode suppresses stdout and stderr output, and for command-driven workflows it behaves like `--dry-run` so release planning, command steps, and other mutations are skipped while still preserving the command exit status.

**Before:**

```bash
mc release
```

**After:**

```bash
mc --quiet release
```

Use `--quiet` in CI jobs that only care about success or failure and do not want progress logs, warnings, or rendered release summaries in the job output.
