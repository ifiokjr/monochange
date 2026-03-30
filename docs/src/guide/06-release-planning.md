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

## GitHub automation built on the release plan

<!-- {=githubAutomationOverview} -->

MonoChange keeps GitHub automation layered on top of the same `PrepareRelease` result used for normal release planning.

That means one set of `.changeset/*.md` inputs can drive all of these workflows consistently:

- `mc release-manifest` writes a stable JSON artifact for downstream automation
- `mc publish-release` previews or publishes GitHub releases from the structured release notes
- `mc release-pr` previews or opens an idempotent release pull request
- `mc release-deploy` emits deployment intents for later workflow execution
- `mc changeset-check` evaluates pull-request changeset policy from CI-supplied changed paths and labels

<!-- {/githubAutomationOverview} -->

<!-- {=githubAutomationWorkflowCommands} -->

```bash
mc release --dry-run --format json
mc release-manifest --dry-run
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
mc release-deploy --dry-run --format json
mc changeset-check --format json --changed-path crates/monochange/src/lib.rs
```

<!-- {/githubAutomationWorkflowCommands} -->

For a complete repository example, see the dedicated [GitHub automation guide](./08-github-automation.md).

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
- can preview or open/update release pull requests via `OpenReleasePullRequest`
- can emit deployment intents via `Deploy` for merge-driven or workflow-driven deploy orchestration
- can evaluate pull-request changeset policy via `EnforceChangesetPolicy` using changed paths and labels supplied by CI
- includes any emitted deployment intents in manifest JSON so downstream CI can gate or fan out deployments safely
- applies group-owned release identity for outward `tag`, `release`, and `version_format`
- deletes consumed change files only after a successful non-dry-run execution
- leaves the workspace untouched during `--dry-run` except for explicitly requested outputs such as a rendered release manifest or GitHub release preview

A GitHub Actions check can pass changed paths and labels directly into a policy workflow, for example:

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
          args=(changeset-check --format json)
          for path in $CHANGED_FILES; do
            args+=(--changed-path "$path")
          done
          for label in "${labels[@]}"; do
            args+=(--label "$label")
          done
          devenv shell -- mc "${args[@]}" | tee policy.json
          jq -e '.status != "failed"' policy.json >/dev/null
```

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
- `OpenReleasePullRequest` reuses the same structured release data to render release-PR summaries, branch names, and idempotent PR updates
- `Deploy` turns configured `[[deployments]]` entries into structured deployment intents for release manifests and downstream automation
- `EnforceChangesetPolicy` evaluates changed paths, skip labels, and changed `.changeset/*.md` files into reusable pass/skip/fail diagnostics and optional failure comments
- CLI text and JSON output render workspace paths relative to the repository root for stable snapshots and automation

<!-- {/releasePlanningRules} -->
