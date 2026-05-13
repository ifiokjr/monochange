# Async performance work log

## Status

- Branch: `feat/async-migration`
- PR: <https://github.com/monochange/monochange/pull/440>
- Main benchmark ref: `4cf0b0349fc4aa5f5775d6a6db624c6cd18b7a39`
- PR benchmark ref: `9c767aff13e591e2c202328f7649a694ad4a9a01`
- Benchmark workdir: `/tmp/monochange-async-perf-20260513`
- Hosted benchmark report: <https://htmlpreview.github.io/?https://gist.github.com/ifiokjr/6d59be48cc190d0808c768878f9c7c4a/raw/index.html>
- Gist with raw reports: <https://gist.github.com/ifiokjr/6d59be48cc190d0808c768878f9c7c4a>

## Work completed

- Rebasing and validation:
  - Rebasing on latest `origin/main` completed.
  - Re-applied pre-existing unstaged local work after the rebase.
  - Pushed rebase adjustments in `6aaebc7b fix: finalize async migration rebase adjustments`.
  - Fixed failing checks in `9c767aff fix: resolve async migration check failures`.
  - Verified the PR checks are passing; remaining `mergeStateStatus: BLOCKED` appears to be branch protection or review policy rather than failing CI.
- Local quality gates run successfully:
  - `git diff --check`
  - `devenv shell -- lint:format`
  - `devenv shell -- lint:clippy`
  - `devenv shell -- test:cargo`
  - `devenv shell -- coverage:patch` with `100.00%` patch coverage
- Benchmark setup:
  - Created isolated detached worktrees for current `main` and `feat/async-migration` under `/tmp/monochange-async-perf-20260513`.
  - Built both release binaries with explicit libiconv native search path:
    - `RUSTFLAGS="-L native=/nix/store/xvmhkpvfvmy4sfdkqwg9inq3qkpnx81b-libiconv-109.100.2/lib"`
  - Confirmed `hyperfine 1.20.0` is available.
- Benchmark execution:
  - Ran top-level CLI benchmark with `scripts/benchmark-cli.mjs`.
  - Ran built-in step-command benchmark with `scripts/benchmark-step-commands.mjs`.
  - Re-ran step commands with `--warmup 3 --runs 8` to remove a noisy `step:display-versions` outlier.
  - Produced an HTML benchmark comparison report and hosted it in a secret gist for review.
- Follow-up automation:
  - Scheduled a workspace-scoped recurring prompt every 15 minutes for the next 24 hours: `Keep improving performance and reducing memory usage`.
- Post-report optimization iteration:
  - Profiled `step:diagnose-changesets` on the large fixture and found repeated changeset load-context construction plus repeated root-relative path normalization in the hot path.
  - Reused a shared `ChangesetLoadContext` while loading all diagnosed changesets.
  - Reused a shared `ChangesetLoadContext` while validating attached changesets in affected-package policy checks.
  - Reused precomputed changeset-relative paths while building prepared changesets and diagnostics output.
  - Added a focused unit test covering multi-file diagnose loading through the shared-context path.

## Current benchmark summary

### Top-level CLI commands

Measured with `hyperfine --warmup 1 --runs 6`.

| Fixture                                            | Command                     |      main |       PR | PR/main | Reduction |
| :------------------------------------------------- | :-------------------------- | --------: | -------: | ------: | --------: |
| Baseline, 20 packages / 50 changesets / 50 commits | `mc step:validate`          |   37.5 ms |  25.4 ms |   0.68× |     32.3% |
| Baseline, 20 packages / 50 changesets / 50 commits | `mc discover --format json` |   31.2 ms |  16.4 ms |   0.53× |     47.4% |
| Baseline, 20 packages / 50 changesets / 50 commits | `mc release --dry-run`      |  316.6 ms | 131.8 ms |   0.42× |     58.4% |
| Baseline, 20 packages / 50 changesets / 50 commits | `mc release`                |  345.9 ms | 148.9 ms |   0.43× |     57.0% |
| Large, 200 packages / 500 changesets / 500 commits | `mc step:validate`          |  595.1 ms | 475.3 ms |   0.80× |     20.1% |
| Large, 200 packages / 500 changesets / 500 commits | `mc discover --format json` |  548.0 ms | 380.7 ms |   0.69× |     30.5% |
| Large, 200 packages / 500 changesets / 500 commits | `mc release --dry-run`      | 2769.8 ms | 708.6 ms |   0.26× |     74.4% |
| Large, 200 packages / 500 changesets / 500 commits | `mc release`                | 3021.1 ms | 784.6 ms |   0.26× |     74.0% |

Top-level CLI benchmark violations: `0`.

### Built-in step commands

Measured with `hyperfine --warmup 3 --runs 8` on 200 packages, 500 changesets, 500 commits.

