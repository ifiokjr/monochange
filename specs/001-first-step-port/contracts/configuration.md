# Configuration Contract: `monochange.toml`

## Purpose

Define the first-step configuration surface for cross-ecosystem workspace discovery and release planning.

## File Location

- Repository root: `monochange.toml`

## Top-Level Sections

### `[defaults]`

Repository-wide default behavior.

| Field                    | Type    | Required | Meaning                                                                            |
| ------------------------ | ------- | -------- | ---------------------------------------------------------------------------------- |
| `parent_bump`            | string  | No       | Default bump applied to affected parent packages when no stronger evidence exists. |
| `include_private`        | boolean | No       | Whether private packages are included in discovery and planning output.            |
| `warn_on_group_mismatch` | boolean | No       | Whether existing version-group mismatches emit warnings.                           |

### `[[version_groups]]`

Defines packages that always share the same planned version.

| Field      | Type             | Required | Meaning                                                                  |
| ---------- | ---------------- | -------- | ------------------------------------------------------------------------ |
| `name`     | string           | Yes      | Stable group identifier.                                                 |
| `members`  | array of strings | Yes      | Package identifiers or manifest-relative paths that belong to the group. |
| `strategy` | string           | No       | Group versioning strategy; defaults to shared version behavior.          |

### `[ecosystems.cargo]`

### `[ecosystems.npm]`

### `[ecosystems.deno]`

### `[ecosystems.dart]`

Per-ecosystem switches and discovery overrides.

| Field     | Type             | Required | Meaning                                        |
| --------- | ---------------- | -------- | ---------------------------------------------- |
| `enabled` | boolean          | No       | Enables or disables the ecosystem adapter.     |
| `roots`   | array of strings | No       | Additional scan roots for that ecosystem.      |
| `exclude` | array of strings | No       | Paths or globs to exclude from that ecosystem. |

## Example

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true

[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk", "packages/mobile-sdk"]

[ecosystems.cargo]
enabled = true

[ecosystems.npm]
enabled = true
roots = ["packages/*"]

[ecosystems.deno]
enabled = true

[ecosystems.dart]
enabled = true
```

## Validation Rules

- Unknown top-level sections must produce actionable warnings or errors.
- Group names must be unique.
- Group members must resolve to discovered packages.
- Ecosystem-specific roots may use supported glob syntax.
- Excluded paths must remove matching packages from final discovery output.
- If `parent_bump` is omitted, the default is `patch`.

## Extensibility Requirements

- The format must remain composable so later milestones can add publishing and automation settings without breaking existing planning configuration.
- Ecosystem-specific settings must remain namespaced so adapters can evolve independently.
