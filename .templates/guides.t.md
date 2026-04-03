<!-- {@discoverySupportedSources} -->

- Cargo workspaces and standalone crates
- npm workspaces, pnpm workspaces, Bun workspaces, and standalone `package.json` packages
- Deno workspaces and standalone `deno.json` / `deno.jsonc` packages
- Dart and Flutter workspaces plus standalone `pubspec.yaml` packages

<!-- {/discoverySupportedSources} -->

<!-- {@discoveryKeyBehaviors} -->

- native workspace globs are expanded by each ecosystem adapter
- dependency names are normalized into one graph
- package ids and manifest paths in CLI output are rendered relative to the repository root for deterministic automation
- version-group assignments are attached after discovery
- unmatched group members and version mismatches produce warnings
- discovery currently scans all supported ecosystems regardless of `[ecosystems.*]` toggles in `monochange.toml`

<!-- {/discoveryKeyBehaviors} -->

<!-- {@configurationDefaultsSnippet} -->

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"
```

<!-- {/configurationDefaultsSnippet} -->

<!-- {@configurationVersionGroupsSnippet} -->

```toml
[defaults]
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"

[package.sdk-core]
path = "crates/sdk_core"
versioned_files = ["crates/sdk_core/extra.toml"]
tag = false
release = false
version_format = "namespaced"

[package.sdk-core.changelog]
path = "crates/sdk_core/CHANGELOG.md"
format = "monochange"
```

<!-- {/configurationVersionGroupsSnippet} -->

<!-- {@configurationPackageOverridesSnippet} -->

Legacy repositories may still contain `[[package_overrides]]` entries such as:

```toml
[[package_overrides]]
package = "crates/sdk_core"
changelog = "crates/sdk_core/changelog.md"
```

Under the new model, move that changelog configuration onto the matching `[package.<id>]` declaration instead. When `[defaults].package_type` is set, package entries may also omit an explicit `type`.

MonoChange currently supports two changelog formats:

- `monochange` keeps the current heading-and-bullets layout
- `keep_a_changelog` renders section headings such as `### Features`, `### Fixes`, and `### Breaking changes`

Defaults can set a repository-wide changelog path pattern and format, while package and group changelog tables can override either field.

You can also customize release-note rendering with a workspace-wide `[release_notes]` table plus per-package or per-group `extra_changelog_sections` definitions.

Supported template variables include:

| Variable                         | Meaning                                                               | Notes                                                                                                      |
| -------------------------------- | --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `{{ summary }}`                  | rendered release-note summary heading                                 | always available                                                                                           |
| `{{ details }}`                  | optional long-form details body                                       | omitted when the changeset has no details                                                                  |
| `{{ package }}`                  | owning package id for the rendered entry                              | useful in shared templates                                                                                 |
| `{{ version }}`                  | release version for the current target                                | package or group version                                                                                   |
| `{{ target_id }}`                | release target id                                                     | package id or group id                                                                                     |
| `{{ bump }}`                     | resolved bump severity                                                | `patch`, `minor`, or `major`                                                                               |
| `{{ type }}`                     | changeset note type                                                   | e.g. `feature`, `fix`, `security`; omitted when absent                                                     |
| `{{ context }}`                  | compact default metadata block                                        | preferred rendered block for human-readable notes                                                          |
| `{{ provenance }}`               | legacy alias for `{{ context }}`                                      | kept for backward compatibility                                                                            |
| `{{ changeset_path }}`           | source `.changeset/*.md` path                                         | tracked in manifests and still available for custom templates, but not shown by default in `{{ context }}` |
| `{{ change_owner }}`             | plain-text hosted actor label                                         | usually something like `@ifiokjr`                                                                          |
| `{{ change_owner_link }}`        | markdown link to the hosted actor                                     | falls back to plain text when no URL is available                                                          |
| `{{ review_request }}`           | plain-text PR/MR label                                                | e.g. `PR #31` or `MR !42`                                                                                  |
| `{{ review_request_link }}`      | markdown link to the PR/MR                                            | falls back to plain text when no URL is available                                                          |
| `{{ introduced_commit }}`        | short SHA for the commit that first introduced the changeset          | plain text only                                                                                            |
| `{{ introduced_commit_link }}`   | markdown link to the introducing commit                               | preferred for changelog output                                                                             |
| `{{ last_updated_commit }}`      | short SHA for the most recent commit that changed the changeset       | only populated when different from `{{ introduced_commit }}`                                               |
| `{{ last_updated_commit_link }}` | markdown link to the most recent commit that changed the changeset    | only populated when different from `{{ introduced_commit }}`                                               |
| `{{ closed_issues }}`            | plain-text list of issues closed by the linked review request         | typically `#12, #18`                                                                                       |
| `{{ closed_issue_links }}`       | markdown links to issues closed by the linked review request          | preferred for changelog output                                                                             |
| `{{ related_issues }}`           | plain-text list of related issues that were referenced but not closed | host support may vary                                                                                      |
| `{{ related_issue_links }}`      | markdown links to related issues that were referenced but not closed  | host support may vary                                                                                      |

