---
monochange_publish: minor
---

# Add progress logging to `mc publish`

When running `mc publish`, each package being processed is now logged via `tracing::info!` so users can observe progress in real time. Use `--log-level info` or set `RUST_LOG=info` to see these messages. When `--quiet` is set, no tracing subscriber is initialized so the log messages are silently discarded (zero overhead).

Log events emitted during the publish loop:

- **`publishing package`** — at the start of processing each package, with `package_name`, `version`, `registry`, `dry_run`, and `mode` fields
- **`skipping external package`** — when a package opts out of built-in publishing
- **`skipping already-published version`** — when the version already exists on the registry
- **`would publish package (dry run)`** — when `--dry-run` would publish the package
- **`published package`** — on successful publish
- **`publish command failed to execute`** (`tracing::error`) — when the publish command cannot run
- **`publish command returned non-zero exit`** (`tracing::error`) — when the publish command fails
