# Design Note: Group Changelog Inclusion Policy

## Status

Planning draft for implementation in `spike/group-changelog-targeting`.

- Worktree: `/Users/ifiokjr/Developer/projects/monochange-worktrees/group-changelog-targeting`
- Branch: `spike/group-changelog-targeting`

## Goal

Reduce noise in group changelogs without changing release-planning semantics.

Today, grouped releases can surface every member-targeted note in the group changelog. That is correct for small groups, but it becomes noisy when a group contains internal packages whose detailed changes are not meaningful to consumers of the group's outward release identity.

The new design adds a group-level inclusion policy so repositories can decide which member-targeted changesets are allowed to appear in the group changelog while keeping package changelogs and version planning unchanged.

## Problem Statement

A group in MonoChange owns a shared release identity:

- one synchronized version
- one outward tag / release identity
- optionally one group changelog

However, the group changelog currently behaves like an aggregation surface for member package notes. That creates a mismatch in some repositories:

- the group release is consumer-facing
- some member packages are implementation details
- package changelogs remain useful and detailed
- the group changelog becomes too noisy if every internal note is copied upward

We need a way to say:

- which member packages are allowed to contribute notes to the group changelog
- that direct group-targeted notes are always group-relevant
- that filtered-out member notes still participate in version planning and package changelogs

## Non-Goals

This design does **not** change:

- release planning or bump propagation
- synchronized group version behavior
- package changelog generation
- the rule that a changeset may not target both a group and one of its members in the same file
- interactive CLI target selection

## Proposed Config API

Add an `include` field to grouped changelog tables:

```toml
[group.main.changelog]
path = "changelog.md"
include = "group-only" # or "all" or ["cli", "api"]
```

### Allowed values

- `"all"`
- `"group-only"`
- `[]` or a non-empty array of package ids belonging to that group

### Semantics

#### Omitted `include`

If `include` is omitted, MonoChange behaves as though `include = "all"`.

That preserves current behavior for existing repositories.

#### `include = "all"`

Include in the group changelog:

- all direct group-targeted changesets
- all member-targeted changesets for members of the group

#### `include = "group-only"`

Include in the group changelog:

- all direct group-targeted changesets
- no member-targeted changesets

#### `include = ["package-a", "package-b"]`

Include in the group changelog:

- all direct group-targeted changesets
- member-targeted changesets only when every target within that group is in the allowlist

#### `include = []`

Treat `[]` exactly like `"group-only"`.

This allows a compact allowlist form while still documenting `"group-only"` as the preferred explicit spelling.

## Inclusion Rules

For a group `G`:

1. A direct group-targeted changeset is always eligible for `G`'s changelog.
2. A member-targeted changeset is evaluated only for changelog visibility, not for release planning.
3. If `include = "all"`, every member-targeted changeset in `G` is eligible.
4. If `include = "group-only"`, no member-targeted changeset in `G` is eligible.
5. If `include = [ids...]`, a member-targeted changeset is eligible only if every target belonging to `G` is in the allowlist.
6. Targets outside `G` do not affect `G`'s inclusion decision.

### Why the allowlist requires all in-group targets

A single changeset body can describe multiple package targets. If one allowed member and one disallowed member share the same note body, MonoChange should not partially reinterpret the note for the group changelog. Requiring every in-group target to be allowed avoids ambiguous rendering and accidental leakage of internal details.

## Examples

### Example 1 — include all member notes

```toml
[group.sdk.changelog]
path = "docs/sdk-CHANGELOG.md"
include = "all"
```

- direct group notes appear
- notes targeting `core` appear
- notes targeting `app` appear

### Example 2 — group-facing notes only

```toml
[group.main.changelog]
path = "changelog.md"
include = "group-only"
```

- direct group notes appear
- member notes do not appear
- package changelogs still include their own notes
- version planning is unchanged

### Example 3 — only public-surface packages contribute upward

```toml
[group.main.changelog]
path = "changelog.md"
include = ["cli"]
```

- direct group notes appear
- notes targeting `cli` appear
- notes targeting `internal-lib` do not appear
- notes targeting both `cli` and `internal-lib` do not appear

### Example 4 — outside-group targets do not disqualify the note

