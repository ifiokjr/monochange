# Release planning

Create a changeset with the CLI:

<!-- {=releaseChangesAddCommand} -->

```bash
mc change --package sdk-core --bump minor --reason "public API addition"
mc change --package sdk-core --bump patch --type security --reason "rotate signing keys" --details "Roll the signing key before the release window closes."
mc change --package sdk-core --bump none --type docs --reason "clarify migration guidance" --output .changeset/sdk-core-docs.md
mc change --package sdk-core --bump major --version 2.0.0 --reason "break the public API" --output .changeset/sdk-core-major.md
```

Or use interactive mode to select packages, bumps, and options from a guided wizard:

```bash
mc change -i
```

Interactive mode automatically prevents conflicting selections (a group and one of its members) and lets you pick per-package bumps and optional explicit versions.

<!-- {/releaseChangesAddCommand} -->

Or write one manually with configured package or group ids:

<!-- {=releaseManualChangesetExample} -->

```markdown
---
sdk-core:
  bump: patch
  type: security
---

# rotate signing keys

Roll the signing key before the release window closes.
```

<!-- {/releaseManualChangesetExample} -->

Group-targeted changesets are also valid:

```markdown
---
sdk: minor
---

# coordinated SDK release
```

<!-- {=releaseExplicitVersionChangesetExample} -->

Use scalar shorthand for plain bumps (`sdk-core: minor`) or for configured change types (`sdk-core: security`). To pin an exact version or combine `bump`, `version`, and `type`, use the object syntax:

```markdown
---
sdk-core:
  bump: major
  version: "2.0.0"
---

# promote to stable
```

When `version` is provided without `bump`, the bump is inferred from the current version. If the package belongs to a version group, the explicit version propagates to the whole group.

<!-- {/releaseExplicitVersionChangesetExample} -->

If multiple changesets specify conflicting explicit versions for the same package or group, monochange uses the highest semver version and emits a warning by default. Set `defaults.strict_version_conflicts = true` to fail instead.

monochange keeps its own changeset standard rather than reusing a narrower external parser. Top-level frontmatter keys are package ids or group ids only. Each target can use scalar shorthand or the object syntax with `bump`, `version`, and `type`, while the markdown body is split into a summary plus optional detailed follow-up paragraphs. Authored heading depth is normalized when release notes are rendered, so use natural markdown headings in the changeset body instead of hard-coding output depth.

Validate before planning:

```bash
mc validate
```

## Release manifests vs release records

Release planning and release repair use two different artifacts on purpose.

- `RenderReleaseManifest` captures **what monochange is preparing right now** during command execution.
- `ReleaseRecord` captures **what a release commit historically declared** inside the monochange-managed release commit body.

Use the manifest when you want execution-time automation such as CI artifacts, MCP/server responses, previews, or downstream machine-readable release data.

Use the release record when you want to rediscover or repair a release later from a tag or descendant commit.

That is why `ReleaseRecord` does not replace `RenderReleaseManifest`: one is an execution-time automation artifact, the other is a durable git-history artifact.

When you need to inspect or repair a recent release, see [Repairable releases](./12-repairable-releases.md).

Generate a plan directly when you want to inspect the raw planner output:

<!-- {=projectPlanCommand} -->

```bash
mc release --dry-run --format json
```

<!-- {/projectPlanCommand} -->

Preferred repository command flow:

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

`mc release` is a config-defined top-level command. When your config omits `[cli.<command>]` entries, monochange synthesizes the default `release` command automatically.

During migration, you may still see references to `[[package_overrides]]` in older documentation or repositories, but release preparation now expects package/group declarations and consumes `.changeset/*.md` files through that new model.

Current `PrepareRelease` behavior:

- reads `.changeset/*.md`
- computes one synchronized release plan from discovered change files
- updates native manifests plus configured changelogs and versioned files
- renders changelog files through structured release notes using the configured `monochange` or `keep_a_changelog` format
- groups release notes into default `Breaking changes`, `Features`, `Fixes`, and `Notes` sections, with package/group overrides available through `extra_changelog_sections`
- applies workspace-wide release-note templates from `[release_notes].change_templates`
- can snapshot the prepared release as a stable JSON manifest via `RenderReleaseManifest`
- can preview or publish provider releases via `PublishRelease`
- can preview or open/update release requests via `OpenReleaseRequest`
- can comment on released issues via `CommentReleasedIssues`
- can evaluate pull-request changeset policy via `AffectedPackages` using changed paths and labels supplied by CI
- applies group-owned release identity for outward `tag`, `release`, and `version_format`
- deletes consumed change files only after a successful non-dry-run execution
- leaves the workspace untouched during `--dry-run` except for explicitly requested outputs such as a rendered release manifest or release preview

