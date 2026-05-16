# Async migration performance comparison

This branch should be measured against the current non-async `main` branch with two binaries built from the same checkout, toolchain, profile, and benchmark fixtures.

## What to compare

Use the existing CLI benchmark harness in `scripts/benchmark-cli.mjs`. It creates deterministic fixture repositories and measures these command paths with `hyperfine`:

- `mc step:validate`
- `mc step:discover --format json`
- `mc step:prepare-release --dry-run`
- `mc step:prepare-release` (the built-in step, equivalent to a typical `mc release` workflow command)

The two bundled scenarios are intentionally different:

- `baseline`: 20 packages, 50 changesets, 50 commits.
- `history_x10`: 200 packages, 500 changesets, 500 commits.

These should expose whether async process execution and hosted-source/workspace orchestration improve wall-clock time for larger repositories, while also catching regressions in small repositories.

Use `scripts/benchmark-step-commands.mjs` for built-in `mc step:<kebab>` coverage. It benchmarks all built-in step commands that can run safely against the offline fixture and reports the remaining provider/publish steps as explicit skips with setup rationale instead of silently omitting them.

## Local workflow

From a clean clone or worktree:

```bash
git fetch origin main feat/async-migration

workdir="$(mktemp -d)"
git worktree add "$workdir/main" origin/main
git worktree add "$workdir/async" feat/async-migration

cargo build --manifest-path "$workdir/main/Cargo.toml" --release -p monochange --bin mc
cargo build --manifest-path "$workdir/async/Cargo.toml" --release -p monochange --bin mc

node "$workdir/async/scripts/benchmark-cli.mjs" run \
  --main-bin "$workdir/main/target/release/mc" \
  --pr-bin "$workdir/async/target/release/mc" \
  --output "$workdir/async-performance.md" \
  --violations-output "$workdir/async-performance-violations.txt"

node "$workdir/async/scripts/benchmark-step-commands.mjs" run \
  --main-bin "$workdir/main/target/release/mc" \
  --pr-bin "$workdir/async/target/release/mc" \
  --output "$workdir/async-step-performance.md" \
  --violations-output "$workdir/async-step-performance-violations.txt"

cat "$workdir/async-performance.md"
cat "$workdir/async-step-performance.md"
```

Run this inside `devenv shell` if `hyperfine` or the pinned toolchain is not already available.

## Interpreting results

Use `main` as the control and `pr` as the async branch. Treat results as meaningful when they are stable across repeated runs and exceed normal noise:

- `pr < 0.98x main`: likely improvement.
- `0.98x..1.02x`: effectively flat.
- `pr > 1.02x main`: possible regression worth investigating.

For `release` and `release --dry-run`, inspect the phase tables as well as total wall-clock time. The async migration should show its value most clearly in phases that launch independent git/provider/process work.

For step-command benchmarks, use the pairwise `pr/main` summary from `benchmark-step-commands.mjs`; the raw `hyperfine` relative column is normalized against the fastest command in the whole table and is not a pairwise main-vs-PR ratio.

## Latest local run

On 2026-05-12, the async branch working tree on top of `ecb43db0` was compared with `origin/main` at `03d28640` using release binaries and `hyperfine --warmup 1 --runs 6`.

Generated reports:

- `/tmp/monochange-async-perf/async-performance.md`
- `/tmp/monochange-async-perf/step-bench.md`

Both violation files reported `0`:

- `/tmp/monochange-async-perf/async-performance-violations.txt`
- `/tmp/monochange-async-perf/step-bench-violations.txt`

Headline results from the stable pairwise summaries:

- Baseline fixture: `validate`, `discover`, `release --dry-run`, and `release` all improved.
- Large-history fixture: `discover`, `release --dry-run`, and `release` improved; `validate` was noisy in the latest local run but remains advisory.
- Step benchmark on 200 packages / 500 changesets / 500 commits: every measured safe built-in step command improved, including `step:prepare-release --dry-run --format json` at roughly `0.24x` main.

## PR automation proposal

Add a non-blocking PR workflow that:

1. Builds `origin/main` and the PR head in release mode.
2. Runs `scripts/benchmark-cli.mjs run` and `scripts/benchmark-step-commands.mjs run`.
3. Uploads the generated Markdown as artifacts.
4. Comments the Markdown on the PR.
5. Fails only when configured phase budgets are exceeded; otherwise reports improvement/regression as advisory data.

Keeping this advisory initially avoids blocking the async migration on machine-level benchmark variance while still giving reviewers a clear performance signal.
