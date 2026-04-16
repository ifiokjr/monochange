# Release PR example

## Recommend this when

- the team wants a reviewable release branch
- release files should be inspected before they land on the default branch
- tags and publishes should happen only after merge

## Default recommendation

- use `mc release-pr` for branch refresh
- do not create tags on the release branch
- after merge, detect the durable release commit with `mc release-record --from HEAD --format json`
- run `mc tag-release --from HEAD` before package publishing

## Good default output

- release PR refresh strategy
- post-merge tagging and publish steps
- approval and human-review checkpoints
