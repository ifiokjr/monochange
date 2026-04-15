# GitHub automation

<!-- {=githubAutomationOverview} -->

monochange keeps source-provider automation layered on top of the same `PrepareRelease` result used for normal release planning.

That means one set of `.changeset/*.md` inputs can drive all of these commands and automation flows consistently:

- `mc release --dry-run --format json` refreshes the cached manifest and shows the downstream automation payload
- `mc publish-release` previews or publishes provider releases from the structured release notes
- `mc release-pr` previews or opens an idempotent provider release request
- `mc affected` evaluates pull-request changeset policy from CI-supplied changed paths and labels

<!-- {/githubAutomationOverview} -->

## Quick start with `mc init --provider`

The fastest way to configure GitHub automation is using the `--provider` flag during initialization:

<!-- {=initProviderQuickStart} -->

```bash
# Initialize with GitHub automation pre-configured
mc init --provider github

# The generated monochange.toml includes:
# - [source] section with GitHub releases and pull request settings
# - CLI commands for commit-release and release-pr
# - GitHub Actions workflows in .github/workflows/
```

This single command generates:

1. **Complete source configuration** — `[source]`, `[source.releases]`, and `[source.pull_requests]` sections
2. **Automation CLI commands** — `commit-release` and `release-pr` commands ready to use
3. **GitHub Actions workflows** — `release.yml` and `changeset-policy.yml` for CI/CD
4. **Auto-detected repository info** — parses your git remote to pre-fill owner and repo

<!-- {/initProviderQuickStart} -->

## CLI commands

<!-- {=githubAutomationWorkflowCommands} -->

```bash
mc release --dry-run --format json
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
mc affected --format json --changed-paths crates/monochange/src/lib.rs
```

<!-- {/githubAutomationWorkflowCommands} -->

## Inspecting and repairing a recent release

GitHub automation now has a repair-oriented history flow in addition to the existing manifest-driven execution flow.

Use these commands when you need to inspect, tag, or repair a just-created release:

```bash
mc release-record --from v1.2.3
mc tag-release --from HEAD --dry-run --format json
mc repair-release --from v1.2.3 --target HEAD --dry-run
mc repair-release --from v1.2.3 --target HEAD
```

The important distinction is:

- the cached release manifest still describes the execution-time release plan for automation
- `ReleaseRecord` describes the durable release declaration stored in the release commit body
- `mc tag-release` consumes that durable record after merge and creates the declared tag set on the default branch

Use `--dry-run` first for `repair-release`. It is a destructive workflow because it retargets release tags.

If immutable registry artifacts have already been published, prefer cutting a new patch release instead of retargeting the source release.

## Package publishing and trusted publishing

Package publishing is separate from provider release publishing:

- `mc publish` handles package registries such as `crates.io`, `npm`, `jsr`, and `pub.dev`
- `mc publish-release` handles hosted source-provider releases such as GitHub releases

When `publish.trusted_publishing` is enabled, monochange can derive GitHub trust metadata from the workflow runtime and the configured `[source]` block. npm packages are the only ecosystem with built-in bulk trust automation today:

- monochange checks the existing trust configuration first
- if trust is missing, it runs `npm trust github ...`
- pnpm workspaces run the trust command through `pnpm exec npm trust ...`
- monochange verifies the result after running the trust command instead of assuming success

For `crates.io`, `jsr`, and `pub.dev`, monochange reports the setup URL for the package and requires manual trusted-publishing setup before the next built-in release publish. Placeholder publishing can still proceed so the package exists before that manual step.

For exact registry-side setup steps and field mappings, see [Trusted publishing and OIDC](./07-trusted-publishing.md).

For full GitHub and GitLab CI examples by ecosystem — npm, Cargo, Deno/JSR, and Dart/pub.dev — see [Advanced: CI, package publishing, and release PR flows](./13-ci-and-publishing.md).

## Release notes, GitHub releases, and release PRs

<!-- {=githubAutomationReleaseConfigExample} -->

