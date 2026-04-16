---
monochange: patch
---

#### align publish rate-limit plans with pending registry work

`mc publish`, `mc placeholder-publish`, and `mc publish-plan` now count only the package versions that are still missing from their registries when they build `publishRateLimits` output.

**Before:**

```bash
mc publish --dry-run --format json
mc placeholder-publish --dry-run --format json
mc publish-plan --format json
```

If some selected package versions were already present in their registries, the rate-limit report could still count them as pending work and recommend extra batches even though the publish command would skip them.

**After:**

```bash
mc publish --dry-run --format json
mc placeholder-publish --dry-run --format json
mc publish-plan --format json
```

The `publishRateLimits` report now shrinks automatically on reruns, partial publishes, and placeholder bootstrap flows where some packages already exist. That keeps advisory warnings, optional enforcement, and CI batch plans aligned with the actual work left to publish.
