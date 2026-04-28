---
"monochange": patch
"@monochange/skill": patch
---

#### Add Cargo publish-readiness guards

Built-in crates.io publishing now fails readiness before registry mutation when the current `Cargo.toml` is not publishable: `publish = false`, `publish = [...]` without `crates-io`, missing `description`, or missing both `license` and `license-file`.

Workspace-inherited Cargo metadata is accepted, and already-published package versions remain non-blocking when the saved readiness artifact still matches current readiness.
