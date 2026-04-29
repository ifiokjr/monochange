---
"@monochange/cli": patch
"monochange": patch
"monochange_telemtry": patch
---

# Add local-only telemetry events

Users can now opt in to local-only telemetry for CLI support and debugging without sending data over the network. By default, nothing is recorded. Setting `MC_TELEMETRY=local` writes OpenTelemetry-style JSON Lines events to the user state directory, while `MC_TELEMETRY_FILE=/path/to/telemetry.jsonl` writes to a chosen file.

Command:

```bash
MC_TELEMETRY=local mc validate
MC_TELEMETRY_FILE=/tmp/mc-telemetry.jsonl mc validate
```

The recorded events are limited to low-cardinality command and step metadata such as command name, step kind, duration, outcome, and sanitized error category. The local sink does not record package names, repository paths, repository URLs, branch names, tags, commit hashes, command strings, raw error messages, or release-note text.