The `*_link` variants render markdown links when the hosting provider exposes URLs. By default `{{ context }}` renders the highest-value metadata for readers — owner, review request, introduced commit, last updated commit when different, and linked issues — without exposing the transient `.changeset/*.md` path unless you explicitly reference `{{ changeset_path }}` in your template.

<!-- {/configurationPackageOverridesSnippet} -->

<!-- {@configurationWorkflowsSnippet} -->

```toml
[release_notes]
change_templates = [
	"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ details }}",
	"- {{ summary }}",
]

[package.core]
path = "crates/core"
extra_changelog_sections = [{ name = "Security", types = ["security"] }]

[cli.discover]
help_text = "Discover packages across supported ecosystems"

[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.discover.steps]]
type = "Discover"

[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"

[cli.release-manifest]
help_text = "Prepare a release and write a stable JSON manifest"

[[cli.release-manifest.steps]]
type = "PrepareRelease"

[[cli.release-manifest.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"

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

name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

type = "PrepareRelease"

type = "Command"
command = "cargo test --workspace --all-features"
dry_run_command = "cargo test --workspace --all-features"
shell = true

[cli.verify]
help_text = "Evaluate pull-request changeset policy"

[[cli.verify.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.verify.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.verify.inputs]]
name = "label"
type = "string_list"

[[cli.verify.steps]]
type = "VerifyChangesets"
```

<!-- {/configurationWorkflowsSnippet} -->

<!-- {@configurationWorkflowVariables} -->

- default command substitution when `variables` is omitted: `{{ version }}`, `$group_version`, `$released_packages`, `$changed_files`, and `$changesets`
- custom command substitution when `variables` is present: map your own replacement strings to variable names such as `version`, `group_version`, `released_packages`, `changed_files`, and `changesets`
- `dry_run_command` on a `Command` step replaces `command` only when the CLI command is run with `--dry-run`
- `shell = true` runs the command through the current shell; the default mode runs the executable directly after shell-style splitting

<!-- {/configurationWorkflowVariables} -->

<!-- {@configurationGitHubSnippet} -->

```toml
[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[source.releases]
enabled = true
draft = false
prerelease = false
source = "monochange"

[source.pull_requests]
enabled = true
branch_prefix = "monochange/release"
base = "main"
title = "chore(release): prepare release"
labels = ["release", "automated"]
auto_merge = false

[source.bot.changesets]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["crates/**", "packages/**", "npm/**", "skills/**"]
ignored_paths = [
	"docs/**",
	"specs/**",
	"readme.md",
	"CONTRIBUTING.md",
	"license",
]

name = "production"
trigger = "release_pr_merge"
release_targets = ["sdk"]
requires = ["main"]
```

<!-- {/configurationGitHubSnippet} -->

<!-- {@configurationEcosystemSettingsSnippet} -->

```toml
[ecosystems.cargo]
enabled = true
roots = ["crates/*"]
exclude = ["crates/experimental/*"]

[ecosystems.npm]
enabled = true
roots = ["packages/*"]
exclude = ["packages/legacy/*"]

[ecosystems.deno]
enabled = true

[ecosystems.dart]
enabled = true
```

<!-- {/configurationEcosystemSettingsSnippet} -->

<!-- {@configurationPackageReferenceRules} -->

Package references in changesets and CLI commands should use configured package ids or group ids. Legacy manifest-relative paths and directory paths may still appear in older repos during migration, but `mc validate` should guide you toward declared ids.

<!-- {/configurationPackageReferenceRules} -->

<!-- {@configurationCurrentStatus} -->

Current implementation notes:

- `defaults.include_private` is parsed, but discovery behavior is still centered on the supported fixture-driven CLI commands in this milestone
- `version_groups.strategy` belongs to the legacy model and should be migrated to `[group.<id>]`
- legacy `[[workflows]]` configuration is no longer supported; use `[cli.<command>]` plus `[[cli.<command>.steps]]` instead
- `[ecosystems.*].enabled/roots/exclude` are parsed, but discovery still scans all supported ecosystems regardless of those settings today
- `package_overrides.changelog` is a legacy setting that should be migrated to package declarations
- source automation expects `[source]` with provider-specific settings under `[source.releases]`, `[source.pull_requests]`, and `[source.bot.changesets]`; GitHub remains the default provider
- live GitHub release and release-request publishing uses `octocrab` with `GITHUB_TOKEN` / `GH_TOKEN`; GitLab and Gitea use direct HTTP APIs
- release-request publishing still uses local `git` for branch, commit, and push operations before provider API updates when not in dry-run mode
- changeset policy commands currently apply only to the GitHub provider and expect `[source.bot.changesets]`, a `changed_paths` command input, and reusable diagnostics for GitHub Actions consumption
- supported command steps today are `Validate`, `Discover`, `CreateChangeFile`, `PrepareRelease`, `RenderReleaseManifest`, `PublishRelease`, `OpenReleaseRequest`, `CommentReleasedIssues`, `VerifyChangesets`, and `Command`
- legacy `PublishGitHubRelease`, `OpenReleasePullRequest`, and `EnforceChangesetPolicy` step names are still accepted as migration aliases

