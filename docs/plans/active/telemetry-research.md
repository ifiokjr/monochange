# Telemetry research

## Status

- Date: 2026-04-28
- Branch/worktree: `chore/telemetry-research`
- Scope: research, local-only telemetry implementation, and follow-up issue planning.
- Initial implementation added local OpenTelemetry-style JSONL command/step events behind `MC_TELEMETRY` / `MC_TELEMETRY_FILE`.

## Problem statement

monochange is a Rust CLI and workspace library for planning releases in multi-ecosystem monorepos. Today it has local diagnostics through `tracing`, `tracing-subscriber`, and logging configured from `RUST_LOG` / `--log-level`, but it does not send product telemetry or analytics anywhere.

The project could benefit from faster feedback about which commands, release flows, package ecosystems, and APIs are actually used. That feedback would help prioritize work while avoiding guesses based only on issues and maintainer intuition.

## What telemetry means here

Telemetry in this context means deliberately emitted, privacy-aware operational and product signals from the monochange CLI and library. It is different from local logging:

- **Local tracing/logging**: developer-controlled diagnostics printed locally or written to a local collector. Useful for debugging a single run.
- **Product telemetry**: aggregate usage events such as command names, step kinds, durations, outcomes, feature flags used, workspace shape counts, and sanitized error categories. Useful for understanding what users do across many installations.
- **Error/performance monitoring**: crash reports, panic reports, and slow path traces. Useful for quality but more sensitive and not the same as feature analytics.

For monochange, telemetry should be treated as an explicit product feature with a published privacy policy, opt-in or at least clear opt-out, and a strict schema that avoids collecting repository contents or package names by default.

## High-value feedback questions

Telemetry would be most useful if it answers questions like:

- Which top-level commands are used: `validate`, `discover`, `change`, release commands, publish-related commands, and generated `step:*` commands?
- Which `CliStepDefinition` variants are common or unused?
- Which ecosystems are actually present in discovered workspaces: Cargo, npm, Deno, Dart, Flutter?
- How often do users run dry-run versus real release/publish flows?
- Which output/progress formats are used: markdown/json, unicode/ascii/json progress?
- Which release/publish flows fail most often, and at what step?
- How long do discovery, release planning, publishing, and hosted-source operations take?
- Which optional API surfaces are used by downstream crates versus only by the binary? This is better answered with explicit library instrumentation or docs/API surveys, not just CLI telemetry.

## Candidate instrumentation points

### CLI command lifecycle

File: `crates/monochange/src/cli_runtime.rs`

- `execute_matches` resolves the command name, flags such as `--dry-run`, `--diff`, `--progress-format`, and CLI inputs.
- `execute_cli_command_with_options` has a single command run boundary with `command_started_at` and `progress.command_finished(...)`.
- Recommended event: `command_run` with command name, monochange version, dry-run, diff flag, progress format, step count, duration, exit status, and sanitized error kind.

### Step lifecycle

File: `crates/monochange/src/cli_runtime.rs`

- The loop over `cli_command.steps.iter().enumerate()` already records `step_started_at` and has success/failure handling.
- `CliStepDefinition::kind_name()` provides a stable low-cardinality step kind.
- Recommended event: `command_step` with command name, step index, step kind, skipped/executed, duration, phase timings where available, and sanitized outcome.
- Avoid sending rendered command strings, shell args, template inputs, refs, package IDs, file paths, or release notes unless a later explicit debug mode is added.

### Progress/reporting

File: `crates/monochange/src/cli_progress.rs`

- `CliProgressReporter` already centralizes user-visible command/step state transitions.
- It is a useful reference for lifecycle naming, but telemetry should not be coupled directly to terminal output because progress can be disabled or formatted as JSON.
- If reused, keep a separate `TelemetryReporter` trait and call both reporters from `cli_runtime.rs`.

### Discovery/workspace shape

File: `crates/monochange/src/workspace_ops.rs`

- `discover_workspace` and `discover_packages` detect packages across supported ecosystems.
- Recommended event properties: total package count, ecosystem counts, version group count, CLI command count, source provider kind if configured, whether changesets/changelog are configured.
- Do not send package names, paths, repository URLs, branch names, or manifest content by default.

### Release planning and publish flows

Files:

- `crates/monochange/src/cli_runtime.rs`
- `crates/monochange/src/package_publish.rs`
- `crates/monochange/src/publish_rate_limits.rs`
- `crates/monochange/src/prepared_release_cache.rs`

High-signal events:

- `release_prepare` — packages changed count, files planned count, phase timings, dry-run.
- `release_publish` — provider kind, request count, dry-run, outcome category.
- `package_publish` — selected package count, ecosystem counts, dry-run, trust/rate-limit outcome categories.
- `prepared_release_cache` — cache hit/miss and artifact save/load outcome.

