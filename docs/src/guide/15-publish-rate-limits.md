# Publish rate-limit planning

`mc publish-plan` previews package-registry publish work against monochange's built-in ecosystem rate-limit metadata.

```bash
mc publish-plan --format json
mc publish-plan --mode placeholder --format json
mc publish-plan --ci github-actions
```

The report includes:

- registry windows grouped by publish operation
- the number of pending package publishes per registry
- whether the work fits in a single rate-limit window
- how many batches are required when it does not fit
- a provider-agnostic batch schedule with package ids per batch
- evidence links and confidence levels for the built-in limits

`mc publish-plan` only counts package versions that are still missing from their registries. If you rerun a release after some packages were already published, the remaining batches shrink automatically.

## Current built-in coverage

- `crates.io` — source-backed publish window metadata
- `npm` — conservative advisory metadata when exact package publish quotas are not officially documented
- `jsr` — official publish-window metadata
- `pub.dev` — conservative daily publish planning metadata for CI batching

Use `mc publish-plan` before `mc publish` when you want CI to fail early instead of discovering registry throttling mid-release.

## Filtering and enforcement

Both `mc publish` and `mc placeholder-publish` accept repeated `--package <id>` filters so you can execute one planned batch at a time.

If you want monochange to block risky built-in publishes instead of only warning, enable:

```toml
[ecosystems.dart.publish.rate_limits]
enforce = true
```

That setting is inherited by matching packages and causes monochange to stop before publishing when the selected package set needs more than one known registry window.

## CI snippets

`mc publish-plan --ci github-actions` renders a GitHub Actions job matrix snippet.

`mc publish-plan --ci gitlab-ci` renders a GitLab CI matrix snippet.

Both snippets use explicit `mc publish --package ...` invocations for each planned batch so you can wire the batches into manual, scheduled, or follow-up pipelines without relying on long sleeps inside CI.
