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
| New published package or crate    | **Create new**           | First release note should use a `major` bump    |
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

The `monochange_analysis` crate provides MCP tools for semantic diff inspection and changeset validation.

`monochange_analyze_changes` and `monochange_validate_changeset` now return real semantic analysis for Cargo, npm, Deno, and Dart/Flutter packages. They surface ecosystem-specific evidence such as Rust public API diffs, JS/TS export changes, `package.json` and `deno.json` export metadata, and `pubspec.yaml` dependency or plugin-platform changes.

**`monochange_analyze_changes`** — Analyzes git diffs and returns granular semantic changes for affected packages:

```json
{
	"path": "/path/to/repo",
	"frame": "main...feature-branch",
	"detection_level": "signature",
	"max_suggestions": 10
}
```

Current output includes:

- Cargo public Rust API diffs plus `Cargo.toml` dependency and manifest metadata changes
- npm-family JS/TS exported symbol diffs plus `package.json` exports, commands, dependency, and script changes
- Deno JS/TS exported symbol diffs plus `deno.json` exports, import aliases, task, and compiler-option changes
- Dart and Flutter public `lib/` API diffs plus `pubspec.yaml` executables, dependency, environment, and plugin-platform changes

This tool intentionally **does not decide** whether the diff is patch/minor/major. It returns semantic evidence for the agent to interpret.

**`monochange_validate_changeset`** — Validates that a changeset matches the current semantic diff:

```json
{
	"path": "/path/to/repo",
	"changeset_path": ".changeset/feature.md"
}
```

The validator uses the same Cargo, npm, Deno, and Dart/Flutter analyzer registry as `monochange_analyze_changes`, so stale symbol checks and missing-item suggestions stay aligned with the semantic evidence surfaced for each ecosystem.

**Lifecycle integration:**

These tools integrate with the changeset lifecycle:

- Use `monochange_analyze_changes` to inspect semantic evidence before creating or updating a changeset
- Use `monochange_validate_changeset` to catch stale symbol references or underspecified summaries
- Treat the returned semantic model as evidence for the agent, not an automatic bump decision

<!-- {/changesetAnalysisMcpTools} -->

## Dependency propagation with `caused_by`

<!-- {=changesetCausedByField} -->

### Dependency propagation with `caused_by`

When a dependency changes, monochange automatically patches all dependents. This creates release notes with no context for _why_ the dependent is being updated.

The `caused_by` field in changeset frontmatter provides that context. It lists the root package(s) or group(s) that triggered this dependent change. Because `caused_by` is part of the object form, use table syntax instead of scalar shorthand whenever you need it:

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
2. With `caused_by`: the authored changeset **replaces** the matching automatic propagation — it provides human-readable context instead
3. A changeset with `caused_by` and `bump: patch` suppresses the automatic "dependency changed → patch" record for that package when the same upstream package or group triggered it
4. A changeset with `caused_by` and `bump: none` suppresses that matching propagation entirely — the package is acknowledged as affected but no version bump is warranted
5. Unrelated upstream sources can still propagate normally; `caused_by` is not a global opt-out for every dependency edge

**`none` bump with `caused_by` — the "nothing meaningful changed" case:**

When `mc step:affected-packages` (or a config-defined wrapper such as `mc affected`) flags a package but the change is not meaningful (just a lockfile update or a re-export), use `bump: none` with `caused_by`:

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

This tells monochange: "this package is affected, but the change doesn't warrant a version bump for consumers. Suppress the matching automatic patch propagation entirely."

CLI authoring supports one or more `--caused-by` flags:

- `mc change --package <id> --bump patch --caused-by monochange_core --reason "update dependency"`
- `mc change --package <id> --bump none --caused-by sdk --reason "dependency-only follow-up"`

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
