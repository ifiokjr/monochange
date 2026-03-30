# GitHub automation

<!-- {=githubAutomationOverview} -->

MonoChange keeps GitHub automation layered on top of the same `PrepareRelease` result used for normal release planning.

That means one set of `.changeset/*.md` inputs can drive all of these commands and automation flows consistently:

- `mc release-manifest` writes a stable JSON artifact for downstream automation
- `mc publish-release` previews or publishes GitHub releases from the structured release notes
- `mc release-pr` previews or opens an idempotent release pull request
- `mc release-deploy` emits deployment intents for later workflow execution
- `mc changeset-check` evaluates pull-request changeset policy from CI-supplied changed paths and labels

<!-- {/githubAutomationOverview} -->

## CLI commands

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

## Release notes, GitHub releases, and release PRs

<!-- {=githubAutomationReleaseConfigExample} -->

```toml
[defaults.changelog]
path = "{path}/changelog.md"
format = "keep_a_changelog"

[release_notes]
change_templates = ["#### $summary\n\n$details", "- $summary"]

[group.main.changelog]
path = "changelog.md"
format = "monochange"

[github]
owner = "ifiokjr"
repo = "monochange"

[github.releases]
enabled = true
source = "monochange"

[github.pull_requests]
enabled = true
branch_prefix = "monochange/release"
base = "main"
title = "chore(release): prepare release"
labels = ["release", "automated"]
auto_merge = false

[cli.release-manifest]
help_text = "Prepare a release and write a stable JSON manifest"

[[cli.release-manifest.steps]]
type = "PrepareRelease"

[[cli.release-manifest.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"

[cli.publish-release]
help_text = "Prepare a release and publish GitHub releases"

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
type = "PrepareRelease"

[[cli.publish-release.steps]]
type = "PublishGitHubRelease"

[cli.release-pr]
help_text = "Prepare a release and open or update a GitHub release pull request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleasePullRequest"
```

<!-- {/githubAutomationReleaseConfigExample} -->

## Deployment intents and changeset policy

<!-- {=githubAutomationPolicyAndDeployConfigExample} -->

```toml
[github.bot.changesets]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = [
	"crates/**",
	".github/**",
	"Cargo.toml",
	"Cargo.lock",
	"devenv.nix",
	"devenv.yaml",
	"devenv.lock",
	"monochange.toml",
	"codecov.yml",
	"deny.toml",
	"scripts/**",
]
ignored_paths = [
	".changeset/**",
	"docs/**",
	"**/*.md",
	"license",
]

[[deployments]]
name = "docs"
trigger = "release_published"
workflow = "docs-release"
environment = "github-pages"
release_targets = ["main"]
requires = ["main"]
metadata = { site = "github-pages" }

[cli.release-deploy]
help_text = "Prepare a release and emit deployment intents"

[[cli.release-deploy.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-deploy.steps]]
type = "PrepareRelease"

[[cli.release-deploy.steps]]
type = "Deploy"

[cli.changeset-check]
help_text = "Evaluate pull-request changeset policy"

[[cli.changeset-check.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.changeset-check.inputs]]
name = "changed_path"
type = "string_list"
required = true

[[cli.changeset-check.inputs]]
name = "label"
type = "string_list"

[[cli.changeset-check.steps]]
type = "EnforceChangesetPolicy"
```

<!-- {/githubAutomationPolicyAndDeployConfigExample} -->

## GitHub Actions policy workflow

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
          args=(changeset-check --format json)

          for path in $CHANGED_FILES; do
            args+=(--changed-path "$path")
          done

          for label in "${labels[@]}"; do
            args+=(--label "$label")
          done

          devenv shell -- mc "${args[@]}" | tee policy.raw
          awk 'BEGIN { capture = 0 } /^\{/ { capture = 1 } capture { print }' policy.raw > policy.json
          jq -e '.status != "failed"' policy.json >/dev/null
```

<!-- {/changesetPolicyGitHubActionWorkflow} -->

## Dogfooding on the monochange repository

<!-- {=githubAutomationDogfoodNotes} -->

The MonoChange repository itself can dogfood this model by:

- declaring `[github]`, `[github.releases]`, and `[github.pull_requests]` in `monochange.toml`
- exposing `release-manifest`, `publish-release`, `release-pr`, `release-deploy`, and `changeset-check` as top-level CLI commands
- running a real `changeset-policy` GitHub Actions workflow that shells into `mc changeset-check`
- keeping docs deployment represented as a deployment intent so downstream workflows can reason about it from the release manifest

<!-- {/githubAutomationDogfoodNotes} -->
