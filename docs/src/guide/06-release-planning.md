# Release planning

Create a change input file with the CLI:

<!-- {=releaseChangesAddCommand} -->

```bash
mc changes add --root . --package sdk_core --bump minor --reason "public API addition"
```

<!-- {/releaseChangesAddCommand} -->

Or write one manually:

<!-- {=releaseManualChangesetExample} -->

```markdown
---
sdk_core: minor
---

#### public API addition
```

<!-- {/releaseManualChangesetExample} -->

Optionally include Rust semver evidence and explicit origins in markdown frontmatter:

<!-- {=releaseEvidenceExample} -->

```markdown
---
sdk_core: patch
origin:
  sdk_core: direct-change
evidence:
  sdk_core:
    - rust-semver:major:public API break detected
---

#### breaking API change
```

<!-- {/releaseEvidenceExample} -->

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

Planning rules in this milestone:

<!-- {=releasePlanningRules} -->

- `mc changes add` defaults `--bump` to `patch`
- markdown change files require an explicit `patch`, `minor`, or `major` entry per package
- dependents default to the configured `parent_bump`
- Rust semver evidence can escalate both the changed crate and its dependents
- version-group synchronization runs before final output is rendered

<!-- {/releasePlanningRules} -->

## PrepareRelease workflow behavior

`mc release` only works when your config defines a workflow named `release`.

<!-- {=releaseWorkflowBehavior} -->

Current `PrepareRelease` behavior:

- reads `.changeset/*.md`
- computes one synchronized release plan from discovered change files
- updates Cargo package versions and Cargo workspace dependency versions when a release is applied
- appends changelog sections only for packages configured through `[[package_overrides]]` with `changelog` paths
- deletes consumed change files only after a successful non-dry-run execution
- leaves the workspace untouched during `--dry-run`

<!-- {/releaseWorkflowBehavior} -->
