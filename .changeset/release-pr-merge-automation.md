---
monochange: patch
---

#### require release PRs to use an explicit merge-commit workflow

The repository release-PR automation can now block normal GitHub merges for branches under `monochange/release/` and route them through a dedicated `release-pr-merge` workflow instead.

**Before (manual process):**

- the release PR could be merged from the normal GitHub UI
- maintainers had to remember to use a merge commit instead of squash or rebase
- accidental merge-method changes could rewrite the durable monochange release commit before post-merge tagging and publish

**After:**

- `ci.yml` exposes a required `release-pr-manual-merge-blocker` job that fails for release PR branches
- `.github/workflows/release-pr-merge.yml` verifies every other check is green and then runs:

```bash
gh pr merge <release-pr> --merge --admin --match-head-commit <sha>
```

- the merge workflow expects a `RELEASE_PR_MERGE_TOKEN` secret that is allowed to bypass branch protection for that one intentional path

That setup keeps the exact monochange release commit intact when it lands on `main`, which means the later `mc release-record --from HEAD`, `mc tag-release --from HEAD`, and `mc publish` steps still operate on the durable release record that the PR prepared.
