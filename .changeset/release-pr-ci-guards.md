---
monochange: patch
---

#### tighten release commit detection in CI and run `release-pr` only on `main`

The built-in GitHub Actions release automation now treats a commit as a release commit only when `HEAD` itself matches the stored release record. That prevents ordinary commits from skipping `publish:check` just because an older release record exists somewhere in history.

Command used by the workflow:

```bash
mc release-record --from HEAD --format json
```

**Before (workflow behavior):**

```yaml
if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
echo "is_release_commit=true" >> "$GITHUB_OUTPUT"
else
echo "is_release_commit=false" >> "$GITHUB_OUTPUT"
fi
```

Any reachable release record could make CI behave as if the current commit was the release commit.

**After:**

```yaml
resolved_commit="$(jq -r '.resolvedCommit' /tmp/release-record.json)"
record_commit="$(jq -r '.recordCommit' /tmp/release-record.json)"

if [ "$resolved_commit" = "$record_commit" ]; then
echo "is_release_commit=true" >> "$GITHUB_OUTPUT"
else
echo "is_release_commit=false" >> "$GITHUB_OUTPUT"
fi
```

With that guard in place:

- `publish:check` is skipped only for the actual release commit at `HEAD`
- the generated `release.yml` template uses the same detection logic
- the `release-pr` job now runs only on pushes to `main`
- the workflow passes `GH_TOKEN` to `mc release-pr` so the built-in GitHub provider can authenticate without extra wrapper scripting
