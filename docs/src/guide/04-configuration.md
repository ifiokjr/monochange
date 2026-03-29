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

## Packages

Declare every release-managed package explicitly.

<!-- {=configurationVersionGroupsSnippet} -->

```toml
[package.sdk-core]
path = "crates/sdk_core"
type = "cargo"
changelog = "crates/sdk_core/CHANGELOG.md"
versioned_files = ["crates/sdk_core/extra.toml"]
tag = false
release = false
version_format = "namespaced"
```

<!-- {/configurationVersionGroupsSnippet} -->

Required fields:

- `path`
- `type`

Supported `type` values:

- `cargo`
- `npm`
- `deno`
- `dart`
- `flutter`

Optional package fields:

- `changelog`
- `versioned_files`
- `tag`
- `release`
- `version_format`

## Groups

Groups own outward release identity for their member packages.

```toml
[group.sdk]
packages = ["sdk-core", "web-sdk", "mobile-sdk"]
changelog = "CHANGELOG.md"
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
