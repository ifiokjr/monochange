---
"monochange": patch
"monochange_github": patch
---

Make `CommitRelease` more resilient when release metadata is stored in `.monochange/` artifacts instead of staged manifest files.

Before:

```sh
mc release --commit
# could fail if .monochange/release-manifest.json was gitignored

mc commit-release
# required a PrepareRelease step in the same command
```

After:

```sh
mc release
# refreshes .monochange/release-manifest.json as a local cached artifact

mc release --commit
# succeeds even when .monochange/release-manifest.json is ignored

mc commit-from-cache
# can reuse .monochange/prepared-release-cache.json without rerunning PrepareRelease
```

`PrepareRelease` now refreshes `.monochange/release-manifest.json` automatically as local cached metadata, and `CommitRelease` skips ignored untracked manifest files and stale missing pathspecs while still staging real release files. `CommitRelease` can also reuse a saved prepared release artifact when the command does not include its own `PrepareRelease` step.

The GitHub release PR flow now applies the same staging guardrails, so ignored local release artifacts do not break release PR preparation or related CI coverage/benchmark paths.
