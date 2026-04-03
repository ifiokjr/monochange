# Release planning

Create a changeset with the CLI:

<!-- {=releaseChangesAddCommand} -->

```bash
mc change --package sdk-core --bump minor --reason "public API addition"
mc change --package sdk-core --bump patch --type security --reason "rotate signing keys" --details "Roll the signing key before the release window closes."
mc change --package sdk-core --bump major --reason "break the public API" --evidence rust-semver:major:public API break detected --output .changeset/sdk-core-major.md
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

MonoChange keeps its own changeset standard rather than reusing a narrower external parser. In addition to package/group bump entries, MonoChange changesets can include reserved metadata keys such as `evidence`, `origin`, and `type`, while the markdown body is split into a summary plus optional detailed follow-up paragraphs.

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

`mc release` is a config-defined top-level command. When your config omits `[cli.<command>]` entries, MonoChange synthesizes the default `release` command automatically.

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
- can emit deployment intents via `Deploy` for merge-driven or CI-driven deploy orchestration
- can evaluate pull-request changeset policy via `VerifyChangesets` using changed paths and labels supplied by CI
- includes any emitted deployment intents in manifest JSON so downstream CI can gate or fan out deployments safely
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

- `mc change` defaults `--bump` to `patch`
- markdown change files require an explicit `patch`, `minor`, or `major` entry per package
- optional change `type` values can route entries into custom changelog sections without changing semver impact
- `mc change` can attach extra `--evidence ...` entries and write to a deterministic path with `--output ...`
- change templates support detailed multi-line release-note entries through `{{ details }}`, compact metadata blocks through `{{ context }}`, and fine-grained linked metadata like `{{ change_owner_link }}`, `{{ review_request_link }}`, and `{{ closed_issue_links }}`
- dependents default to the configured `parent_bump`
- Rust semver evidence can escalate both the changed crate and its dependents
- configured groups synchronize before final output is rendered
- release targets carry effective `tag`, `release`, and `version_format` metadata
- release-manifest JSON captures release targets, changelog payloads, authored changesets, linked changeset context metadata, changed files, and the synchronized release plan for downstream automation
- `PublishRelease` reuses the same structured release data to build provider release requests for grouped and package-owned releases
- `OpenReleaseRequest` reuses the same structured release data to render release-request summaries, branch names, and idempotent provider updates
- `CommentReleasedIssues` can use linked changeset context metadata to add follow-up comments to closed issues after a release is published
- `Deploy` turns configured `[[deployments]]` entries into structured deployment intents for release manifests and downstream automation
- `VerifyChangesets` evaluates changed paths, skip labels, and changed `.changeset/*.md` files into reusable pass/skip/fail diagnostics and optional failure comments
- CLI text and JSON output render workspace paths relative to the repository root for stable snapshots and automation

<!-- {/releasePlanningRules} -->
