---
monochange: minor
---

#### reuse prepared release state across follow-up commands

`PrepareRelease`-driven commands can now reuse a matching prepared release artifact instead of rebuilding the same release state from scratch every time. `mc release` writes a reusable cache under `.monochange/prepared-release-cache.json`, and later commands with a `PrepareRelease` step automatically pick it up when the workspace still matches.

**Before:**

```bash
mc release
mc release-pr --dry-run
```

Every follow-up command recalculated release planning state independently, even when the release files, git status, and `HEAD` still matched the earlier run.

**After:**

```bash
mc release
mc release-pr --dry-run
```

Warm follow-up commands now reuse the prepared artifact automatically when it is still valid. If you need to pass the prepared state between explicit commands or CI steps, you can also pin the artifact path yourself:

```bash
mc release --prepared-release /tmp/release-plan.json
mc release-pr --prepared-release /tmp/release-plan.json --format json
```

The cached artifact is invalidated when `HEAD`, workspace status, tracked release inputs, or relevant configuration drift, so monochange keeps the fast path without reusing stale release data.
