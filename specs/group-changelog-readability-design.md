# Design Note: Group Changelog Readability and Change Provenance

## Status

Planning draft prepared in isolated worktree for discussion.

- Worktree: `/Users/ifiokjr/Developer/projects/monochange-worktrees/group-changelog-format-review`
- Branch: `spike/group-changelog-format-review`

## Goal

Improve grouped changelog output so readers can immediately tell:

1. which group member a note came from
2. which members changed directly versus only synchronized to the group version
3. what Markdown structure monochange should render by default for grouped releases

The design should improve readability without making package-owned changelogs noisier.

## Current Behavior

When a group member is the only package referenced in a changeset, monochange:

- expands that change into the grouped release plan
- writes the member package changelog
- writes the group changelog
- writes fallback entries for synchronized members that did not have direct notes

### Reproduced example

Input:

- group: `sdk`
- members: `core`, `app`
- changeset targets only `core`

Current group changelog output:

```md
## 1.1.0

Grouped release for `sdk`.

Members: core, app

### Features

- add keep a changelog support
```

Current changed package changelog output:

```md
## [1.1.0]

### Features

- add keep a changelog support
```

Current synchronized member changelog output:

```md
## [1.1.0]

### Features

- No package-specific changes were recorded; `workflow-app` was updated to 1.1.0 as part of group `sdk`.
```

## Problem Statement

The current grouped changelog is correct but not very informative.

### What is clear today

- the release belongs to a group
- which packages belong to the group
- the release-note section type, for example `Features`

### What is not clear today

- which member package a given note belongs to
- whether a package changed directly or only because the group synchronized its version
- how to scan multiple entries quickly when several members contributed notes

### Why the output looks this way

The current group changelog aggregates member notes into one list, but the default change templates do not include `$package`:

```toml
[release_notes]
change_templates = ["#### $summary\n\n$details", "- $summary"]
```

That means grouped changelog entries render the summary text only, even though the release-note model already knows the originating package.

## Design Principles

### 1. Provenance should be explicit in grouped changelogs

A reader should not have to infer ownership from surrounding context.

### 2. Package changelogs should stay concise

A package changelog already implies package ownership by file location. Repeating package labels there is usually redundant.

### 3. The default should be readable without extra config

Users should not need to discover template variables just to get understandable grouped output.

### 4. The Markdown should remain plain and portable

Rendered output should still look good in GitHub, GitLab, generated release notes, and raw file views.

## Options Considered

## Option A — Keep current behavior

### Example

```md
### Features

- add keep a changelog support
```

### Pros

- no implementation work
- smallest output

### Cons

- package provenance stays implicit
- grouped releases with multiple member notes are hard to scan
- does not answer the user question "which member changed?"

### Recommendation

Do not choose this.

## Option B — Use workspace-wide templates to include `$package`

Example configuration:

```toml
[release_notes]
change_templates = [
	"#### $package — $summary\n\n$details",
	"- **$package**: $summary",
]
```

### Example grouped output

```md
### Features

- **core**: add keep a changelog support
```

### Pros

- already possible today
- no code change required
- immediately solves provenance in the group changelog

### Cons

- also affects package changelogs, where package labels are usually redundant
- makes workspace-wide formatting policy carry group-specific readability concerns
- not a strong default product experience

### Recommendation

Useful as a stopgap, but not the preferred product direction.

## Option C — Automatically label entries only in group changelogs

### Example grouped output

```md
## 1.1.0

Grouped release for `sdk`.

Members: core, app

### Features

- **core**: add keep a changelog support
```

### Example package output

```md
## [1.1.0]

### Features

- add keep a changelog support
```

### Pros

- explicit provenance where it matters most
- package changelogs stay clean
- minimal behavior change
- no extra config required

### Cons

- grouped output becomes slightly more verbose
- requires group-aware rendering behavior

### Recommendation

This is the recommended first implementation step.

## Option D — Label entries and summarize changed versus synchronized members

### Example grouped output

```md
## 1.1.0

Grouped release for `sdk`.

Changed members: core Synchronized members: app

### Features

- **core**: add keep a changelog support
```

### Pros

- best scanability
- directly explains why every member version moved
- especially helpful for larger groups

### Cons