If a changeset targets:

```markdown
---
cli: minor
other-tool: patch
---
```

and `cli` belongs to `main` while `other-tool` is outside `main`, the note remains eligible for `main` if `cli` is allowed by `group.main.changelog.include`.

## Rendering Behavior

The inclusion policy only changes what appears in the **group changelog**.

It does not change:

- package changelog entries
- release target calculation
- group version calculation
- manifest / versioned-file updates

### When no notes remain eligible

If a grouped release occurs but no notes are eligible for the group changelog after filtering:

- the group changelog is still written when the group version changes
- the summary should still show:
  - `Changed members`
  - `Synchronized members`
- MonoChange should render built-in fallback text explaining that no group-facing notes were recorded because member package changes are not configured for inclusion in the group changelog

Recommended fallback shape:

```md
Grouped release for `main`.

Changed members: internal-lib Synchronized members: cli

No group-facing notes were recorded for this release. Member packages were updated as part of the synchronized group version, but their changes are not configured for inclusion in this changelog.
```

This fallback is separate from existing `empty_update_message` behavior.

## Validation Rules

### Valid

- `include = "all"`
- `include = "group-only"`
- `include = []`
- `include = ["cli"]`
- `include = ["cli", "api"]`

### Invalid

- `include = "members"`
- `include = ["missing-package"]`
- `include = ["package-outside-this-group"]`
- non-string, non-array values
- array entries that are empty strings

### Normalization

- duplicate package ids in the array are deduplicated internally
- array order is not semantically meaningful

## Diagnostics

This feature should not make changesets invalid. It only changes whether a pending note is **eligible for group changelog output**.

A later diagnostics improvement may surface that eligibility explicitly, but the first implementation should focus on rendering and validation of the configuration itself.

Recommended future terminology:

- “eligible for group changelog”

## Implementation Sketch

The implementation should happen at **group changelog note construction time**, not in the release-planning model.

### Core/domain

Add a typed group changelog inclusion policy to the shared configuration model.

Suggested shape:

- `All`
- `GroupOnly`
- `Selected(BTreeSet<String>)`

### Config parsing

Parse `group.<id>.changelog.include` from:

- string enums `"all"` and `"group-only"`
- arrays of configured package ids

Validation should ensure every array member:

- references a declared package id
- belongs to the current group

### Release-note construction

When building a group changelog:

1. collect direct group-targeted notes
2. collect member-targeted notes for group members
3. filter member-targeted notes through the inclusion policy
4. keep package changelog generation unchanged
5. preserve the changed/synchronized member summary even when filtered notes produce no entries
6. render the new built-in fallback when the filtered note set is empty

## Code Areas Likely to Change

- `crates/monochange_core/src/lib.rs`
  - group changelog inclusion policy types
  - shared config model
- `crates/monochange_config/src/lib.rs`
  - parsing and validation for `group.<id>.changelog.include`
- `crates/monochange/src/lib.rs`
  - grouped changelog note filtering
  - grouped fallback rendering
- tests and fixtures:
  - `crates/monochange_config/src/__tests.rs`
  - `crates/monochange/tests/changelog_formats.rs`
  - `fixtures/tests/...`
  - docs and `monochange.toml` comments

## Acceptance Criteria

1. Repositories can set `group.<id>.changelog.include` to `"all"`, `"group-only"`, or an allowlist array.
2. Omitting `include` preserves current behavior.
3. Direct group-targeted changesets are always included in the group changelog.
4. Member-targeted changesets are filtered only for group changelog rendering.
5. Package changelogs and release planning remain unchanged.
6. Invalid allowlist entries fail configuration validation with actionable errors.
7. When filtering removes all eligible group notes, the group changelog still renders an honest fallback summary.
8. Structured release-note consumers inherit the same filtered group note set.

## Recommended Delivery Plan

### Phase 1

- add config parsing and validation
- add grouped changelog filtering
- add fallback summary rendering
- update docs and config annotations
- add fixture-driven tests

### Phase 2

- expose eligibility in diagnostics output
- evaluate whether additional summary/detail controls are needed for grouped changelogs
- revisit mixed group/member changesets only if real authoring pain remains after Phase 1
