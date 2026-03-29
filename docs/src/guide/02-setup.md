# Setting up a project

Add a `monochange.toml` file at the repository root.

<!-- {=projectSetupConfig} -->

```toml
[defaults]
parent_bump = "patch"
warn_on_group_mismatch = true

[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk"]

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
```

<!-- {/projectSetupConfig} -->

<!-- {=projectSetupConfigNote} -->

This is the smallest config needed to make `mc release` work in the current implementation.

For changelog updates, add `[[package_overrides]]` entries with `changelog` paths. Discovery currently scans all supported ecosystems automatically; the top-level `[ecosystems.*]` settings are parsed today but are not yet used to filter discovery.

<!-- {/projectSetupConfigNote} -->

Then verify discovery:

<!-- {=projectDiscoverCommand} -->

```bash
mc workspace discover --root . --format json
```

<!-- {/projectDiscoverCommand} -->