- larger behavior change
- requires deriving and rendering two member sets
- more opinionated output

### Recommendation

Good follow-up enhancement after Option C lands.

## Recommended Direction

Ship the change in two phases.

### Phase 1 — Explicit member labels in grouped changelogs

Change only grouped changelog entry rendering so each entry is prefixed with its source package id or package display name.

Preferred formatting:

```md
- **core**: add keep a changelog support
```

Rationale:

- compact
- easy to scan
- renders well in common Markdown surfaces
- works for both `monochange` and `keep_a_changelog`

### Phase 2 — Add direct versus synchronized member summary lines

Extend the grouped changelog summary block to distinguish:

- members with direct notes in this release
- members included because the group synchronized their version

Preferred formatting:

```md
Grouped release for `sdk`.

Changed members: core Synchronized members: app
```

If every member changed directly, omit the synchronized line. If no direct notes exist, fall back to the current group-level empty update message behavior.

## Proposed Markdown Shapes

## monochange format

### Current

```md
## 1.1.0

Grouped release for `sdk`.

Members: core, app

### Features

- add keep a changelog support
```

### Phase 1

```md
## 1.1.0

Grouped release for `sdk`.

Members: core, app

### Features

- **core**: add keep a changelog support
```

### Phase 2

```md
## 1.1.0

Grouped release for `sdk`.

Changed members: core Synchronized members: app

### Features

- **core**: add keep a changelog support
```

## Keep a Changelog format

### Phase 1

```md
## [1.1.0]

Grouped release for `sdk`.

Members: core, app

### Features

- **core**: add keep a changelog support
```

### Phase 2

```md
## [1.1.0]

Grouped release for `sdk`.

Changed members: core Synchronized members: app

### Features

- **core**: add keep a changelog support
```

## Implementation Sketch

## Rendering model impact

The release-note model already carries enough data for Phase 1:

- `ReleaseNoteChange.package_id`
- `ReleaseNoteChange.package_name`

The missing piece is group-aware rendering.

### Phase 1 implementation approach

1. Detect when a changelog document belongs to a group target.
2. Render grouped entries with a package label prefix.
3. Keep package-target rendering unchanged.
4. Preserve existing section grouping and templates for details blocks.

One safe strategy is:

- keep the current summary and section model
- when rendering an entry for a group changelog, prefix the first line with the member label
- do not alter package changelog entries

### Phase 2 implementation approach

1. Identify group members that contributed direct notes.
2. Identify remaining released members as synchronized-only members.
3. Replace or extend the current `Members: ...` summary line with separate lines.
4. Keep fallback behavior for fully note-less grouped releases.

## Code Areas Likely to Change

- `crates/monochange/src/lib.rs`
  - `group_release_note_changes`
  - `group_release_summary`
  - release-note document construction for grouped targets
- potentially `crates/monochange_core/src/lib.rs`
  - only if a small rendering context enum or helper is useful
- tests:
  - `crates/monochange/tests/changelog_formats.rs`
  - `crates/monochange/src/__tests.rs`
  - possibly `crates/monochange/tests/cli_output.rs` if user-facing text changes

## Testing Strategy

### Phase 1

Add or update tests that verify:

- a group changelog includes direct member notes
- each grouped changelog entry includes the source member label
- package changelogs do not gain the same label by default
- both `monochange` and `keep_a_changelog` formats behave correctly

### Phase 2

Add tests that verify:

- direct members are listed under `Changed members`
- synchronized-only members are listed under `Synchronized members`
- the synchronized line is omitted when unnecessary
- the fallback grouped empty update message still works when no direct notes exist

## Acceptance Criteria

### Phase 1

- grouped changelog entries clearly identify their source member package
- package changelog entry formatting stays unchanged by default
- formatting remains readable in both supported changelog formats

### Phase 2

- grouped changelog summary distinguishes direct-change members from synchronized-only members
- grouped changelog remains compact and readable for small groups
- behavior remains deterministic and snapshot-friendly

## Recommendation Summary

Recommended sequence:

1. adopt Option C as the immediate product improvement
2. follow with Option D if the first change reads well in tests and fixtures

That sequence gives monochange a clear readability win quickly while keeping the initial implementation narrow and low-risk.