<!-- {/configurationCurrentStatus} -->

<!-- {@versionGroupsExample} -->

```toml
[package.sdk-core]
path = "cargo/sdk-core"
type = "cargo"

[package.web-sdk]
path = "packages/web-sdk"
type = "npm"

[group.sdk]
packages = ["sdk-core", "web-sdk"]
tag = true
release = true
version_format = "primary"
```

<!-- {/versionGroupsExample} -->

<!-- {@versionGroupsBehavior} -->

- the highest required bump in the group wins
- every member in the group receives that bump
- one planned group version is calculated from the highest current member version
- the group owns outward release identity
- member package changelogs can still be updated individually
- group changelog and group `versioned_files` can also be updated
- grouped packages can use `empty_update_message` when their own changelog needs a version-only update with no direct notes
- dependents of newly synced members still receive propagated parent bumps
- unmatched members produce warnings during discovery
- mismatched current versions produce warnings when `warn_on_group_mismatch = true`

<!-- {/versionGroupsBehavior} -->

<!-- {@versionGroupsCurrentStatus} -->

Legacy `version_groups.strategy` is no longer the primary authoring model. The current implementation always derives synchronized release behavior from `[group.<id>]` declarations.

<!-- {/versionGroupsCurrentStatus} -->

<!-- {@releaseChangesAddCommand} -->

```bash
mc change --package sdk-core --bump minor --reason "public API addition"
mc change --package sdk-core --bump patch --type security --reason "rotate signing keys" --details "Roll the signing key before the release window closes."
mc change --package sdk-core --bump major --reason "break the public API" --evidence rust-semver:major:public API break detected --output .changeset/sdk-core-major.md
```

<!-- {/releaseChangesAddCommand} -->

<!-- {@releaseManualChangesetExample} -->

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

<!-- {@releaseEvidenceExample} -->

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

<!-- {@releasePlanningRules} -->

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
- `VerifyChangesets` evaluates changed paths, skip labels, and changed `.changeset/*.md` files into reusable pass/skip/fail diagnostics and optional failure comments
- CLI text and JSON output render workspace paths relative to the repository root for stable snapshots and automation

<!-- {/releasePlanningRules} -->

<!-- {@releaseWorkflowBehavior} -->

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
- can evaluate pull-request changeset policy via `VerifyChangesets` using changed paths and labels supplied by CI
- applies group-owned release identity for outward `tag`, `release`, and `version_format`
- deletes consumed change files only after a successful non-dry-run execution
- leaves the workspace untouched during `--dry-run` except for explicitly requested outputs such as a rendered release manifest or release preview

A GitHub Actions check can pass changed paths and labels directly into a policy workflow, for example:

<!-- {/releaseWorkflowBehavior} -->

<!-- {@changesetPolicyGitHubActionWorkflow} -->

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

<!-- {@githubAutomationOverview} -->

MonoChange keeps source-provider automation layered on top of the same `PrepareRelease` result used for normal release planning.

That means one set of `.changeset/*.md` inputs can drive all of these commands and automation flows consistently:

- `mc release-manifest` writes a stable JSON artifact for downstream automation
- `mc publish-release` previews or publishes provider releases from the structured release notes
- `mc release-pr` previews or opens an idempotent provider release request
- `mc verify` evaluates pull-request changeset policy from CI-supplied changed paths and labels

<!-- {/githubAutomationOverview} -->

<!-- {@githubAutomationWorkflowCommands} -->

```bash
mc release --dry-run --format json
mc release-manifest --dry-run
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
mc verify --format json --changed-paths crates/monochange/src/lib.rs
```

<!-- {/githubAutomationWorkflowCommands} -->

<!-- {@githubAutomationReleaseConfigExample} -->

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

[cli.release-manifest]
help_text = "Prepare a release and write a stable JSON manifest"

[[cli.release-manifest.steps]]
type = "PrepareRelease"

[[cli.release-manifest.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"

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

[cli.verify]
help_text = "Evaluate pull-request changeset policy"

[[cli.verify.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.verify.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.verify.inputs]]
name = "label"
type = "string_list"

[[cli.verify.steps]]
type = "VerifyChangesets"
```

<!-- {@githubAutomationDogfoodNotes} -->

The MonoChange repository itself can dogfood this model by:

- declaring `[github]`, `[github.releases]`, and `[github.pull_requests]` in `monochange.toml`
- running a real `changeset-policy` GitHub Actions workflow that shells into `mc verify`

<!-- {/githubAutomationDogfoodNotes} -->
