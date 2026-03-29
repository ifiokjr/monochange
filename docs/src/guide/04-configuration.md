# Configuration

Repository configuration lives in `monochange.toml`.

## Defaults

<!-- {=configurationDefaultsSnippet} -->

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true
```

<!-- {/configurationDefaultsSnippet} -->

## Version groups

<!-- {=configurationVersionGroupsSnippet} -->

```toml
[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk", "packages/mobile-sdk"]
strategy = "shared"
```

<!-- {/configurationVersionGroupsSnippet} -->

## Package overrides

`package_overrides` currently let you point released packages at changelog files that should be updated by `PrepareRelease`.

<!-- {=configurationPackageOverridesSnippet} -->

```toml
[[package_overrides]]
package = "crates/sdk_core"
changelog = "crates/sdk_core/CHANGELOG.md"

[[package_overrides]]
package = "packages/web-sdk"
changelog = "packages/web-sdk/CHANGELOG.md"
```

<!-- {/configurationPackageOverridesSnippet} -->

## Workflows

Workflows are user-defined top-level commands. In this milestone, a workflow name such as `release` becomes invocable as `mc release`.

<!-- {=configurationWorkflowsSnippet} -->

```toml
[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"

[[workflows.steps]]
type = "Command"
command = "printf '%s\n' '$version'"
```

<!-- {/configurationWorkflowsSnippet} -->

Workflow command interpolation variables:

<!-- {=configurationWorkflowVariables} -->

- `$version` — one shared release version when all released packages resolve to the same version
- `$group_version` — one shared synced version across released version groups, falling back to `$version`
- `$released_packages` — comma-separated released package names
- `$changed_files` — space-separated changed file paths
- `$changesets` — space-separated consumed `.changeset/*.md` paths

<!-- {/configurationWorkflowVariables} -->

## Ecosystem settings

These settings are parsed from config and document intended control points for discovery:

<!-- {=configurationEcosystemSettingsSnippet} -->

```toml
[ecosystems.cargo]
enabled = true
roots = ["crates/*"]
exclude = ["crates/experimental/*"]

[ecosystems.npm]
enabled = true
roots = ["packages/*"]
exclude = ["packages/legacy/*"]

[ecosystems.deno]
enabled = true

[ecosystems.dart]
enabled = true
```

<!-- {/configurationEcosystemSettingsSnippet} -->

## Package references

<!-- {=configurationPackageReferenceRules} -->

Package references may use package ids, package names, manifest-relative paths, or manifest-directory paths.

<!-- {/configurationPackageReferenceRules} -->

## Current status

<!-- {=configurationCurrentStatus} -->

Current implementation status:

- actively used today: `defaults.parent_bump`, `defaults.warn_on_group_mismatch`, `version_groups`, `package_overrides.changelog`, and `workflows`
- parsed but not currently enforced by discovery or planning: `defaults.include_private`, `version_groups.strategy`, and `[ecosystems.*].enabled/roots/exclude`
- workflow names must be unique, must not collide with built-in commands, and must contain at least one step
- supported workflow steps today: `PrepareRelease` and `Command`

<!-- {/configurationCurrentStatus} -->
