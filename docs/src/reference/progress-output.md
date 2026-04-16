# Progress output

monochange writes progress information to stderr so stdout can remain stable for text, markdown, and JSON command results.

## Selecting a renderer

Use the global `--progress-format <FORMAT>` flag or set `MONOCHANGE_PROGRESS_FORMAT`.

Supported values:

- `auto`: default behavior. Human progress output is enabled only when stderr is a terminal.
- `unicode`: force the human renderer with Unicode symbols and spinners.
- `ascii`: force the human renderer with ASCII-safe symbols.
- `json`: emit newline-delimited JSON progress events on stderr.

`--quiet` suppresses progress output. `MONOCHANGE_NO_PROGRESS=1` also disables the automatic human renderer.

## Human progress output

The human renderer is designed for interactive terminal runs:

- step labels use each step's `name = "..."` value when present, then fall back to the built-in step kind
- long-running steps show a delayed spinner so short steps do not flicker
- command stdout and stderr stream live under the active step
- completed `PrepareRelease` and `DisplayVersions` steps print per-phase timings so slow phases are visible without a separate trace

Built-in commands already attach descriptive step names such as `prepare release`, `publish release`, and `open release request`. Custom commands can override those names per step.

## JSON event stream

`--progress-format json` is intended for machines, not humans. It writes one JSON object per line to stderr.

Common lifecycle events:

- `command_started`
- `step_started`
- `command_output`
- `step_finished`
- `step_failed`
- `step_skipped`
- `command_finished`

Shared fields:

- `sequence`: monotonically increasing event sequence number for the command run
- `command`: CLI command name, such as `release`
- `dryRun`: whether the command is running in dry-run mode
- `totalSteps`: total step count for the command
- `stepIndex`: 1-based step index for step events
- `stepKind`: built-in step kind, such as `PrepareRelease`
- `stepDisplayName`: rendered human label for the step
- `stepName`: explicit configured `name`, or `null` when omitted

Event-specific fields:

- `command_output` adds `stream` and `text`
- `step_finished` adds `durationMs` and `phaseTimings`
- `step_failed` adds `durationMs` and `error`
- `step_skipped` may add `condition`
- `command_finished` adds `durationMs`

Example:

```json
{"sequence":0,"event":"command_started","command":"release","dryRun":true,"totalSteps":2}
{"sequence":1,"event":"step_started","command":"release","dryRun":true,"stepIndex":1,"totalSteps":2,"stepKind":"PrepareRelease","stepDisplayName":"plan release","stepName":"plan release"}
{"sequence":2,"event":"step_finished","command":"release","dryRun":true,"stepIndex":1,"totalSteps":2,"stepKind":"PrepareRelease","stepDisplayName":"plan release","stepName":"plan release","durationMs":243,"phaseTimings":[{"label":"discover release workspace","durationMs":97}]}
```

## Benchmark integration

The binary benchmark workflow uses `--progress-format json` to extract `PrepareRelease` phase timings for both `mc release --dry-run` and `mc release`.

Those timings are summarized and compared against `.github/scripts/benchmark_phase_budgets.json`, which lets pull requests fail when real release-path regressions exceed the configured budget.

For hosted-provider analysis outside CI, `.github/scripts/benchmark_cli.sh run-fixture` can benchmark an existing repository checkout and render the same markdown summary against a real hosted fixture. See [Hosted release benchmarks](./hosted-release-benchmarks.md).
