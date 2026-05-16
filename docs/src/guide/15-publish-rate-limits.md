# Publish rate-limit planning

`mc step:plan-publish-rate-limits` previews package-registry publish work against monochange's built-in ecosystem rate-limit metadata.

```bash
mc step:publish-readiness --from HEAD --output .monochange/readiness.json
mc step:plan-publish-rate-limits --readiness .monochange/readiness.json --format json
mc step:plan-publish-rate-limits --mode placeholder --format json
mc step:plan-publish-rate-limits --ci github-actions
```

The report includes:

- registry windows grouped by publish operation
- the number of pending package publishes per registry
- whether the work fits in a single rate-limit window
- how many batches are required when it does not fit
- a provider-agnostic batch schedule with package ids per batch
- evidence links and confidence levels for the built-in limits

`mc step:plan-publish-rate-limits` only counts package versions that are still missing from their registries. If you rerun a release after some packages were already published, the remaining batches shrink automatically. When you pass `--readiness <path>`, the plan first validates that the readiness artifact covers the current release record, selected package set, and publish input fingerprint, then excludes package ids that are not ready in both the artifact and the fresh local readiness check.

## Current built-in coverage

- `crates.io` — source-backed publish window metadata
- `npm` — conservative advisory metadata when exact package publish quotas are not officially documented
- `jsr` — official publish-window metadata
- `pub.dev` — conservative daily publish planning metadata for CI batching

Use `mc step:publish-readiness --from HEAD --output <path>`, then `mc step:plan-publish-rate-limits --readiness <path>`, then `mc step:publish-packages` when you want CI to fail early instead of discovering registry throttling mid-release. Rerun `mc step:publish-readiness` if workspace config, package manifests, lockfiles, or registry/tooling files changed since the artifact was written. The `--readiness` input is only valid for normal publish planning; placeholder planning still uses `mc step:plan-publish-rate-limits --mode placeholder` without a readiness artifact.

## Filtering and enforcement

Both `mc step:publish-packages` and `mc step:placeholder-publish` accept repeated `--package <id>` filters so you can execute one planned batch at a time. For planning, generate the readiness artifact with the same `--package <id>` selection, or pass a broader readiness artifact to `mc step:plan-publish-rate-limits --readiness <path> --package <id>` so the plan can validate that the artifact covers the selected package subset. The later `mc step:publish-packages --package <id>` run derives work directly from release state and does not consume the readiness artifact.

If you want monochange to block risky built-in publishes instead of only warning, enable:

```toml
[ecosystems.dart.publish.rate_limits]
enforce = true
```

That setting is inherited by matching packages and causes monochange to stop before publishing when the selected package set needs more than one known registry window.

## CI snippets

`mc step:plan-publish-rate-limits --ci github-actions` renders a GitHub Actions job matrix snippet.

`mc step:plan-publish-rate-limits --ci gitlab-ci` renders a GitLab CI matrix snippet.

Both snippets use explicit `mc step:publish-packages --package ...` invocations for each planned batch so you can wire the batches into manual, scheduled, or follow-up pipelines without relying on long sleeps inside CI. Pair each planned batch with `mc step:publish-readiness --from HEAD --package ... --output <path>` when you want a preflight report for that subset; publish the batch with `mc step:publish-packages --package ...`.
