# Design Note: Package/Group Configuration API

## Status

Draft proposal prepared in isolated worktree for discussion.

- Worktree: `/Users/ifiokjr/Developer/projects/monochange-group-config-api`
- Branch: `spike/group-config-api`

## Goal

Replace the current `[[version_groups]]` and `[[package_overrides]]` model with a richer configuration API based on explicit package declarations and named groups.

The new API should:

- declare packages explicitly under `[package.<id>]`
- declare groups explicitly under `[group.<id>]`
- allow changesets to target either package ids or group ids
- support shared group changelogs and per-package changelogs
- support extra `versioned_files` at both package and group level
- support release/tag policy at both package and group level
- introduce `mc check` as the validation entrypoint for config and changesets
- use `miette` diagnostics for actionable, source-aware errors

## Finalized Decisions

### 1. Package declarations are explicit

Packages are declared under `[package.<id>]`.

The `<id>` is a logical monochange identifier. It is **not required** to match the native manifest package name.

This allows disambiguation across ecosystems, for example:

- `npm:@scope/name`
- `deno:@scope/name`

### 2. Package fields

Every package declaration must include:

- `path` — relative path to the package directory
- `type` — ecosystem-level package type

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

### 3. Groups replace version groups completely

`[group.<id>]` fully replaces `[[version_groups]]`.

A group declares:

- `packages`
- optional shared `changelog`
- optional `versioned_files`
- optional `tag`
- optional `release`
- optional `version_format`

### 4. Group membership rules

- groups may only reference package ids already declared under `[package.<id>]`
- unknown package references are errors
- a package may belong to only one group
- package ids and group ids share one namespace and must not collide

### 5. Group precedence

If a package belongs to a group, the group owns the outward release identity.

For grouped packages, the group controls:

- `tag`
- `release`
- `version_format`

The package still contributes:

- `path`
- `type`
- `changelog`
- `versioned_files`

### 6. Changelog behavior

If a group defines `changelog`, a shared group changelog is written.

If a grouped package also defines its own `changelog`, that package changelog is also written.

### 7. Versioned file behavior

Both packages and groups may define `versioned_files`.

When a group version changes:

- native manifests for all member packages are updated
- package-level `versioned_files` are applied for each member package
- group-level `versioned_files` are applied

`versioned_files` are **extra managed files** in addition to native manifest handling.

### 8. Release/tag defaults

Defaults for both packages and groups:

- `tag = false`
- `release = false`
- `version_format = "namespaced"`

### 9. Version format

The field name is `version_format`.

Allowed values:

- `namespaced`
- `primary`

Meaning:

- `namespaced` → `{name}/v{version}`
- `primary` → `v{version}`

Constraint:

- only one package or group in the full configuration may use `version_format = "primary"`

### 10. Changeset targets

Changesets may target:

- declared package ids
- declared group ids

They may **not** target raw manifest package names unless those names are also the configured ids.

Invalid changeset cases:

- unknown package/group id
- ambiguous id resolution
- referencing both a group and one of its member packages in the same changeset

### 11. Validation command

`mc check` should validate both configuration and changesets.

Primary form:

```bash
mc check --root .
```

## Proposed Configuration Schema

### Example

```toml
[defaults]
parent_bump = "patch"
package_type = "cargo"

[package.monochange]
path = "crates/monochange"
changelog = "crates/monochange/changelog.md"
versioned_files = ["Cargo.toml"]

[package.monochange_core]
path = "crates/monochange_core"
changelog = "crates/monochange_core/changelog.md"

[package."npm:@scope/name"]
path = "packages/web-sdk"
type = "npm"
changelog = "packages/web-sdk/changelog.md"

[group.main]
packages = ["monochange", "monochange_core", "npm:@scope/name"]
changelog = "changelog.md"
versioned_files = [
	"Cargo.toml",
	{ path = "Cargo.lock", dependency = "monochange_core" },
]
tag = true
release = true
version_format = "primary"
```

## Proposed `versioned_files` Forms

### Shorthand string form

```toml
versioned_files = ["Cargo.toml"]
```

