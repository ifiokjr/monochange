# TagRelease

`TagRelease` creates release tags declared by a committed monochange release record.

Use it in post-merge release automation after a release commit reaches an allowed release branch.

```sh
mc step:tag-release --from HEAD --dry-run
mc step:tag-release --from HEAD
mc step:tag-release --from v1.2.3 --format json
```

## Inputs

- `from` — git ref, tag, or commit containing a release record.
- `push` — push created tags to the configured remote when enabled by the command input.
- `format` — text, markdown, or JSON output.

## Output

The step reports the tag names and target commit. JSON output includes a flat tag map keyed by package or group id so automation can select exact tags without relying on display order.

## Safety notes

`TagRelease` is mutating unless run with `--dry-run`. It enforces configured release-branch reachability before creating tags. Agents must not create, delete, or modify tags unless a human maintainer explicitly owns that operation and project rules allow it.

## Composition notes

`TagRelease` reads release state from a release record and does not require a previous [`PrepareRelease`](07-prepare-release.md) step. Pair it with [`ReleaseRecord`](19-release-record.md) for inspection and [`PublishReadiness`](20-publish-readiness.md) before package publishing.