### Hosted source integrations

Files around GitHub/GitLab/Gitea adapters and hosted-source runtime should emit only provider kind and outcome category. Do not send repository owner/name, issue numbers, PR numbers, URLs, or tokens.

## Data classification proposal

### Safe by default

- monochange version
- OS family and architecture
- command name and configured step kind names
- boolean flags: dry-run, quiet, diff, progress format
- counts: package count, ecosystem counts, version group count, step count
- duration buckets or milliseconds
- outcome category: success, canceled, config_error, network_error, provider_error, publish_error, unknown_error
- randomly generated anonymous installation ID, resettable by the user

### Avoid by default

- package names
- workspace paths
- repository URLs, owners, names, branches, tags, refs, commit SHAs
- command strings and shell arguments
- changeset text, release notes, changelog bodies, issue IDs, PR IDs
- environment variable names/values, tokens, usernames, emails
- exact error messages unless sanitized and reviewed

### Optional debug-only mode, if ever added

A future `monochange telemetry export` or `monochange doctor --telemetry-bundle` could create a local support bundle that the user can inspect and manually attach to an issue. That should be separate from automatic product telemetry.

## Privacy and UX guardrails

Recommended guardrails before implementation:

- Prefer **opt-in** telemetry for the initial release.
- Provide a persistent status command, e.g. `monochange telemetry status`.
- Provide explicit commands or environment variables to enable/disable:
  - `monochange telemetry enable`
  - `monochange telemetry disable`
  - `MC_TELEMETRY=1`
  - `MC_TELEMETRY=0`
- Show what is collected in docs and ideally expose a local preview/export.
- Generate a random anonymous installation ID stored in a user config file, and allow reset.
- Keep event schemas low-cardinality and documented in-repo.
- Never block CLI success on telemetry delivery.
- Never send telemetry from tests unless explicitly enabled with a mock sink.
- Batch and send asynchronously/best-effort with short timeouts.
- Disable automatically in common CI environments unless explicitly enabled.

Homebrew and GitHub CLI are useful UX references: both emphasize public documentation, limited collection, and user controls.

## Rust telemetry library research

Versions below were checked with `cargo search` / `cargo info` on 2026-04-28.

| Option               | Current crate(s) checked                                                                                                | Primary fit                           | Notes for monochange                                                                                                                                                                                                                                                                          |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| OpenTelemetry        | `opentelemetry = 0.31.0`, `opentelemetry_sdk = 0.31.0`, `opentelemetry-otlp = 0.31.1`, `tracing-opentelemetry = 0.32.1` | Standards-based traces, metrics, logs | Best fit if monochange wants vendor-neutral observability or to export existing `tracing` spans. Less ideal alone for product analytics dashboards unless paired with a collector/backend. Rust docs list traces/metrics/logs as beta. MSRV is lower than monochange's current Rust `1.90.0`. |
| PostHog              | `posthog-rs = 0.5.3`                                                                                                    | Product analytics, feature flags      | Strong fit for command/feature usage analytics. Official Rust client supports async client by default; docs mention capture and feature flag concepts. Requires selecting a PostHog project/host and being clear about privacy. MSRV `1.78.0`.                                                |
| Sentry               | `sentry = 0.47.0`, `sentry-tracing = 0.47.0`, `sentry-opentelemetry = 0.47.0`                                           | Error and performance monitoring      | Good fit for panic/error reporting and performance traces. Not the best primary tool for product analytics. Higher MSRV `1.88`, still below monochange's Rust `1.90.0`. Needs careful PII scrubbing.                                                                                          |
| RudderStack          | `rudderanalytics = 1.1.4`                                                                                               | Analytics routing/CDP                 | Useful if events should be routed to multiple destinations. Adds an analytics pipeline/vendor decision. Rust version was not declared in crate metadata.                                                                                                                                      |
| Segment-style client | `analytics = 0.2.0`                                                                                                     | Segment tracking API                  | Small/simple Segment-style tracking client, but older and less obviously maintained. Rust version was not declared in crate metadata.                                                                                                                                                         |
| OpenFeature          | `open-feature = 0.3.0`, `open-feature-flagd = 0.1.0`                                                                    | Feature flags, not telemetry          | Useful if monochange later wants remote feature flags or experiments. Not a telemetry/event collection system. MSRV `1.80.1`.                                                                                                                                                                 |
| Custom minimal sink  | Existing `reqwest` workspace dependency                                                                                 | Product analytics with strict control | Could send a small documented JSON event envelope to a monochange endpoint or PostHog HTTP API without coupling the core architecture to a vendor SDK. More maintenance burden, but easier to guarantee schema/privacy and avoid heavy dependencies.                                          |

## Recommendation

