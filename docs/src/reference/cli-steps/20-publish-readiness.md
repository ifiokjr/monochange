# PublishReadiness

`PublishReadiness` checks package-registry publishability from a committed release record without publishing packages.

Use it in CI before real registry mutation, or locally when you need to understand why a release package is blocked.

```sh
mc step:publish-readiness --from HEAD
mc step:publish-readiness --from HEAD --output .monochange/local/readiness.json
mc step:publish-readiness --from v1.2.3 --package core --format json
```

## Inputs

- `from` — git ref, tag, or commit containing a release record.
- `package` — optional repeatable package filter.
- `output` — optional JSON readiness artifact path.
- `format` — text, markdown, or JSON output.

## Output

The report groups packages by readiness state, including ready, already published, unsupported, and blocked packages. JSON artifacts include schema metadata, release-record identity, selected package ids, and fingerprints of publish inputs so later planning can detect stale artifacts.

## Composition notes

`PublishReadiness` reads release state from a release record. Run it after a release commit exists. If first-time packages are missing from registries, run [`PlaceholderPublish`](15-placeholder-publish.md), then rerun `PublishReadiness` before [`PlanPublishRateLimits`](09-plan-publish-rate-limits.md) or [`PublishPackages`](16-publish-packages.md).
