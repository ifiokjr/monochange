# Publish progress and parallel ecosystems

## Why

`mc publish` currently depends on telemetry and log-level output for most of its visibility. That makes local terminal runs feel stalled and makes CI failures hard to diagnose without digging through structured traces. Publishing also runs as one sequential list, so an npm-heavy run can finish all npm work before crates.io starts, even when ecosystems are independent.

This work makes release operations easier to trust by giving users readable progress on stderr while preserving machine-readable reports on stdout/files. It also prepares publish execution for safe ecosystem-level parallelism so independent registries can make progress at the same time without breaking dependency order.

## Goals

- Add a reusable progress reporter that writes to stderr.
- Use emojis for ecosystem identity in both terminals and CI logs.
- Use loading indicators only for interactive terminals; CI gets deterministic start/finish lines with emojis and no spinner noise.
- Let each ecosystem define its own progress emoji through a trait-like abstraction instead of hardcoding all presentation at call sites.
- Add publish progress first, then broaden progress to all CLI steps, then add safe parallel publish lanes by ecosystem.
- Keep JSON/stdout outputs stable unless a command explicitly renders human output.

## Non-goals

- Do not publish any packages while implementing or validating this work.
- Do not change release planning semantics.
- Do not run same-ecosystem packages in parallel in the first parallel-publish pass.
- Do not replace telemetry; progress output is complementary.

## Affected areas

- `crates/monochange/src/cli_runtime.rs` — CLI step start/finish/skip progress.
- `crates/monochange/src/cli_theme.rs` / new progress module — shared styling and stderr rendering.
- `crates/monochange/src/package_publish.rs` — wire publish progress from the app layer.
- `crates/monochange_publish/src/lib.rs` — publish events, ecosystem emoji trait, and later ecosystem scheduler.
- `crates/monochange*/src/__tests__/` — focused unit coverage for changed executable lines.
- CLI snapshots/integration tests where human output intentionally changes.

## PR split

### 1. Progress reporter foundation + publish progress

- [ ] Add a small progress abstraction with two renderers:
  - interactive terminal renderer with spinner-style status updates,
  - CI/plain renderer with deterministic emoji start/finish lines.
- [ ] Add an ecosystem presentation trait or trait-like helper so ecosystems own their emoji and label.
- [ ] Emit publish events for:
  - publish run start and completion,
  - ecosystem lane/package start,
  - registry check,
  - skip existing/external,
  - dry-run planned publish,
  - published,
  - blocked/failed.
- [ ] Keep progress on stderr and existing reports on stdout/artifacts.
- [ ] Add tests for emoji labels, CI/plain output, and publish event sequencing.
- [ ] Validate with `cargo fmt`, targeted tests, `cargo clippy -q -p monochange --all-targets --all-features -- -D warnings`, and `devenv shell mc step:validate`.

### 2. Progress across CLI steps

- [ ] Emit step-level progress in `cli_runtime`:
  - command workflow start/finish,
  - step start/finish,
  - skipped steps with `when` context,
  - failure context before returning errors.
- [ ] Add step-specific concise summaries for discover, validate/check/lint, prepare release, commit/tag/open release request, publish readiness, issue comments, affected packages, and retargeting.
- [ ] Ensure JSON output commands remain parseable by keeping progress on stderr.
- [ ] Add tests/snapshots for CI/plain stderr formatting where the harness supports it.

### 3. Parallel publish by ecosystem

- [ ] Build a dependency-aware scheduler that only releases a package when its publish dependencies succeeded or were already published/skipped existing.
- [ ] Partition runnable work by ecosystem and run one sequential lane per ecosystem.
- [ ] Preserve dependency order inside each ecosystem lane.
- [ ] Preserve stable `PackagePublishReport.packages` ordering by original plan order, not completion order.
- [ ] Stop scheduling new work after failure, while allowing in-flight ecosystem lanes to finish and report outcomes.
- [ ] Add tests for independent npm/cargo concurrency, cross-ecosystem dependencies, failure behavior, and stable report ordering.
- [ ] Move this plan to `docs/plans/completed/` when the third PR lands.

## Decisions

- Use emojis, not nerdfonts, for portability and readability.
- Emojis are allowed in CI logs; only spinner/loading animation is terminal-only.
- stderr is the default channel for progress so stdout remains usable for JSON and command composition.
- Parallelism starts at ecosystem granularity, not package granularity, to limit registry-rate and dependency-order risk.

## Open questions

- Whether to add an explicit `--no-progress`/`NO_COLOR`-style opt-out for progress lines or rely on stdout/stderr separation.
- Whether the progress abstraction should live in `monochange` only first, then move to `monochange_core`/`monochange_publish` once the parallel scheduler needs it across crates.
- Whether parallel publish should be opt-in for one release before becoming default.
