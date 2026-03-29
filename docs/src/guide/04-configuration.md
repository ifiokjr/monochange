# Configuration

Repository configuration lives in `monochange.toml`.

## Defaults

<!-- {=configurationDefaultsSnippet} -->

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true
package_type = "cargo"
changelog = "{path}/changelog.md"
```

<!-- {/configurationDefaultsSnippet} -->

## Packages

Declare every release-managed package explicitly.

<!-- {=configurationVersionGroupsSnippet} -->

```toml
[defaults]
package_type = "cargo"
changelog = "{path}/changelog.md"

[package.sdk-core]
path = "crates/sdk_core"
versioned_files = ["crates/sdk_core/extra.toml"]
tag = false
release = false
version_format = "namespaced"
```

<!-- {/configurationVersionGroupsSnippet} -->

Required fields:

- `path`
- `type`, unless `[defaults].package_type` is set

Supported `type` values:

- `cargo`
- `npm`
- `deno`
- `dart`
- `flutter`

Optional package fields:

- `type`, when `[defaults].package_type` is set
- `changelog`
- `versioned_files`
- `tag`
- `release`
- `version_format`

`changelog` accepts three forms on packages:

- `true` → use `{path}/CHANGELOG.md`
- `false` → disable the package changelog
- `"some/path.md"` → use that exact path

`[defaults].changelog` also accepts three forms:

- `true` → default every package to `{path}/CHANGELOG.md`
- `false` → default every package to no changelog
- `"{path}/changelog.md"` or another pattern → replace `{path}` with each package path

A package-level `changelog` value overrides the default for that package.

## Groups

Groups own outward release identity for their member packages.

```toml
[group.sdk]
packages = ["sdk-core", "web-sdk", "mobile-sdk"]
changelog = "changelog.md"
versioned_files = ["group.toml"]
tag = true
release = true
version_format = "primary"
```

Rules:

- group members must already be declared under `[package.<id>]`
- package and group ids share one namespace
- a package may belong to only one group
- only one package or group may use `version_format = "primary"`
- group `tag`, `release`, and `version_format` override member package release identity
- package changelogs and package `versioned_files` still apply when grouped

## Versioned files

`versioned_files` are additional managed files beyond native manifests.

Examples:

```toml
versioned_files = ["Cargo.lock"]
versioned_files = [{ path = "group.toml", dependency = "sdk-core" }]
```

Dependency targets in `versioned_files` must reference declared package ids.

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
command = "cargo test --workspace --all-features"
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

## Package overrides migration note

<!-- {=configurationPackageOverridesSnippet} -->

Legacy repositories may still contain `[[package_overrides]]` entries such as:

```toml
[[package_overrides]]
package = "crates/sdk_core"
changelog = "crates/sdk_core/changelog.md"
```

Under the new model, move that changelog configuration onto the matching `[package.<id>]` declaration instead. When `[defaults].package_type` is set, package entries may also omit an explicit `type`.

<!-- {/configurationPackageOverridesSnippet} -->

## Package references

<!-- {=configurationPackageReferenceRules} -->

Package references in changesets and CLI commands should use configured package ids or group ids. Legacy manifest-relative paths and directory paths may still appear in older repos during migration, but `mc check` should guide you toward declared ids.

<!-- {/configurationPackageReferenceRules} -->

## Current status

<!-- {=configurationCurrentStatus} -->

Current implementation notes:

- `defaults.include_private` is parsed, but discovery behavior is still centered on the supported fixture-driven workflows in this milestone
- `version_groups.strategy` belongs to the legacy model and should be migrated to `[group.<id>]`
- `[ecosystems.*].enabled/roots/exclude` are parsed and documented as the ecosystem control surface
- `package_overrides.changelog` is a legacy setting that should be migrated to package declarations
- supported workflow steps today are `PrepareRelease` and `Command`

<!-- {/configurationCurrentStatus} -->

## Validation

Run:

```bash
mc check --root .
```

`mc check` validates:

- package and group declarations
- manifest presence for each package type
- group membership rules
- `versioned_files` references
- `.changeset/*.md` targets and overlap rules
