---
main: patch
---

#### speed up `mc release --dry-run`

`mc release --dry-run` now avoids the expensive work that made local release previews take tens of seconds or more in large workspaces.

**Before:**

```bash
mc release --dry-run
# could take more than a minute in a repo with many pending changesets,
# fixture manifests, or a configured GitHub token
```

**After:**

```bash
mc release --dry-run
# stays a local preview path and completes much faster
```

The dry-run path now reuses shared changeset lookup state, avoids repeating repository-wide package discovery during release planning, caches git tag queries, and skips hosted GitHub changeset enrichment when it is only rendering a local preview.

That means a command such as:

```bash
mc release --dry-run --format text
```

no longer pays for remote provider lookups just because `GITHUB_TOKEN` is present in the environment.

If you want provider-facing previews, keep using the dedicated commands that are meant to render those payloads explicitly:

```bash
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
```
