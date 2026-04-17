# Changeset lifecycle guide

This guide explains how to actively manage changesets throughout the development lifecycle. Changesets are not fire-and-forget — they must be reviewed, updated, and removed as code evolves.

## Lifecycle stages

Every changeset goes through these stages:

1. **Created** — When a change is first written
2. **Updated** — When the scope of the described change expands or shifts
3. **Removed** — When the described change no longer applies (feature reverted, approach changed)
4. **Consumed** — When the release runs and the changeset is deleted after publishing

## Before creating a changeset

<!-- {=changesetLifecycleSteps} -->

Before creating any changeset, follow these steps:

1. **Read all existing changesets** in `.changeset/` to understand current coverage
2. **Identify which packages are affected** by the current diff
3. **For each affected package**, determine:
   - Does an existing changeset already describe this change? → **Update it**
   - Does an existing changeset describe a removed feature? → **Remove it**
   - Is this a genuinely new change? → **Create a new changeset**
4. **Classify each change** by artifact type to choose the right bump level and section type
5. **Write the changeset** following the artifact-type template
6. **Validate** by running `mc validate` or `mc diagnostics --format json`

When removing a changeset for a reverted feature, delete the file entirely — do not leave empty or zero-bump changesets as they create noise in release notes.

<!-- {/changesetLifecycleSteps} -->

## Decision matrix

<!-- {=changesetLifecycleDecisionMatrix} -->

| Scenario                          | Action                   | Rationale                                       |
| --------------------------------- | ------------------------ | ----------------------------------------------- |
| New feature added                 | **Create new**           | Granular tracking of distinct changes           |
| Existing feature expanded         | **Update existing**      | Keep related changes together                   |
| Feature removed or reverted       | **Remove changeset**     | Don't release notes for removed features        |
| Same change, different approach   | **Replace changeset**    | Document the actual implementation              |
| Multiple small related changes    | **Create new** (grouped) | Summarize when exceeding threshold              |
| Bug found in unreleased feature   | **Update existing**      | Combine fix with feature, not a separate entry  |
| Refactor of unreleased change     | **Update existing**      | Rewrite description to reflect new structure    |
| Changeset references removed code | **Remove changeset**     | Stale changesets create confusing release notes |

<!-- {/changesetLifecycleDecisionMatrix} -->

## How to update an existing changeset

When expanding an existing feature, edit the changeset file in place:

**Before** — `.changeset/add-config-validation.md`:

```markdown
---
core: minor
---

#### add config validation

Introduce validation for monochange.toml files.
```

**After** — same file, updated:

```markdown
---
core: minor
---

#### add config validation with schema checking

Introduce validation for monochange.toml files including:

- Basic syntax validation
- Schema conformance checking
- Helpful error messages for common mistakes
```

The bump level may need to change too — a `minor` might become `major` if the scope expanded to include breaking changes.

## How to replace a changeset

When the implementation approach changed, delete the old changeset and create a new one:

```bash
rm .changeset/add-config-validation.md
mc change --package core --bump minor --reason "add config validation with relaxed parsing" --output .changeset/add-config-validation.md
```

Use `--output` to write to a deterministic path so the new changeset replaces the old one.

## How to remove a stale changeset

When a feature was reverted or removed from the branch:

```bash
rm .changeset/add-experimental-feature.md
```

Do not leave orphaned changesets. A changeset that describes code that no longer exists creates confusing release notes.

## Using MCP tools for lifecycle management

<!-- {=changesetAnalysisMcpTools} -->

The `monochange_analysis` crate provides MCP tools for automated changeset generation:

**`monochange_analyze_changes`** — Analyzes git diffs and suggests granular changeset structure:

```json
{
	"path": "/path/to/repo",
	"frame": "main...feature-branch",
	"detection_level": "signature",
	"max_suggestions": 10
}
```

Returns an analysis with:

- Per-package change classifications (library, application, CLI, mixed)
- Semantic change details (added/removed functions, routes, commands)
- Suggested bump levels and grouping
- Breaking change detection

**`monochange_validate_changeset`** — Validates that a changeset matches the actual code changes:

```json
{
	"path": "/path/to/repo",
	"changeset_path": ".changeset/feature.md"
}
```

Returns validation issues with severity levels and suggestions for fixing mismatches between changeset claims and actual code changes.

**Lifecycle integration:**

These tools integrate with the changeset lifecycle:

- Use `monochange_analyze_changes` to understand what changed before deciding whether to create, update, or remove a changeset
- Use `monochange_validate_changeset` to verify that existing changesets still accurately describe the current diff
- Both tools respect the artifact type classification to provide appropriate suggestions

<!-- {/changesetAnalysisMcpTools} -->

## Dependency propagation with `caused_by`

<!-- {=changesetCausedByField} -->

### Dependency propagation with `caused_by`

When a dependency changes, monochange automatically patches all dependents. This creates release notes with no context for _why_ the dependent is being updated.

The `caused_by` field in changeset frontmatter provides that context. It lists the root package(s) or group(s) that triggered this dependent change:

```markdown
---
monochange_config:
  bump: patch
  caused_by: ["monochange_core"]
---

#### update dependency on monochange_core

Bumps `monochange_core` dependency to v2.1.0 after the public API change to `ChangelogFormat`.
```

**How it works:**

1. Without `caused_by`: a dependent gets an automatic "dependency changed → patch" record with no explanation
2. With `caused_by`: the authored changeset **replaces** the automatic propagation — it provides human-readable context instead
3. A changeset with `caused_by` and `bump: patch` suppresses the automatic "dependency changed → patch" record for that package
4. A changeset with `caused_by` and `bump: none` suppresses propagation entirely — the package is acknowledged as affected but no version bump is warranted

**`none` bump with `caused_by` — the "nothing meaningful changed" case:**

When `mc affected` flags a package but the change is not meaningful (just a lockfile update or a re-export), use `bump: none` with `caused_by`:

```markdown
---
monochange_config:
  bump: none
  caused_by: ["monochange_core"]
  type: deps
---

#### update monochange_core dependency

No user-facing changes. Dependency version updated to match the group release.
```

This tells monochange: "this package is affected, but the change doesn't warrant a version bump for consumers. Suppress the automatic patch propagation entirely."

CLI flag: `mc change --package <id> --bump patch --caused-by monochange_core --reason "update dependency"`

<!-- {/changesetCausedByField} -->

## Validating changesets

After creating, updating, or removing changesets, always validate:

```bash
mc validate
```

This checks that:

- Every referenced package or group id exists in `monochange.toml`
- Bump levels are valid (`none`, `patch`, `minor`, `major`, or configured change types)
- Type values match configured `extra_changelog_sections`
- No orphaned references remain

For structured diagnostics with git provenance and linked metadata:

```bash
mc diagnostics --format json
```
