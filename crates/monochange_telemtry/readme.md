# `monochange_telemtry`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_telemtry"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange**telemtry-orange?logo=rust)](https://crates.io/crates/monochange_telemtry) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange**telemtry-1f425f?logo=docs.rs)](https://docs.rs/monochange_telemtry/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_telemtry)](https://codecov.io/gh/monochange/monochange?flag=monochange_telemtry) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeTelemtryCrateDocs} -->

`monochange_telemtry` provides local-only telemetry primitives for the `monochange` CLI.

Reach for this crate when you need the reusable event sink, event payloads, and privacy-preserving error classification that power opt-in local JSONL telemetry. The crate intentionally keeps transport simple: it appends OpenTelemetry-style JSON Lines records to a local file and does not send telemetry over the network.

## Why use it?

- keep telemetry capture separate from CLI orchestration and package discovery
- share one local JSONL event schema across command and step instrumentation
- classify errors into low-cardinality categories without exposing raw error text
- make telemetry writes best-effort so observability cannot change command outcomes

## Best for

- embedding monochange's local telemetry sink in the CLI runtime
- smoke-testing event schemas without provisioning a backend
- building future telemetry commands, exporters, or redaction tests on top of a small public API

## Public entry points

- `TelemetrySink::from_env()` resolves `MC_TELEMETRY` and `MC_TELEMETRY_FILE` into either a disabled sink or a local JSONL sink
- `TelemetrySink::capture_command(...)` writes `command_run` events
- `TelemetrySink::capture_step(...)` writes `command_step` events
- `CommandTelemetry`, `StepTelemetry`, and `TelemetryOutcome` describe the stable event payloads

## Privacy boundaries

The crate only accepts low-cardinality command metadata, booleans, counts, durations, enum outcomes, and sanitized `error_kind` values. It does not collect package names, paths, repository URLs, branch names, refs, commit hashes, shell command strings, environment values, changeset text, release notes, issue or pull request IDs, or raw errors.

## Example

```rust
use monochange_telemtry::CommandTelemetry;
use monochange_telemtry::TelemetryOutcome;
use monochange_telemtry::TelemetrySink;
use std::time::Duration;

let sink = TelemetrySink::Disabled;
sink.capture_command(CommandTelemetry {
    command_name: "validate",
    dry_run: false,
    show_diff: false,
    progress_format: "auto",
    step_count: 1,
    duration: Duration::from_millis(42),
    outcome: TelemetryOutcome::Success,
    error: None,
});
```

<!-- {/monochangeTelemtryCrateDocs} -->
