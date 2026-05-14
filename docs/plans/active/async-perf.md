# Async performance work log

## Status

- Branch: `feat/async-migration`
- PR: <https://github.com/monochange/monochange/pull/440>
- Main benchmark ref: `4cf0b0349fc4aa5f5775d6a6db624c6cd18b7a39`
- PR benchmark ref: `463cd512779cb2add55434c07079568728d90b79` plus local diagnose follow-up changes
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
  - Deferred workspace discovery and changeset package-reference mapping in affected-package policy until attached changeset files are present.
  - Precompiled affected-package path globs and normalized package roots once per policy evaluation instead of rebuilding them for every changed-path/package pair.
  - Switched changed-path collection from repeated `Vec::contains` checks to a `BTreeSet` while merging diff and untracked paths.
  - Added a configuration-only attached-changeset coverage index so policy checks can validate config package/group ids without manifest discovery and avoid discovering manifests even on attached-changeset errors.
  - Added regression tests proving affected-package checks without attached changesets and config-id attached changesets do not require workspace discovery.
  - Reused the same configuration-only package index for diagnostics, so `step:diagnose-changesets` can skip manifest discovery when changesets only use configured package/group ids and explicit bumps.
  - Added a regression test proving diagnostics can load config-id changesets without parsing package manifests.
  - Reworked `step:display-versions` summary rendering to borrow sorted package/group version-plan entries instead of cloning them into owned maps.
  - Replaced eager version `to_string()` allocation for JSON with streaming `Display` serialization, and render text output directly into one buffer instead of building a temporary `Vec<String>`.
  - Kept a diagnostics fallback regression test for explicit-version changesets so the optimized config-only fast path remains covered without regressing patch coverage.
  - Streamed non-interactive and interactive changeset markdown rendering directly into one buffer, including inline YAML quoting for target keys and `caused_by`, to avoid temporary line vectors, escaped-string vectors, and joins.
  - Avoided cloning interactive target ids into a temporary vector when only the first target is needed for the default changeset file slug.
  - Streamed prepared-release artifact JSON through buffered readers/writers, serialized prepared releases and file diffs through borrowed views, filtered git status lines before allocating retained status strings, deduplicated tracked snapshot paths before cloning them, and filled tracked-path hashes in place.
  - Streamed colorized diff rendering directly into one output buffer instead of allocating a temporary vector of per-line strings before joining.
  - Streamed release deduplication index reads/writes through buffered files, parsed JSONL entries into borrowed structs, and wrote sorted borrowed hashes instead of allocating formatted line strings and a joined file body.
  - Reduced versioned-file update allocations by deduplicating borrowed definitions, passing dependency names as borrowed strings, streaming glob matches, rendering cached JSON directly into bytes, borrowing package release versions by config id, and bypassing UTF-8 conversion for binary Bun lockfiles.
  - Streamed publish-readiness text, Markdown, JSON newline, and package-fingerprint rendering directly into output buffers instead of allocating temporary line or identity-render vectors before joining, and made publish-readiness package identities borrow report fields instead of cloning strings for validation and fingerprinting.
  - Streamed release-branch policy error construction directly into one string, avoiding temporary branch-name vectors and joined strings on rejection paths.
  - Streamed git error details directly into one string, avoiding temporary two-item vectors and joins when git commands fail.
  - Streamed `--jq` output rendering directly into one buffer instead of collecting rendered values into a temporary vector before joining, and writes scalar and composite filtered values without allocating per-value strings.
  - Streamed publish rate-limit enforcement error details directly into one string, avoiding temporary blocked-window and detail vectors on rejection paths.
  - Streamed publish-readiness package identity lists directly into one string instead of joining cloned identity strings.
  - Streamed lint progress summary counts directly into one line, avoiding a temporary parts vector and comma-join path.
  - Streamed workspace operation TOML array and inline-table rendering directly into output strings instead of collecting temporary fragments before joining.
  - Streamed subagent overwrite-conflict errors directly into one message instead of collecting path strings before joining.
  - Reused the publish dependency-order edge stream directly while building graph state, avoiding an intermediate remapped edge vector.
  - Streamed configured CLI command usage rendering directly into one buffer, avoiding a temporary parts vector and final join.
  - Streamed MCP changeset validation text and semantic-item suggestions directly into strings, avoiding temporary cloned text and formatted example vectors.
  - Avoided redundant `BTreeSet` round-trips while deriving interactive configured change-type choices, and streamed group target display labels without joining package ids.
  - Streamed workspace-populate added command lists directly into the CLI result message, avoiding a temporary joined command string.
  - Streamed CLI help bordered-header rendering directly into the output buffer, avoiding a temporary line vector, joined header body, and repeated border strings.
  - Streamed CLI help option and see-also rendering directly into the output buffer, avoiding temporary label strings, padding strings, linked-command vectors, and joins.
  - Switched `monochange` and `xtask` entrypoints to a Tokio current-thread runtime to keep the CLI universally async while avoiding multi-thread runtime startup overhead for short direct step commands.

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

