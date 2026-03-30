# Release planning

Create a changeset with the CLI:

<!-- {=releaseChangesAddCommand} -->

```bash
mc change --package sdk-core --bump minor --reason "public API addition"
mc change --package sdk-core --bump patch --type security --reason "rotate signing keys" --details "Roll the signing key before the release window closes."
```

<!-- {/releaseChangesAddCommand} -->

Or write one manually with configured package or group ids:

<!-- {=releaseManualChangesetExample} -->

```markdown
---
sdk-core: patch
type:
  sdk-core: security
---

#### rotate signing keys

Roll the signing key before the release window closes.
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
mc validate
```

Generate a plan directly when you want to inspect the raw planner output:

<!-- {=projectPlanCommand} -->

```bash
mc release --dry-run --format json
```

<!-- {/projectPlanCommand} -->

Preferred repository workflow:

<!-- {=projectDryRunCommand} -->

```bash
mc release --dry-run --format json
```

<!-- {/projectDryRunCommand} -->

<!-- {=projectReleaseCommand} -->

```bash
mc release
```

<!-- {/projectReleaseCommand} -->

<!-- {=releaseWorkflowBehavior} -->

`mc release` is a workflow-defined top-level command. When your config omits workflows, MonoChange synthesizes the default `release` workflow automatically.

During migration, you may still see references to `[[package_overrides]]` in older documentation or repositories, but release preparation now expects package/group declarations and consumes `.changeset/*.md` files through that new model.

Current `PrepareRelease` behavior:

- reads `.changeset/*.md`
- computes one synchronized release plan from discovered change files
- updates native manifests plus configured changelogs and versioned files
- renders changelog files through structured release notes using the configured `monochange` or `keep_a_changelog` format
- groups release notes into default `Breaking changes`, `Features`, `Fixes`, and `Notes` sections, with package/group overrides available through `extra_changelog_sections`
- applies workspace-wide release-note templates from `[release_notes].change_templates`
- can snapshot the prepared release as a stable JSON manifest via `RenderReleaseManifest`
- can preview or publish GitHub releases via `PublishGitHubRelease`
- applies group-owned release identity for outward `tag`, `release`, and `version_format`
- deletes consumed change files only after a successful non-dry-run execution
- leaves the workspace untouched during `--dry-run` except for explicitly requested outputs such as a rendered release manifest or GitHub release preview

<!-- {/releaseWorkflowBehavior} -->

Planning rules in this milestone:

<!-- {=releasePlanningRules} -->

- `mc change` defaults `--bump` to `patch`
- markdown change files require an explicit `patch`, `minor`, or `major` entry per package
- optional change `type` values can route entries into custom changelog sections without changing semver impact
- change templates support detailed multi-line release-note entries through `$details`
- dependents default to the configured `parent_bump`
- Rust semver evidence can escalate both the changed crate and its dependents
- configured groups synchronize before final output is rendered
- release targets carry effective `tag`, `release`, and `version_format` metadata
- release-manifest JSON captures release targets, changelog payloads, changed files, and the synchronized release plan for downstream automation
- `PublishGitHubRelease` reuses the same structured release data to build GitHub release requests for grouped and package-owned releases
- CLI text and JSON output render workspace paths relative to the repository root for stable snapshots and automation

<!-- {/releasePlanningRules} -->