```toml
[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"

[release_notes]
change_templates = [
	"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ details }}",
	"- {{ summary }}",
]

[group.main.changelog]
path = "changelog.md"
format = "monochange"

[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[source.releases]
enabled = true
source = "monochange"

[source.pull_requests]
enabled = true
branch_prefix = "monochange/release"
base = "main"
title = "chore(release): prepare release"
labels = ["release", "automated"]
auto_merge = false

[cli.publish-release]
help_text = "Prepare a release and publish provider releases"

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
type = "PrepareRelease"

[[cli.publish-release.steps]]
type = "PublishRelease"

[[cli.publish-release.steps]]
type = "CommentReleasedIssues"

[cli.release-pr]
help_text = "Prepare a release and open or update a provider release request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
```

<!-- {/githubAutomationReleaseConfigExample} -->

When you want fine-grained changelog formatting instead of the default `{{ context }}` block, GitHub-backed release notes can reference individual metadata fields such as `{{ change_owner_link }}`, `{{ review_request_link }}`, `{{ introduced_commit_link }}`, `{{ closed_issue_links }}`, and `{{ related_issue_links }}`. Those variables render markdown links when host URLs are available, so generated changelogs can point directly at the responsible actor, the PR, and linked issues. The source changeset path stays available through `{{ changeset_path }}`, but `{{ context }}` keeps that transient file path out of the default rendered note.

```toml
[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[source.bot.changesets]
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
	"npm/**",
	"skills/**",
]
ignored_paths = [
	".changeset/**",
	"docs/**",
	"specs/**",
	"readme.md",
	"CONTRIBUTING.md",
	"license",
]

name = "docs"
trigger = "release_published"
workflow = "docs-release"
environment = "github-pages"
release_targets = ["main"]
requires = ["main"]
metadata = { site = "github-pages" }

name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

type = "PrepareRelease"

[cli.affected]
help_text = "Evaluate pull-request changeset policy"

[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.affected.inputs]]
name = "label"
type = "string_list"

[[cli.affected.steps]]
type = "AffectedPackages"
```

## Release and npm publish workflows

monochange now includes a release workflow modeled around long-running release PR refresh plus post-merge tagging:

- `.github/workflows/release.yml` refreshes the dedicated release PR branch on normal `main` pushes
- the same workflow detects when `HEAD` is already a merged monochange release commit, runs `mc tag-release --from HEAD`, and then runs `mc publish`
- tag-triggered or downstream workflows can then build archives, create hosted releases, publish additional assets from the pushed tags, or run a separate `mc publish-release` job when you still want manifest-driven hosted-release publication

That split keeps tag creation on the default branch side of the merge and lets downstream automation consume the exact durable release metadata that monochange stored in git history.

For release repair, GitHub is also the first provider with hosted-release retarget sync support. monochange uses the durable release record plus tag names from that record to keep the hosted release view aligned with moved tags.

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

## Dogfooding on the monochange repository

<!-- {=githubAutomationDogfoodNotes} -->

The monochange repository itself can dogfood this model by:

- declaring `[source]`, `[source.releases]`, and `[source.pull_requests]` in `monochange.toml`
- running a real `changeset-policy` GitHub Actions workflow that shells into `mc affected`

<!-- {/githubAutomationDogfoodNotes} -->

## Supported providers

The `--provider` flag supports three source providers:

| Provider | `--provider` value | Workflow generation       | Release automation | Pull/merge requests |
| -------- | ------------------ | ------------------------- | ------------------ | ------------------- |
| GitHub   | `github`           | Yes — GitHub Actions      | Yes                | Yes                 |
| GitLab   | `gitlab`           | No — use `.gitlab-ci.yml` | Yes                | Yes                 |
| Gitea    | `gitea`            | No — use Gitea Actions    | Yes                | Yes                 |

All providers configure the `[source]` section in `monochange.toml` with appropriate settings for releases and pull/merge requests. GitLab and Gitea require manual CI configuration since they don't support GitHub Actions workflow files.

If you are comparing provider-specific CI layouts or designing a long-running release PR branch, continue with [Advanced: CI, package publishing, and release PR flows](./13-ci-and-publishing.md).