### Structured form

```toml
versioned_files = [
	{ path = "Cargo.lock", dependency = "monochange_core" },
]
```

## Recommended Initial Structured Fields

- `path` — repo-relative file path
- `dependency` — package id whose version references should be updated in the file

Future expansion may add other targeted update modes, but the initial proposal keeps this small.

## Effective Release Identity Rules

### Ungrouped package

An ungrouped package uses its own:

- `tag`
- `release`
- `version_format`

### Grouped package

A grouped package inherits outward release identity from its group.

That means:

- package `tag/release/version_format` do not control tagging/release behavior
- group `tag/release/version_format` are used instead
- package `changelog` and `versioned_files` still apply

## `mc check` Contract

## Responsibilities

`mc check` should validate:

### Configuration

- package ids are unique
- group ids are unique
- package ids and group ids share one global namespace
- every package declares `path`
- every package declares `type`, unless `[defaults].package_type` is set
- `type` is supported
- package path exists
- expected manifest for package type exists
- package paths are unique
- every group member is already declared as a package
- no package belongs to more than one group
- at most one item uses `version_format = "primary"`
- `versioned_files` entries are structurally valid

### Changesets

- `.changeset/*.md` files parse successfully
- frontmatter keys resolve only to declared package/group ids
- bump values are one of `patch`, `minor`, `major`
- a changeset may not reference both a group and one of its member packages

## Diagnostics

The command should use `miette` so errors can include:

- exact source location in `monochange.toml` or a changeset file
- a concise explanation of what is wrong
- a specific help message suggesting how to fix it

### Example diagnostic quality bar

Unknown package in group:

```text
× unknown package `monochange_semve`
╭─[monochange.toml:18:15]
│
18 │ packages = ["monochange", "monochange_semve"]
│                              ────────┬────────────
│                                      ╰── this package was not declared
│
help: declare it first under `[package.monochange_semver]`
help: or fix the typo in `group.main.packages`
```

Group/member overlap in changeset:

```text
× changeset references both group `workspace` and member package `monochange_core`
╭─[.changeset/feature.md:2:1]
│
2 │ workspace: minor
│   ──────── group reference
3 │ monochange_core: patch
│   ──────────────── member package reference
│
help: reference either the group or the package, but not both in the same changeset
```

## Migration Plan

### Current model

Current configuration uses:

- `[[version_groups]]`
- `[[package_overrides]]`

### New model

Replace with:

- `[package.<id>]`
- `[group.<id>]`

### Migration example

#### Before

```toml
[[version_groups]]
name = "main"
members = [
	"crates/monochange",
	"crates/monochange_core",
]

[[package_overrides]]
package = "crates/monochange"
changelog = "crates/monochange/changelog.md"
```

#### After

```toml
[defaults]
package_type = "cargo"

[package.monochange]
path = "crates/monochange"
changelog = "crates/monochange/changelog.md"

[package.monochange_core]
path = "crates/monochange_core"
changelog = "crates/monochange_core/changelog.md"

[group.main]
packages = ["monochange", "monochange_core"]
```

## Data Model Implications

The current model has separate concepts for:

- `VersionGroupDefinition`
- `PackageOverride`

The proposed replacement should introduce explicit models for:

- package declarations
- group declarations
- shared release identity settings
- extra versioned file entries

A likely Rust model split:

- `PackageType`
- `VersionFormat`
- `VersionedFileDefinition`
- `PackageDefinition`
- `GroupDefinition`
- updated `WorkspaceConfiguration`

## Open Implementation Work

This design note does not implement the feature yet. Implementation work would include at least:

1. replacing current config structs and parsers
2. adding `miette`-based diagnostics
3. adding `mc check`
4. updating changeset resolution logic to use declared ids only
5. updating release preparation to respect group precedence
6. updating docs, examples, and tests

## Non-Goals for the First Implementation Slice

- supporting undeclared packages in groups
- supporting raw manifest package names in changesets outside declared ids
- supporting packages in multiple groups
- broad plugin-style file update rules beyond the initial `versioned_files` proposal
