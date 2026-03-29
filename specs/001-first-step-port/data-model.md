# Data Model: Cross-Ecosystem Release Planning Foundation

## 1. WorkspaceConfiguration

**Purpose**: Captures repository-wide defaults and explicit package/group declarations loaded from `monochange.toml`.

### Fields

- `root_path`: Canonical repository root.
- `defaults`: Shared planning defaults.
- `packages`: Declared package definitions keyed by monochange-owned ids.
- `groups`: Declared shared release units keyed by monochange-owned ids.
- `workflows`: Configured top-level workflows.
- per-ecosystem settings for cargo, npm, deno, and dart adapters.

### Validation Rules

- package and group ids share one namespace.
- package paths must be unique.
- groups may reference only declared package ids.
- a package may belong to at most one group.
- only one package or group may use `version_format = "primary"`.

## 2. PackageDefinition

**Purpose**: Declares one release-managed package.

### Fields

- `id`
- `path`
- `package_type`
- `changelog`
- `versioned_files`
- `tag`
- `release`
- `version_format`

## 3. GroupDefinition

**Purpose**: Declares a shared release identity for a set of packages.

### Fields

- `id`
- `packages`
- `changelog`
- `versioned_files`
- `tag`
- `release`
- `version_format`

## 4. EffectiveReleaseIdentity

**Purpose**: Resolves the outward release owner for a package.

### Behavior

- grouped packages inherit the group’s `tag`, `release`, and `version_format`
- ungrouped packages use their own release metadata
- grouped packages still retain package-level changelog and package-level `versioned_files`

## 5. PackageRecord

**Purpose**: Normalized representation of a discovered package regardless of ecosystem.

### Important metadata

- native package name
- stable discovered package id
- optional `config_id` linking the discovered package back to `[package.<id>]`
- optional `version_group_id` derived from `[group.<id>]`

## 6. ChangeSignal

**Purpose**: Captures input that a package changed and why.

### Behavior

- markdown changesets may target configured package ids or group ids
- group-targeted changesets expand into package-level signals before planning
- overlapping group/member targets are invalid

## 7. ReleasePlan

**Purpose**: Top-level output for one planning run.

### Includes

- package decisions
- grouped outcomes
- compatibility evidence
- warnings and unresolved items

## 8. PreparedRelease

**Purpose**: Captures the results of workflow-driven release preparation.

### Includes

- discovered changeset paths
- released package list
- release targets with effective tag/release metadata
- changed manifest, changelog, and versioned-file outputs
- deleted changesets after successful non-dry-run execution
