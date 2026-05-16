# ReleaseRecord

`ReleaseRecord` inspects the durable monochange release record associated with a tag or commit.

Use it when debugging post-merge release jobs, verifying which packages a release commit is supposed to publish, or feeding release metadata to external automation.

```sh
mc step:release-record --from HEAD --format json
mc step:release-record --from v1.2.3
mc step:release-record --from HEAD --sha
```

## Inputs

- `from` — git ref, tag, or commit to inspect.
- `format` — text, markdown, or JSON output.
- `sha` — print only the discovered release-record commit SHA.

## Output

The step reports the discovered release record, release targets, package versions, tag mapping, source-provider metadata, and the commit that contains the record.

## Composition notes

`ReleaseRecord` is standalone and reads committed release state. It does not require a previous [`PrepareRelease`](07-prepare-release.md) step. Use it before [`PublishReadiness`](20-publish-readiness.md), [`PlaceholderPublish`](15-placeholder-publish.md), [`PublishPackages`](16-publish-packages.md), or [`TagRelease`](21-tag-release.md) when a workflow starts from an already-committed release.