Start with a **small internal telemetry abstraction** and defer vendor lock-in.

Recommended first implementation shape:

1. Add a dedicated telemetry crate, `crates/monochange_telemetry`, and import it from the main `monochange` crate.
2. Define a `TelemetrySink` trait with a no-op implementation as the default.
3. Define documented event structs/enums for command, step, discovery, release, and publish events.
4. Gate all collection behind explicit opt-in configuration and `MC_TELEMETRY`.
5. Instrument `execute_matches` / `execute_cli_command_with_options` first because it covers every CLI command and step with minimal code spread.
6. Add discovery/workspace-shape telemetry next, using only aggregate counts.
7. Choose a backend after event schemas are stable:
   - PostHog if the goal is product analytics and feature usage dashboards.
   - OpenTelemetry if the goal is vendor-neutral traces/metrics integrated with existing `tracing` spans.
   - Sentry as a separate opt-in error-reporting layer, not the main product telemetry mechanism.

The most practical path is probably:

- Phase 1: no-op abstraction + local debug sink + docs.
- Phase 2: opt-in product analytics sink, likely PostHog or a minimal HTTP sink using `reqwest`.
- Phase 3: optional Sentry/OpenTelemetry integration for maintainers or enterprise users who want their own observability backend.

## Suggested event schema draft

```text
Event: command_run
Properties:
  command_name: string
  command_source: configured | generated_step
  monochange_version: string
  os_family: string
  arch: string
  dry_run: bool
  diff: bool
  progress_format: auto | unicode | ascii | json
  step_count: integer
  duration_ms: integer
  outcome: success | error
  error_kind: optional enum
```

```text
Event: command_step
Properties:
  command_name: string
  step_index: integer
  step_kind: string
  skipped: bool
  duration_ms: integer
  phase_count: integer
  outcome: success | skipped | error
  error_kind: optional enum
```

```text
Event: workspace_discovered
Properties:
  package_count: integer
  cargo_package_count: integer
  npm_package_count: integer
  deno_package_count: integer
  dart_package_count: integer
  flutter_package_count: integer
  version_group_count: integer
  cli_command_count: integer
  has_source_provider: bool
  source_provider_kind: optional enum
  has_changelog_config: bool
  has_changeset_config: bool
```

```text
Event: release_flow
Properties:
  command_name: string
  dry_run: bool
  prepared_package_count: integer
  changed_file_count: integer
  provider_kind: optional enum
  publish_request_count: integer
  outcome: success | error
  error_kind: optional enum
```

## Possible follow-up tasks

- [x] Write `docs/telemetry.md` with privacy policy, event list, opt-in/opt-out UX, and examples.
- [x] Add an internal telemetry abstraction with a no-op sink and a local JSON debug sink for tests.
- [x] Add telemetry settings resolution for env-based local telemetry.
- [x] Instrument `execute_cli_command_with_options` for command and step lifecycle events.
- [ ] Add persistent telemetry settings and CI-specific defaults; tracked in [#298](https://github.com/monochange/monochange/issues/298).
- [ ] Instrument `discover_workspace` / `discover_packages` for aggregate workspace shape events; tracked in [#296](https://github.com/monochange/monochange/issues/296).
- [x] Add sanitized error classification for the core error enum without sending raw messages.
- [ ] Decide backend: PostHog for product analytics, OpenTelemetry for standards-based traces, Sentry for error monitoring; remote OpenTelemetry tracked in [#297](https://github.com/monochange/monochange/issues/297).
- [ ] Add stronger tests ensuring telemetry is disabled by default, never sends during tests by default, and redacts paths/package names; tracked in [#299](https://github.com/monochange/monochange/issues/299).
- [ ] Add a `mc telemetry status|enable|disable|reset-id|preview` command set if the UX is accepted; tracked in [#295](https://github.com/monochange/monochange/issues/295).
- [ ] Document self-hosted backend options; tracked in [#300](https://github.com/monochange/monochange/issues/300).

## Acceptance checks for a future implementation

- `cargo test --workspace`
- `lint:all`
- `mc validate`
- Manual checks:
  - fresh install reports telemetry disabled or asks for opt-in without sending anything
  - `MC_TELEMETRY=0` suppresses all network sends
  - CI disables telemetry unless explicitly enabled
  - failing network telemetry delivery does not change command exit status
  - telemetry preview contains no paths, package names, repository URLs, or raw command strings

## Open questions

- Should telemetry be opt-in forever, or opt-out after a prominent first-run notice? Research recommends starting opt-in.
- Where should user telemetry configuration live across platforms?
- Should library users have telemetry at all, or only the CLI binary?
- Is monochange comfortable using a third-party SaaS analytics backend, or should it expose OpenTelemetry/export hooks and let users choose?
- What retention period and public transparency commitments should be documented?
