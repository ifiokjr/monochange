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

`caused_by` always uses the object form and can point at package ids or group ids.

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

`caused_by` suppresses only the matching automatic propagation. Other unrelated upstream changes can still propagate normally.

CLI authoring accepts repeated `--caused-by <id>` flags when more than one upstream package or group explains the follow-up change.

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

See [artifact-types.md](./artifact-types.md) for detailed templates.

## Validation loop

After writing or editing changesets:

```bash
mc validate
mc diagnostics --format json
mc release --dry-run --format json
```

Use `mc step:affected-packages --verify --changed-paths ...` in CI or review workflows when you need to prove all changed packages are covered without depending on a config-defined wrapper.

## Changeset cleanup job

Before release, audit all pending changesets and resolve duplicates, stale entries, and incomplete descriptions.

### Step 1: Export diagnostics data

```bash
mc diagnostics --format json > /tmp/changesets.json
```

This produces a JSON document with `requestedChangesets` and `changesets` arrays containing paths, summaries, targets, and git context.

### Step 2: Filter for common issues

**Find short summaries (likely incomplete):**

```bash
cat /tmp/changesets.json | jq '.changesets[] | select((.summary | length) < 20)'
```

**Find changesets touching a specific package:**

```bash
cat /tmp/changesets.json | jq '.changesets[] | select(.targets[].id == "monochange_core")'
```

**Find changesets without git context:**

```bash
cat /tmp/changesets.json | jq '.changesets[] | select(.context.introduced == null)'
```

**Find changesets with duplicate summaries:**

```bash
cat /tmp/changesets.json | jq -r '.changesets[].summary' | sort | uniq -d
```

### Step 3: Decision matrix

| Situation                                | Action                                 |
| ---------------------------------------- | -------------------------------------- |
| **Same feature in multiple changesets**  | Merge into one multi-package changeset |
| **Feature reverted / PR closed**         | Remove the changeset file              |
| **Description too vague**                | Update body with user-facing details   |
| **Wrong target packages**                | Edit frontmatter to correct targets    |
| **Same change, same PR, multiple files** | Consolidate into single changeset      |

### Step 4: Merge duplicate changesets

When two changesets describe the same feature:

```bash
# Read both source changesets
cat .changeset/feature-cli.md
cat .changeset/feature-core.md

# Create merged version
cat > .changeset/unified-feature.md << 'EOF'
---
monochange_cli: minor
monochange_core: minor
---

#### add unified feature across CLI and core

Description covering both packages.
EOF

# Remove obsolete changesets
git rm .changeset/feature-cli.md .changeset/feature-core.md
git add .changeset/unified-feature.md
```

### Step 5: Remove stale changesets

```bash
# Verify the changeset is truly stale
mc diagnostics --changeset .changeset/stale-feature.md

# Confirm the feature was reverted or abandoned
git log --oneline -- .changeset/stale-feature.md

# Remove
git rm .changeset/stale-feature.md
```

### Step 6: Validation checklist

Before finalizing cleanup:

- [ ] `mc validate` passes
- [ ] `mc diagnostics --format json` loads all remaining changesets without error
- [ ] `mc step:affected-packages --verify --changed-paths <files>` confirms coverage for recent changes
- [ ] No duplicate summaries across changesets
- [ ] No changesets reference reverted features
- [ ] All changesets have user-facing descriptions per [changeset-guide.md](./changeset-guide.md)

## Keep these references nearby

- [changeset-guide.md](./changeset-guide.md) — lifecycle details
- [artifact-types.md](./artifact-types.md) — package-type-specific guidance
- [reference.md](./reference.md) — longer examples and config cross-references
