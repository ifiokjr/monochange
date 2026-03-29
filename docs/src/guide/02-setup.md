# Setting up a project

Add a `monochange.toml` file at the repository root.

<!-- {=projectSetupConfig} -->

```toml
[defaults]
parent_bump = "patch"
warn_on_group_mismatch = true
package_type = "cargo"

[package.sdk-core]
path = "crates/sdk_core"
changelog = "crates/sdk_core/changelog.md"

[package.web-sdk]
path = "packages/web-sdk"
type = "npm"

[package.mobile-sdk]
path = "packages/mobile-sdk"
type = "dart"

[group.sdk]
packages = ["sdk-core", "web-sdk", "mobile-sdk"]
tag = true
release = true
version_format = "primary"

[ecosystems.npm]
enabled = true

[ecosystems.deno]
enabled = true

[ecosystems.dart]
enabled = true

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
```

<!-- {/projectSetupConfig} -->

<!-- {=projectSetupConfigNote} -->

This guide shows the preferred package/group configuration model.

If you are migrating from older config, replace legacy `[[version_groups]]` and `[[package_overrides]]` entries with `[group.<id>]` and `[package.<id>]` declarations before relying on `mc check` and `mc release`.

<!-- {/projectSetupConfigNote} -->

Then validate and verify discovery:

<!-- {=projectDiscoverCommand} -->

```bash
mc check --root .
mc workspace discover --root . --format json
```

<!-- {/projectDiscoverCommand} -->

Use configured package ids such as `sdk-core` and group ids such as `sdk` in changesets and CLI commands.
