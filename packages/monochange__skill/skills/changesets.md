# Changesets skill

Use this guide when the task is to create, review, update, replace, or remove `.changeset/*.md` files.

## When to use this skill

Reach for it when you need to:

- create a new changeset from a code change
- decide whether to update an existing changeset instead of creating another one
- explain dependency propagation with `caused_by`
- choose between package ids and group ids
- describe a CLI, library, application, or protocol change in user-facing language

## Start with the workspace model

Before writing anything:

1. Read `monochange.toml`
2. Run `mc validate`
3. Run `mc discover --format json`
4. Review existing `.changeset/*.md` files
5. Use `mc diagnostics --format json` when you need git and PR context

## The default creation flow

Create a new changeset for one package:

```bash
mc change --package monochange --bump minor --reason "add release-record discovery"
```

Create a dependency-follow-up changeset:

```bash
mc change \
  --package monochange_config \
  --bump patch \
  --caused-by monochange_core \
  --reason "update dependency on monochange_core"
```

Create a no-version-bump acknowledgement for an affected package:

```bash
mc change \
  --package monochange_config \
  --bump none \
  --caused-by monochange_core \
  --reason "dependency-only follow-up"
```

## Package id vs. group id

Prefer a package id when one package changed directly:

```markdown
---
monochange_core: minor
---
```

Use a group id only when the outward release boundary is the whole group:

```markdown
---
sdk: minor
---
```

A good rule:

- **package id** — a leaf package changed and monochange can propagate the rest
- **group id** — the release note should read as one coordinated release owned by the group

## Scalar vs. object frontmatter

Scalar syntax is the shortest form:

```markdown
---
monochange: patch
---
```

Object syntax is for richer intent:

```markdown
---
monochange:
  bump: major
  version: "2.0.0"
  type: security
---
```

Use object syntax when you need:

- `version` for an explicit version target
- `type` for a custom changelog section
- `caused_by` for dependency propagation context

## `caused_by` and dependency propagation

Without `caused_by`, a dependent package gets an automatic dependency-bump record with little context.

**Without authored context:**

```markdown
# automatic propagation, no authored explanation

monochange_config -> patch
```

**With authored context:**

```markdown
---
monochange_config:
  bump: patch
  caused_by: ["monochange_core"]
---

#### update dependency on monochange_core

Bumps `monochange_core` after the `ChangelogFormat` API change.
```

Use `bump: none` when the package is affected but users do not need a version bump.

## Create vs. update vs. replace vs. remove

### Create a new changeset

Use a new file when the outward change is genuinely new.

```markdown
---
monochange: minor
---

#### add `mc diagnostics`

Introduce a command for changeset provenance.
```

### Update an existing changeset

Update in place when the same feature grew before release.

**Before:**

```markdown
---
monochange: minor
---

#### add `mc diagnostics`

Introduce a command for changeset provenance.
```

**After:**

```markdown
---
monochange: minor
---

#### add `mc diagnostics` for provenance and review context

Introduce a command for changeset provenance, linked review requests, and related issues.
```

### Replace a changeset

Replace the file when the implementation changed so much that the old note is misleading.

### Remove a changeset

Delete the file when the feature was reverted before release.

## Write user-facing summaries

Changeset bodies should describe what users notice.

### Weak

```markdown
#### refactor release logic
```

### Better

```markdown
#### add `--diff` previews to dry-run release output

`mc release --dry-run --diff` now shows unified file diffs for version and changelog updates without mutating the workspace.
```

## Artifact-specific framing

Different package types need different release-note framing:

- **Libraries** — public APIs, types, traits, exports, behavior
- **Applications** — routes, screens, workflows, UX changes
- **CLI tools** — commands, flags, output, exit codes, config shape
- **LSP/MCP** — tool names, schemas, protocol methods, response shapes

See [ARTIFACT-TYPES.md](../ARTIFACT-TYPES.md) for detailed templates.

## Validation loop

After writing or editing changesets:

```bash
mc validate
mc diagnostics --format json
mc release --dry-run --format json
```

Use `mc affected --changed-paths ...` in CI or review workflows when you need to prove all changed packages are covered.

## Keep these references nearby

- [CHANGESET-GUIDE.md](../CHANGESET-GUIDE.md) — lifecycle details
- [ARTIFACT-TYPES.md](../ARTIFACT-TYPES.md) — package-type-specific guidance
- [REFERENCE.md](../REFERENCE.md) — longer examples and config cross-references