Measured with `hyperfine --warmup 1 --runs 6` on 200 packages, 500 changesets, 500 commits after the diagnose configuration-index follow-up.

| Command                                               |      main |       PR | PR/main | Status   |
| :---------------------------------------------------- | --------: | -------: | ------: | :------- |
| `mc step:config --dry-run`                            |    8.6 ms |   8.2 ms |   0.95× | improved |
| `mc step:validate --dry-run`                          |  651.9 ms | 518.2 ms |   0.79× | improved |
| `mc step:discover --dry-run --format json`            |  623.4 ms | 553.4 ms |   0.89× | improved |
| `mc step:display-versions --dry-run --format json`    | 1056.7 ms | 724.1 ms |   0.69× | improved |
| `mc step:create-change-file --dry-run`                |  914.0 ms | 593.5 ms |   0.65× | improved |
| `mc step:prepare-release --dry-run --format json`     | 2374.8 ms | 858.5 ms |   0.36× | improved |
| `mc step:affected-packages --dry-run --format json`   | 1442.3 ms |  35.8 ms |   0.02× | improved |
| `mc step:diagnose-changesets --dry-run --format json` | 3072.2 ms | 184.9 ms |   0.06× | improved |

Step-command benchmark violations: `0`.

### Low-gain follow-up after rebase

Measured with `hyperfine --warmup 1 --runs 3` against `origin/main` at `b79accfd3d11bbaab94fa8c8b508421615d9029e` after switching the CLI runtime to Tokio current-thread mode.

| Command                                            |      main |       PR | PR/main | Status   |
| :------------------------------------------------- | --------: | -------: | ------: | :------- |
| `mc step:config --dry-run`                         |  146.3 ms | 115.9 ms |   0.79× | improved |
| `mc step:display-versions --dry-run --format json` | 1037.8 ms | 970.1 ms |   0.93× | improved |

The two previously noisy or regressed direct step commands are now both faster than `main`; the full direct step-command rerun reported `0` regressions in `/tmp/monochange-step-current-thread-violations.txt`.

## Latest targeted optimization results

Measured on the 200-package / 500-changeset / 500-commit profiling fixture after rebuilding the current branch release binary.

| Command                                                                                                                                                          | Baseline |  Current | Result         |
| :--------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------: | -------: | :------------- |
| `mc step:diagnose-changesets --dry-run --format json` vs `main`                                                                                                  |  3.072 s | 184.9 ms | 16.61× faster  |
| `mc step:diagnose-changesets --dry-run --format json` vs pre-optimization async                                                                                  |  2.813 s | 173.7 ms | 16.20× faster  |
| `mc step:diagnose-changesets --dry-run --format json` vs prior shared-context iteration                                                                          | 596.4 ms | 173.7 ms | 3.43× faster   |
| `mc step:affected-packages --dry-run --format json --changed-paths crates/pkg-499/src/lib.rs` vs pre-optimization async                                          |  1.223 s |   7.7 ms | 158.95× faster |
| `mc step:affected-packages --dry-run --format json --changed-paths crates/pkg-99/src/lib.rs --changed-paths .changeset/change-0499.md` vs pre-optimization async |  1.244 s |  12.7 ms | 97.93× faster  |
| `mc step:affected-packages --dry-run --format json --from HEAD~1` vs pre-optimization async                                                                      |  1.248 s |  38.9 ms | 32.07× faster  |

