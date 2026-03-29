# Configuration Contract: `monochange.toml`

## Purpose

Define the configuration surface for cross-ecosystem workspace discovery, changeset validation, release planning, and workflow-driven release preparation.

## File Location

- Repository root: `monochange.toml`

## Top-Level Sections

### `[defaults]`

Repository-wide default behavior.

| Field                    | Type    | Required | Meaning                                                                            |
| ------------------------ | ------- | -------- | ---------------------------------------------------------------------------------- |
| `parent_bump`            | string  | No       | Default bump applied to affected parent packages when no stronger evidence exists. |
| `include_private`        | boolean | No       | Whether private packages are included in discovery and planning output.            |
| `warn_on_group_mismatch` | boolean | No       | Whether existing grouped-version mismatches emit warnings.                         |

### `[package.<id>]`

Declares a release-managed package using a monochange-owned id.

| Field             | Type    | Required | Meaning                                                         |
| ----------------- | ------- | -------- | --------------------------------------------------------------- |
| `path`            | string  | Yes      | Package directory relative to the repository root.              |
| `type`            | string  | Yes      | One of `cargo`, `npm`, `deno`, `dart`, or `flutter`.            |
| `changelog`       | string  | No       | Changelog file updated during release preparation.              |
| `versioned_files` | array   | No       | Additional files whose version references should be updated.    |
| `tag`             | boolean | No       | Whether this package should produce a tag when not grouped.     |
| `release`         | boolean | No       | Whether this package should produce a release when not grouped. |
| `version_format`  | string  | No       | `namespaced` or `primary`; defaults to `namespaced`.            |

### `[group.<id>]`

Declares a shared release unit that owns outward release identity for its member packages.

| Field             | Type             | Required | Meaning                                                     |
| ----------------- | ---------------- | -------- | ----------------------------------------------------------- |
| `packages`        | array of strings | Yes      | Declared package ids that belong to the group.              |
| `changelog`       | string           | No       | Group changelog updated during release preparation.         |
| `versioned_files` | array            | No       | Additional shared files updated during release preparation. |
| `tag`             | boolean          | No       | Whether the group should produce a tag.                     |
| `release`         | boolean          | No       | Whether the group should produce a release.                 |
| `version_format`  | string           | No       | `namespaced` or `primary`; defaults to `namespaced`.        |

### `[[workflows]]`

Defines named workflows that can be run as top-level commands such as `mc release`.

### `[[workflows.steps]]`

Built-in typed workflow steps.

| Field     | Type   | Required | Meaning                                                     |
| --------- | ------ | -------- | ----------------------------------------------------------- |
| `type`    | string | Yes      | One of `PrepareRelease` or `Command`.                       |
| `command` | string | No       | Required when `type = "Command"`; shell command to execute. |

### `[ecosystems.cargo]`, `[ecosystems.npm]`, `[ecosystems.deno]`, `[ecosystems.dart]`

Per-ecosystem switches and discovery overrides.

## Example

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true

[package.sdk-core]
path = "crates/sdk_core"
type = "cargo"
changelog = "crates/sdk_core/CHANGELOG.md"

[package.web-sdk]
path = "packages/web-sdk"
type = "npm"

[group.sdk]
packages = ["sdk-core", "web-sdk"]
changelog = "CHANGELOG.md"
tag = true
release = true
version_format = "primary"

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"

[ecosystems.cargo]
enabled = true

[ecosystems.npm]
enabled = true
roots = ["packages/*"]
```

## Validation Rules

- package and group ids share one namespace
- groups may only reference declared package ids
- a package may belong to at most one group
- only one package or group may use `version_format = "primary"`
- `versioned_files` dependency entries must reference declared package ids
- changesets may reference only declared package ids or group ids
- a changeset may not reference both a group and one of its members in the same file
