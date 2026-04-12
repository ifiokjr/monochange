---
monochange: minor
monochange_core: minor
monochange_github: patch
monochange_gitlab: patch
monochange_gitea: patch
---

#### stream named release steps and keep `mc release` on the fast path

`mc release` now streams progress as each step runs, shows configured step names in the TTY UI, and keeps the real non-dry-run prepare path much closer to `--dry-run` latency by overlapping hosted changeset enrichment with local release planning. Provider release branches also skip redundant `git checkout` work when the branch is already selected.

**Before:**

```toml
[cli.release]
[[cli.release.steps]]
type = "PrepareRelease"

[[cli.release.steps]]
type = "CommitRelease"
```

```bash
mc release
```

The command waited for each phase to complete before showing the final result, provider enrichment always ran inline, and release-step output did not surface explicit step display names in the progress UI.

**After:**

```toml
[cli.release]
[[cli.release.steps]]
type = "PrepareRelease"
name = "Plan release"

[[cli.release.steps]]
type = "CommitRelease"
name = "Write release commit"
```

```bash
mc release
```

TTY runs now stream named step progress with loading indicators and richer terminal formatting, and release-preparation diagnostics record per-phase timings so you can see where time is spent.

Representative output:

```text
○ Plan release
● Plan release (324ms)
○ Write release commit
● Write release commit (41ms)
```

`monochange_github` now batches hosted review-request enrichment instead of issuing one lookup per changeset, and all hosted providers skip no-op branch checkouts during release branch preparation. The real `mc release` benchmark suite also includes the mutating non-dry-run prepare path, so performance regressions are visible without having to infer them from `--dry-run` alone.

For Cargo workspaces, the built-in fast path continues to prefer direct lockfile text rewrites during `mc release`. If you need a full dependency-resolution refresh afterwards, configure explicit `[ecosystems.cargo].lockfile_commands` or run `cargo generate-lockfile` manually after the release step finishes.