The explicit no-changeset path now skips discovery entirely and validates config-path classification only. Attached changeset checks use a configuration-only package/group index, so PR policy runs that reference config ids no longer pay full manifest discovery cost. Diagnostics now use the same fast configuration-only index before falling back to full discovery, which removes manifest parsing from common `step:diagnose-changesets` runs.

Peak memory sampling with `/usr/bin/time -l` for `step:diagnose-changesets` improved further after the diagnostics fast path: max RSS went from about 22.1 MB to 19.5 MB, peak footprint from about 12.2 MB to 10.8 MB, and retired instructions from about 37.8B to 372.4M. For the explicit no-changeset affected-package path, max RSS dropped from about 16.2 MB to 13.1 MB, peak footprint from about 6.6 MB to 5.1 MB, and retired instructions from about 17.8B to 79.5M. For the explicit attached-changeset path, max RSS dropped from about 17.2 MB to 14.3 MB, peak footprint from about 7.4 MB to 5.6 MB, and retired instructions from about 18.1B to 142.4M.

## Main bottlenecks to tackle next

1. `step:diagnose-changesets` is much faster after skipping discovery for config-id changesets, now around `0.18 s` on the large fixture.
   - Remaining time is mostly unavoidable changeset file reads/parsing, prepared diagnostics construction, and JSON serialization.
   - Next work: profile JSON serialization and allocation-heavy prepared changeset fields before trying lower-level parsing changes.
2. `step:affected-packages` is now fast for both explicit no-changeset and config-id attached-changeset policy checks.
   - No-changeset explicit path checks complete in about `7.7 ms` on the large fixture.
   - Explicit config-id attached changeset checks complete in about `12.7 ms`; `--from HEAD~1` with git diff and an attached changeset completes in about `38.9 ms`.
   - Remaining work: rerun the full step-command matrix and look for non-policy affected-package cases that still fall back to full discovery.
3. `step:display-versions` now avoids cloning package/group ids and formatted versions while rendering its version summary.
   - JSON output streams version display values directly and text output writes into one buffer.
   - The low-gain follow-up rerun now shows this command faster than `main`; remaining work is to isolate prepare-release and serialization costs for additional headroom.
4. Large-fixture `mc step:validate` still spends roughly `0.49 s` on the PR branch.
   - Next work: identify whether validation repeatedly loads manifests or performs serial registry/source checks that can be cached or batched.
5. Release phase timings do not fully explain the wall-clock `mc release` / `mc release --dry-run` durations.
   - Next work: add or inspect outer phase spans around command startup, fixture setup, workflow orchestration, and subprocess boundaries so the remaining wall time is attributable.
6. Memory usage has targeted spot checks but not broad benchmark coverage yet.
   - Next work: add peak RSS measurements for the benchmarked command matrix, then target allocation-heavy structures with borrowed data, streaming iteration, or smaller owned summaries.

## Iteration checklist

- [ ] Add lightweight profiling for the slow step paths without affecting normal output.
- [ ] Measure peak RSS for top-level CLI and direct step-command benchmarks.
- [x] Profile `step:diagnose-changesets` and reduce repeated git or filesystem work.
- [x] Profile `step:affected-packages` and skip package graph discovery for no-changeset and config-id attached-changeset policy paths.
- [x] Profile `step:display-versions` and reduce cloning or serialization overhead.
- [x] Re-run the benchmark scripts after each optimization.
- [x] Keep this file updated with changes, results, and remaining bottlenecks.

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
