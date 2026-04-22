---
"monochange": patch
---

#### harden publish planning and placeholder bootstrap checks

`mc publish-plan`, `mc publish`, and `mc placeholder-publish` now respect the current workspace publishability rules instead of trusting stale release metadata or exact placeholder versions.

For `mc publish-plan --format json`, cargo batches previously included crates with `publish = false`, and release-record entries could keep npm or other ecosystem packages in the plan even after publishing was disabled.

Now publish batches skip packages that are currently private or excluded in discovery, and they also skip packages whose effective publish settings are disabled in the workspace configuration.

For `mc placeholder-publish --dry-run --format json`, placeholder bootstrap checks previously only looked for the exact `0.0.0` version, so a package that already had `1.0.0` on the registry could still be treated as needing a placeholder release.

Now placeholder planning skips any package that already has **any** version on its registry, and npm `setupUrl` values now point at:

```text
https://www.npmjs.com/package/<package>/access
```

`mc publish-plan` also falls back to the crates.io sparse index when the crates.io API denies package lookups, which keeps rate-limit planning working in CI environments that return `403 Forbidden` from the API endpoint.