A GitHub Actions check can pass changed paths and labels directly into a policy workflow, for example:

<!-- {/releaseWorkflowBehavior} -->

<!-- {=changesetPolicyGitHubActionWorkflow} -->

```yaml
name: changeset-policy

on:
  pull_request:
    types:
      - opened
      - synchronize
      - reopened
      - labeled
      - unlabeled

concurrency:
  group: changeset-policy-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true

jobs:
  check:
    timeout-minutes: 60
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: read
    steps:
      - name: checkout repository
        uses: actions/checkout@v6

      - name: setup
        uses: ./.github/actions/devenv
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: collect changed files
        id: changed
        uses: tj-actions/changed-files@v46

      - name: run changeset policy
        env:
          PR_LABELS_JSON: ${{ toJson(github.event.pull_request.labels.*.name) }}
          CHANGED_FILES: ${{ steps.changed.outputs.all_changed_files }}
        shell: bash
        run: |
          set -euo pipefail

          mapfile -t labels < <(jq -r '.[]' <<<"$PR_LABELS_JSON")
          args=(verify --format json)

          for path in $CHANGED_FILES; do
            args+=(--changed-paths "$path")
          done

          for label in "${labels[@]}"; do
            args+=(--label "$label")
          done

          devenv shell -- mc "${args[@]}" | tee policy.raw
          awk 'BEGIN { capture = 0 } /^\{/ { capture = 1 } capture { print }' policy.raw > policy.json
          jq -e '.status != "failed"' policy.json >/dev/null
```

<!-- {/changesetPolicyGitHubActionWorkflow} -->

Planning rules in this milestone:

<!-- {=releasePlanningRules} -->

- `mc change` defaults `--bump` to `patch`; use `--bump none` when you want a type-only or version-only entry, and pass `--version` to pin an explicit release version
- markdown change files use package/group ids as the only top-level frontmatter keys, with scalar shorthand for `none`/`patch`/`minor`/`major` or configured change types, plus object syntax for `bump`, `version`, and/or `type`
- when `version` is given without `bump`, the bump is inferred by comparing the current and target versions
- explicit versions from grouped members propagate to the group version; conflicts take the highest semver or fail when `defaults.strict_version_conflicts = true`
- prefer package ids over group ids in authored changesets when possible; direct package changes still propagate to dependents and synchronize configured groups
- optional change `type` values can route entries into custom changelog sections, and configured section `default_bump` values let scalar type shorthand imply the desired semver behavior
- `mc change` can write to a deterministic path with `--output ...`
- change templates support detailed multi-line release-note entries through `{{ details }}`, compact metadata blocks through `{{ context }}`, and fine-grained linked metadata like `{{ change_owner_link }}`, `{{ review_request_link }}`, and `{{ closed_issue_links }}`
- dependents default to the configured `parent_bump`, including packages outside a changed version group when they depend on a synchronized member
- computed compatibility evidence can still escalate both the changed crate and its dependents when provider analysis produces it
- configured groups synchronize before final output is rendered
- release targets carry effective `tag`, `release`, and `version_format` metadata
- release-manifest JSON captures release targets, changelog payloads, authored changesets, linked changeset context metadata, changed files, and the synchronized release plan for downstream automation
- `PublishRelease` reuses the same structured release data to build provider release requests for grouped and package-owned releases
- `OpenReleaseRequest` reuses the same structured release data to render release-request summaries, branch names, and idempotent provider updates
- `CommentReleasedIssues` can use linked changeset context metadata to add follow-up comments to closed issues after a release is published
- `AffectedPackages` evaluates changed paths, skip labels, and changed `.changeset/*.md` files into reusable pass/skip/fail diagnostics and optional failure comments
- CLI text and JSON output render workspace paths relative to the repository root for stable snapshots and automation

<!-- {/releasePlanningRules} -->
