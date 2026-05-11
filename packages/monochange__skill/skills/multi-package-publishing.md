# Multi-package publishing

monochange can coordinate package publication after a release record exists. Publishing settings live in `[ecosystems.<name>.publish]` and `[package.<id>.publish]`.

Treat publishing as a separate phase from release preparation. Release preparation decides which versions should exist and records that decision; publishing checks registry state, handles first-time placeholder setup when needed, and then publishes only the packages selected by the release record and readiness artifacts.

## Recommended flow

1. Prepare and commit a release so a release record exists.
2. Run `mc publish-readiness --from HEAD --output readiness.json`.
3. Run any first-time bootstrap flow with `mc publish-bootstrap --from HEAD --output bootstrap.json` when packages are missing from registries.
4. Run configured publish planning and publish workflows if the repo defines them.
5. Store output artifacts so failed publishes can be resumed.

Readiness and publish artifacts are part of the safety model. They make a publish job auditable, let later steps confirm they are operating on the same release record, and help distinguish already-published packages from packages that still need work.

## Configuration pattern

```toml
[ecosystems.npm.publish]
enabled = true
mode = "builtin"
registry = "npm"
trusted_publishing = true

[package."@acme/private-app"]
path = "apps/private"
type = "npm"
publish = { enabled = false }

[package."@acme/custom-registry"]
path = "packages/custom-registry"
type = "npm"
publish = { enabled = true, mode = "external" }
```

Built-in publishing targets canonical public registries. Use external mode for private registries, custom scheduling, or registry-specific orchestration that monochange should not manage.

Prefer ecosystem-level defaults when every public package publishes the same way, then override individual package tables for private packages, custom registries, or packages that need a different trust model.

## Safety

- Do not run real publish commands when the user only asked for a preview.
- Prefer `mc publish-readiness` before package publication.
- Prefer dry-run workflows such as a configured `mc publish-check` when available.
- Retain JSON artifacts from readiness, bootstrap, plan, and publish runs.
- Re-run readiness when manifests, lockfiles, publish config, registry auth mode, or package selection changes after an artifact was created.
- Use dry-run or readiness commands for investigation; reserve actual package publication for explicit release operations by an authorized maintainer or CI workflow.
