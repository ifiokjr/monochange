## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-29)

### Changed

#### Add local-only telemetry events

Users can now opt in to local-only telemetry for CLI support and debugging without sending data over the network. By default, nothing is recorded. Setting `MC_TELEMETRY=local` writes OpenTelemetry-style JSON Lines events to the user state directory, while `MC_TELEMETRY_FILE=/path/to/telemetry.jsonl` writes to a chosen file.

Command:

```bash
MC_TELEMETRY=local mc validate
MC_TELEMETRY_FILE=/tmp/mc-telemetry.jsonl mc validate
```

The recorded events are limited to low-cardinality command and step metadata such as command name, step kind, duration, outcome, and sanitized error category. The local sink does not record package names, repository paths, repository URLs, branch names, tags, commit hashes, command strings, raw error messages, or release-note text.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #302](https://github.com/monochange/monochange/pull/302) _Introduced in:_ [`72a126c`](https://github.com/monochange/monochange/commit/72a126cf0789ffbf1c8866f043f307c9f570b088) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Related issues:_ [#295](https://github.com/monochange/monochange/issues/295), [#296](https://github.com/monochange/monochange/issues/296), [#297](https://github.com/monochange/monochange/issues/297), [#298](https://github.com/monochange/monochange/issues/298), [#299](https://github.com/monochange/monochange/issues/299), [#300](https://github.com/monochange/monochange/issues/300)
