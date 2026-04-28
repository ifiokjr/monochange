<!-- {@monochangeLocalTelemetryDocs} -->

monochange can write local-only telemetry events for CLI command and step execution. The first implementation does not send data over the network and does not require a telemetry backend.

## Current scope

This release only supports a local JSON Lines sink with OpenTelemetry-style event envelopes. It is intended for debugging, support bundles, and validating the event schema before any hosted or remote telemetry work is considered.

## Enabling local telemetry

Telemetry is disabled by default. Enable the local sink with environment variables:

```sh
MC_TELEMETRY=local mc validate
```

By default, events are appended to:

```text
$XDG_STATE_HOME/monochange/telemetry.jsonl
```

When `XDG_STATE_HOME` is not set, monochange falls back to:

```text
$HOME/.local/state/monochange/telemetry.jsonl
```

For a one-off file path, set `MC_TELEMETRY_FILE`:

```sh
MC_TELEMETRY=local MC_TELEMETRY_FILE=/tmp/mc-telemetry.jsonl mc validate
```

Setting only `MC_TELEMETRY_FILE` also enables the local sink for that command:

```sh
MC_TELEMETRY_FILE=/tmp/mc-telemetry.jsonl mc discover
```

Disable telemetry explicitly with any of:

```sh
MC_TELEMETRY=0
MC_TELEMETRY=false
MC_TELEMETRY=off
MC_TELEMETRY=disabled
```

## Events

### `command_run`

Emitted when a CLI command completes or fails after command execution starts.

Attributes:

- `command_name`
- `command_source`: `configured` or `generated_step`
- `dry_run`
- `show_diff`
- `progress_format`: `auto`, `unicode`, `ascii`, or `json`
- `step_count`
- `duration_ms`
- `outcome`: `success` or `error`
- `error_kind`: sanitized error category or `null`

### `command_step`

Emitted for each CLI step that succeeds, fails, or is skipped.

Attributes:

- `command_name`
- `step_index`
- `step_kind`
- `skipped`
- `duration_ms`
- `outcome`: `success`, `skipped`, or `error`
- `error_kind`: sanitized error category or `null`

## Local event shape

Each line is one JSON object:

```json
{
	"resource": {
		"service.name": "monochange",
		"service.version": "0.2.0"
	},
	"scope": {
		"name": "monochange.telemetry",
		"version": "0.1.0"
	},
	"time_unix_nano": 1777338000000000000,
	"severity_text": "INFO",
	"body": {
		"string_value": "command_run"
	},
	"attributes": {
		"command_name": "validate",
		"command_source": "configured",
		"dry_run": false,
		"show_diff": false,
		"progress_format": "auto",
		"step_count": 1,
		"duration_ms": 42,
		"outcome": "success",
		"error_kind": null
	}
}
```

## Privacy boundaries

The local sink intentionally records only low-cardinality command metadata and sanitized error categories. It does not record package names, paths, repository URLs, branch names, tag names, commit hashes, issue numbers, pull request numbers, shell command strings, environment values, changeset content, changelog content, release notes, or raw error messages.

## Future work

Remote export, a user-facing telemetry command group, persistent opt-in configuration, hosted dashboards, and richer workspace-shape events are tracked as follow-up issues. Until those are implemented, telemetry remains local-only and best-effort.

<!-- {/monochangeLocalTelemetryDocs} -->