| Command                                               |      main |        PR | PR/main | Status   |
| :---------------------------------------------------- | --------: | --------: | ------: | :------- |
| `mc step:config --dry-run`                            |    9.9 ms |    9.5 ms |   0.96× | improved |
| `mc step:validate --dry-run`                          |  588.1 ms |  486.7 ms |   0.83× | improved |
| `mc step:discover --dry-run --format json`            |  557.3 ms |  403.5 ms |   0.72× | improved |
| `mc step:display-versions --dry-run --format json`    |  574.4 ms |  581.5 ms |   1.01× | flat     |
| `mc step:create-change-file --dry-run`                |  512.8 ms |  402.8 ms |   0.79× | improved |
| `mc step:prepare-release --dry-run --format json`     | 2590.2 ms |  682.3 ms |   0.26× | improved |
| `mc step:affected-packages --dry-run --format json`   | 1268.3 ms | 1063.1 ms |   0.84× | improved |
| `mc step:diagnose-changesets --dry-run --format json` | 2654.9 ms | 2436.8 ms |   0.92× | improved |

Step-command benchmark violations: `0`.

## Latest targeted optimization results

Measured on the 200-package / 500-changeset / 500-commit profiling fixture after rebuilding the current branch release binary.

| Command                                                                                     | Baseline |  Current | Result       |
| :------------------------------------------------------------------------------------------ | -------: | -------: | :----------- |
| `mc step:diagnose-changesets --dry-run --format json` vs `main`                             |  2.919 s | 596.4 ms | 4.89× faster |
| `mc step:diagnose-changesets --dry-run --format json` vs pre-optimization async             |  2.731 s | 596.4 ms | 4.58× faster |
| `mc step:affected-packages --dry-run --format json --from HEAD~1` vs pre-optimization async |  1.229 s |  1.252 s | flat / noisy |

Peak memory sampling with `/usr/bin/time -l` for `step:diagnose-changesets` also improved slightly: max RSS went from about 21.6 MB to 21.3 MB, peak footprint from about 11.6 MB to 11.4 MB, and retired instructions from about 37.6B to 6.9B.

## Main bottlenecks to tackle next

1. `step:diagnose-changesets` is much faster after shared changeset context reuse, but still spends about `0.60 s` on the large fixture.
   - Remaining time is likely split across workspace discovery, git history lookup, JSON construction, and unavoidable changeset file reads.
   - Next work: profile the remaining call graph after this optimization and reduce any remaining repeated path normalization or discovery work.
2. `step:affected-packages` remains over `1.0 s` on the large fixture.
   - Likely bottlenecks are git history traversal, repeated package graph lookups, and repeated changeset or manifest reads.
   - Next work: inspect whether affected-package computation can reuse release workspace discovery and parsed changesets.
3. `step:display-versions` is only flat after the async migration.
   - It may not benefit from the current async paths or may be dominated by serialization/output construction.
   - Next work: isolate computation versus JSON formatting and avoid cloning large version-plan structures.
4. Large-fixture `mc step:validate` still spends roughly `0.49 s` on the PR branch.
   - Next work: identify whether validation repeatedly loads manifests or performs serial registry/source checks that can be cached or batched.
5. Release phase timings do not fully explain the wall-clock `mc release` / `mc release --dry-run` durations.
   - Next work: add or inspect outer phase spans around command startup, fixture setup, workflow orchestration, and subprocess boundaries so the remaining wall time is attributable.
6. Memory usage has not yet been measured alongside runtime.
   - Next work: add peak RSS measurements for the benchmarked commands, then target allocation-heavy structures with borrowed data, streaming iteration, or smaller owned summaries.

## Iteration checklist

- [ ] Add lightweight profiling for the slow step paths without affecting normal output.
- [ ] Measure peak RSS for top-level CLI and direct step-command benchmarks.
- [x] Profile `step:diagnose-changesets` and reduce repeated git or filesystem work.
- [ ] Profile `step:affected-packages` and reuse package graph / changeset discovery data where possible.
- [ ] Profile `step:display-versions` and reduce cloning or serialization overhead.
- [ ] Re-run the benchmark scripts after each optimization.
- [ ] Keep this file updated with changes, results, and remaining bottlenecks.

## Validation commands

```sh
git diff --check
devenv shell -- lint:format
devenv shell -- lint:clippy
devenv shell -- test:cargo
devenv shell -- coverage:patch
```

## Benchmark commands

```sh
workdir=/tmp/monochange-async-perf-20260513
node "$workdir/async/scripts/benchmark-cli.mjs" run \
  --main-bin "$workdir/main/target/release/mc" \
  --pr-bin "$workdir/async/target/release/mc" \
  --output "$workdir/async-performance.md" \
  --violations-output "$workdir/async-performance-violations.txt"

node "$workdir/async/scripts/benchmark-step-commands.mjs" run \
  --main-bin "$workdir/main/target/release/mc" \
  --pr-bin "$workdir/async/target/release/mc" \
  --output "$workdir/async-step-performance-rerun.md" \
  --violations-output "$workdir/async-step-performance-rerun-violations.txt" \
  --warmup 3 \
  --runs 8
```
