# Release planning

Create a changeset with the CLI:

<!-- {=releaseChangesAddCommand} -->

```bash
mc changes add --root . --package sdk-core --bump minor --reason "public API addition"
```

<!-- {/releaseChangesAddCommand} -->

Or write one manually with configured package or group ids:

<!-- {=releaseManualChangesetExample} -->

```markdown
---
sdk-core: minor
---

#### public API addition
```

<!-- {/releaseManualChangesetExample} -->

Group-targeted changesets are also valid:

```markdown
---
sdk: minor
---

#### coordinated SDK release
```

Optionally include Rust semver evidence:

<!-- {=releaseEvidenceExample} -->

```markdown
---
sdk-core: patch
evidence:
  sdk-core:
    - rust-semver:major:public API break detected
---

#### breaking API change
```

<!-- {/releaseEvidenceExample} -->

Validate before planning:

```bash
mc check --root .
```

Generate a plan directly when you want to inspect the raw planner output:

<!-- {=projectPlanCommand} -->

```bash
mc plan release --root . --changes .changeset/my-change.md --format json
```

<!-- {/projectPlanCommand} -->

Preferred repository workflow:

<!-- {=projectDryRunCommand} -->

```bash
mc release --dry-run
```

<!-- {/projectDryRunCommand} -->

<!-- {=projectReleaseCommand} -->

```bash
mc release
```

<!-- {/projectReleaseCommand} -->

<!-- {=releaseWorkflowBehavior} -->

`mc release` only works when your config defines a workflow named `release`.

During migration, you may still see references to `[[package_overrides]]` in older documentation or repositories, but release preparation now expects package/group declarations and consumes `.changeset/*.md` files through that new model.

Current `PrepareRelease` behavior:

- reads `.changeset/*.md`
- computes one synchronized release plan from discovered change files
- updates native manifests plus configured changelogs and versioned files
- applies group-owned release identity for outward `tag`, `release`, and `version_format`
- deletes consumed change files only after a successful non-dry-run execution
- leaves the workspace untouched during `--dry-run`

<!-- {/releaseWorkflowBehavior} -->

Planning rules in this milestone:

<!-- {=releasePlanningRules} -->

- `mc changes add` defaults `--bump` to `patch`
- markdown change files require an explicit `patch`, `minor`, or `major` entry per package
- dependents default to the configured `parent_bump`
- Rust semver evidence can escalate both the changed crate and its dependents
- configured groups synchronize before final output is rendered
- release targets carry effective `tag`, `release`, and `version_format` metadata

<!-- {/releasePlanningRules} -->
