---
monochange: feat
"@monochange/cli": docs
---

#### add `mc tag-release` for post-merge release PR workflows

monochange now ships a first-class `mc tag-release` command for the long-running release PR flow.

**Before:**

```bash
mc release-record --from HEAD --format json
mc publish
```

That let CI detect a merged monochange release commit and publish package registries from the durable `ReleaseRecord`, but monochange did not have a built-in command to create and push the release tag set after merge.

**After:**

```bash
mc release-record --from HEAD --format json
mc tag-release --from HEAD
mc publish
```

`mc tag-release` reads the durable `ReleaseRecord` on the merged release commit, creates the declared tag set on that commit, and pushes those tags to `origin`.

**Before (generated GitHub Actions `release.yml`):**

```yaml
- name: prepare and open release PR
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: mc commit-release
```

**After:**

```yaml
- name: refresh release PR
  if: steps.release_record.outputs.is_release_commit != 'true'
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: mc release-pr

- name: create release tags
  if: steps.release_record.outputs.is_release_commit == 'true'
  run: mc tag-release --from HEAD

- name: publish packages
  if: steps.release_record.outputs.is_release_commit == 'true'
  run: mc publish
```

The generated GitHub workflow now refreshes the release PR on normal `main` pushes, then switches into post-merge tagging and package publication when `HEAD` is already the merged monochange release commit.

The bundled `@monochange/cli` documentation now describes this post-merge tagging flow as part of the recommended release PR workflow.
