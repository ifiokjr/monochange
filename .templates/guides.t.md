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
- gitignored paths and nested git worktrees are skipped during discovery
- version-group assignments are attached after discovery
- unmatched group members (declared in config but not found during discovery) produce warnings
- unresolvable group members (invalid package IDs in `group.packages`) produce errors during configuration loading
- discovery currently scans all supported ecosystems regardless of `[ecosystems.*]` toggles in `monochange.toml`

<!-- {/discoveryKeyBehaviors} -->

<!-- {@initProviderFeature} -->

The `--provider` flag supports `github`, `gitlab`, and `gitea`. When provided, `mc init`:

1. **Configures the `[source]` section** — adds provider-specific settings for releases and pull/merge requests
2. **Generates provider CLI commands** — includes `commit-release` and `release-pr` commands in `monochange.toml`
3. **Creates workflow files** (GitHub only) — writes `.github/workflows/release.yml` and `.github/workflows/changeset-policy.yml`
4. **Auto-detects owner/repo** — parses `git remote get-url origin` to pre-populate `[source]`

Example generated configuration with `--provider github`:

```toml
[source]
provider = "github"
owner = "ifiokjr" # auto-detected from git remote
repo = "monochange" # auto-detected from git remote

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

[cli.commit-release]
help_text = "Prepare a release and create a release commit"

[[cli.commit-release.steps]]
type = "PrepareRelease"
name = "plan release"

[[cli.commit-release.steps]]
type = "CommitRelease"
name = "create release commit"

[cli.release-pr]
help_text = "Prepare a release and open a release pull request"

[[cli.release-pr.steps]]
type = "PrepareRelease"
name = "plan release"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
name = "open release PR"
```

The GitHub Actions workflows enable:

- **Release automation** — `release.yml` builds binaries and creates GitHub releases from tags
- **Changeset policy enforcement** — `changeset-policy.yml` validates PRs have required changeset coverage

For GitLab and Gitea, the `[source]` section is configured but workflows are not generated (use their respective CI configuration files).

<!-- {/initProviderFeature} -->

<!-- {@initProviderQuickStart} -->

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

<!-- {@configurationDefaultsSnippet} -->

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true
strict_version_conflicts = false
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
versioned_files = [
	"Cargo.toml",
	{ path = "crates/sdk_core/extra.toml", type = "cargo" },
]
tag = false
release = false
version_format = "namespaced"

[package.sdk-core.changelog]
path = "crates/sdk_core/CHANGELOG.md"
format = "monochange"
```

<!-- {/configurationVersionGroupsSnippet} -->

<!-- {@configurationRegexVersionedFilesSnippet} -->

Regex entries let you version-stamp any plain-text file — README badges, download links, install scripts — without needing an ecosystem-specific parser. The regex must contain a named `version` capture group; monochange replaces the captured substring with the new version while preserving the surrounding text.

```toml
[package.core]
path = "crates/core"
versioned_files = [
	# update a download link in the README
	{ path = "README.md", regex = 'https://example\.com/download/v(?<version>\d+\.\d+\.\d+)\.tgz' },
	# update a version badge
	{ path = "README.md", regex = 'img\.shields\.io/badge/version-(?<version>\d+\.\d+\.\d+)-blue' },
]

[group.sdk]
packages = ["core", "cli"]
versioned_files = [
	# update the install script across all packages (glob pattern)
	{ path = "**/install.sh", regex = 'SDK_VERSION="(?<version>\d+\.\d+\.\d+)"' },
]

[ecosystems.cargo]
versioned_files = [
	# update a workspace-wide version constant
	{ path = "crates/constants/src/lib.rs", regex = 'pub const VERSION: &str = "(?<version>\d+\.\d+\.\d+)"' },
]
```

Key rules:

- `regex` entries cannot set `type`, `prefix`, `fields`, or `name` — they operate on raw text
- the regex must include a `(?<version>...)` named capture group
- the `path` field supports glob patterns (e.g. `**/README.md`)
- regex entries work on packages, groups, and ecosystem-level `versioned_files`

<!-- {/configurationRegexVersionedFilesSnippet} -->

<!-- {@configurationPackageOverridesSnippet} -->

When `[defaults].package_type` is set, package entries may omit an explicit `type`.

monochange currently supports two changelog formats:

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
| `{{ bump }}`                     | resolved bump severity                                                | `none`, `patch`, `minor`, or `major`                                                                       |
| `{{ type }}`                     | changeset note type                                                   | e.g. `feature`, `fix`, `security`; omitted when absent                                                     |
| `{{ context }}`                  | compact default metadata block                                        | preferred rendered block for human-readable notes                                                          |
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
extra_changelog_sections = [
	{ name = "Security", types = ["security"], default_bump = "patch" },
]

[cli.discover]
help_text = "Discover packages across supported ecosystems"

[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.discover.steps]]
name = "discover packages"
type = "Discover"

[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
name = "prepare release"
type = "PrepareRelease"

[cli.publish-release]
help_text = "Prepare a release and publish provider releases"

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
name = "prepare release"
type = "PrepareRelease"

[[cli.publish-release.steps]]
name = "publish release"
type = "PublishRelease"

[[cli.publish-release.steps]]
name = "comment released issues"
type = "CommentReleasedIssues"

[cli.release-pr]
help_text = "Prepare a release and open or update a provider release request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-pr.steps]]
name = "prepare release"
type = "PrepareRelease"

[[cli.release-pr.steps]]
name = "open release request"
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
name = "evaluate affected packages"
type = "AffectedPackages"
```

<!-- {/configurationWorkflowsSnippet} -->

<!-- {@configurationWorkflowVariables} -->

- built-in command variables are available directly as `{{ version }}`, `{{ group_version }}`, `{{ released_packages }}`, `{{ changed_files }}`, and `{{ changesets }}`
- command templates can read CLI inputs through `{{ inputs.name }}`
- every step can override the inputs it receives with `inputs = { ... }`; direct references like `"{{ inputs.labels }}"` preserve list and boolean values when rebinding to built-in steps
- built-in commands already attach descriptive step `name` labels such as `prepare release` and `publish release`; keep or replace those labels when you want progress output to stay readable
- custom command variables become available when `variables` is present: map your own names to variables such as `version`, `group_version`, `released_packages`, `changed_files`, and `changesets`
- `dry_run_command` on a `Command` step replaces `command` only when the CLI command is run with `--dry-run`
- `shell = true` runs the command through the current shell; the default mode runs the executable directly after shell-style splitting

<!-- {/configurationWorkflowVariables} -->

<!-- {@configurationGitHubSnippet} -->

The `[source]` section configures provider integration for releases, pull requests, and changeset enforcement.

For self-hosted instances, set `api_url` or `host` to your server's URL. These fields **must** use `https://`; insecure `http://` schemes are rejected because API tokens would be transmitted in cleartext.

```toml
[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"
# api_url = "https://github.company.com/api/v3"  # optional: for GitHub Enterprise

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
lockfile_commands = [{ command = "cargo generate-lockfile" }]

[ecosystems.npm]
enabled = true
roots = ["packages/*"]
exclude = ["packages/legacy/*"]
dependency_version_prefix = "^"
versioned_files = ["**/packages/*/package.json"]
lockfile_commands = [
	{ command = "pnpm install --lockfile-only", cwd = "packages/web" },
]

[ecosystems.deno]
enabled = true
# Deno currently has no inferred lockfile command.

[ecosystems.dart]
enabled = true
lockfile_commands = [{ command = "flutter pub get", cwd = "packages/mobile" }]
```

<!-- {/configurationEcosystemSettingsSnippet} -->

<!-- {@configurationPackageReferenceRules} -->

Package references in changesets and CLI commands should use configured ids.

Prefer package ids when a leaf package changed. That keeps the authored change as specific as possible, and monochange will still propagate bumps to dependents and synchronize any configured groups automatically.

Use a group id only when the change is intentionally owned by the whole group and should read that way in release output.

<!-- {/configurationPackageReferenceRules} -->

<!-- {@configurationCurrentStatus} -->

Current implementation notes:

- `defaults.include_private` is parsed, but discovery behavior is still centered on the supported fixture-driven CLI commands documented here
- `[ecosystems.*].enabled/roots/exclude` are parsed, but discovery still scans all supported ecosystems regardless of those settings today
- `defaults.strict_version_conflicts` controls whether conflicting explicit `version` entries across changesets warn-and-pick-highest (default) or fail planning outright
- source automation expects `[source]` with provider-specific settings under `[source.releases]`, `[source.pull_requests]`, and `[source.bot.changesets]`; GitHub remains the default provider
- live GitHub release and release-request publishing uses `octocrab` with `GITHUB_TOKEN` / `GH_TOKEN`; GitLab and Gitea use direct HTTP APIs
- release-request publishing still uses local `git` for branch, commit, and push operations before provider API updates when not in dry-run mode
- changeset policy commands currently apply only to the GitHub provider and expect `[source.bot.changesets]`, a `changed_paths` command input, and reusable diagnostics for GitHub Actions consumption
- supported command steps today are `Validate`, `Discover`, `CreateChangeFile`, `PrepareRelease`, `CommitRelease`, `PublishRelease`, `OpenReleaseRequest`, `CommentReleasedIssues`, `AffectedPackages`, `DiagnoseChangesets`, `RetargetRelease`, and `Command`
- see the [CLI step reference](../reference/cli-steps/00-index.md) for detailed per-step guidance, prerequisites, and composition examples

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
- unmatched members (not found during discovery) produce warnings; unresolvable members (invalid IDs) produce errors
- mismatched current versions produce warnings when `warn_on_group_mismatch = true`

<!-- {/versionGroupsBehavior} -->

<!-- {@releaseChangesAddCommand} -->

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

<!-- {@releaseManualChangesetExample} -->

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

<!-- {@releaseExplicitVersionChangesetExample} -->

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

<!-- {@releasePlanningRules} -->

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

<!-- {@releaseWorkflowBehavior} -->

`mc release` is part of monochange's built-in default command set. The defaults include: `validate`, `discover`, `change`, `release`, `affected`, `diagnostics`, and `repair-release`. You only need to add `[cli.release]` when you want to replace that default definition with your own steps, inputs, or help text.

Commands like `commit-release` (which combines `PrepareRelease` + `CommitRelease` steps) are not included in the defaults — define them explicitly in your `monochange.toml` when you need them.

Current `PrepareRelease` behavior:

- reads `.changeset/*.md`
- computes one synchronized release plan from discovered change files
- updates native manifests plus configured changelogs and versioned files
- renders changelog files through structured release notes using the configured `monochange` or `keep_a_changelog` format
- groups release notes into default `Breaking changes`, `Features`, `Fixes`, and `Notes` sections, with package/group overrides available through `extra_changelog_sections`
- applies workspace-wide release-note templates from `[release_notes].change_templates`
- refreshes the cached `.monochange/release-manifest.json` artifact during `PrepareRelease` for downstream automation
- can preview or publish provider releases via `PublishRelease`
- can preview or open/update release requests via `OpenReleaseRequest`
- can comment on released issues via `CommentReleasedIssues`
- can evaluate pull-request changeset policy via `AffectedPackages` using changed paths and labels supplied by CI
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

monochange keeps source-provider automation layered on top of the same `PrepareRelease` result used for normal release planning.

That means one set of `.changeset/*.md` inputs can drive all of these commands and automation flows consistently:

- `mc release --dry-run --format json` refreshes the cached manifest and shows the downstream automation payload
- `mc publish-release` previews or publishes provider releases from the structured release notes
- `mc release-pr` previews or opens an idempotent provider release request
- `mc affected` evaluates pull-request changeset policy from CI-supplied changed paths and labels

<!-- {/githubAutomationOverview} -->

<!-- {@githubAutomationWorkflowCommands} -->

```bash
mc release --dry-run --format json
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
mc affected --format json --changed-paths crates/monochange/src/lib.rs
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

<!-- {@githubAutomationDogfoodNotes} -->

The monochange repository itself can dogfood this model by:

- declaring `[source]`, `[source.releases]`, and `[source.pull_requests]` in `monochange.toml`
- running a real `changeset-policy` GitHub Actions workflow that shells into `mc affected`

<!-- {/githubAutomationDogfoodNotes} -->

<!-- {@assistantSkillBundleContents} -->

After copying the bundled skill, you get a small documentation set that is designed to load in layers:

- `SKILL.md` — concise entrypoint for agents
- `REFERENCE.md` — broader high-context reference with more examples
- `skills/README.md` — index of focused deep dives
- `skills/changesets.md` — changeset authoring and lifecycle guidance
- `skills/commands.md` — built-in command catalog and workflow selection
- `skills/configuration.md` — `monochange.toml` setup and editing guidance
- `skills/linting.md` — the current lint policy, rationale, and examples

This layout keeps the top-level skill small while still making the richer guidance available when an assistant needs more context.

<!-- {/assistantSkillBundleContents} -->

<!-- {@mcpToolsList} -->

- `monochange_validate` — validate `monochange.toml` and `.changeset` targets
- `monochange_discover` — discover packages, dependencies, and groups across the repository
- `monochange_change` — write a `.changeset` markdown file for one or more package or group ids
- `monochange_release_preview` — prepare a dry-run release preview from discovered `.changeset` files
- `monochange_release_manifest` — generate a dry-run release manifest JSON document for downstream automation
- `monochange_affected_packages` — evaluate changeset policy from changed paths and optional labels

<!-- {/mcpToolsList} -->

<!-- {@mcpConfigSnippet} -->

```json
{
	"mcpServers": {
		"monochange": {
			"command": "monochange",
			"args": ["mcp"]
		}
	}
}
```

<!-- {/mcpConfigSnippet} -->

<!-- {@recommendedCommandFlow} -->

1. **Validate** — `mc validate` checks config and changeset targets.
2. **Discover** — `mc discover --format json` inspects the workspace model.
3. **Create changesets** — `mc change --package <id> --bump <severity> --reason "..."` writes explicit release intent.
4. **Preview release** — `mc release --dry-run --format json` shows planned bumps, changelog output, and changed files.
5. **Inspect changeset context** — `mc diagnostics --format json` shows git provenance and linked review metadata for all pending changesets.
6. **Inspect cached manifest** — `mc release --dry-run --format json` refreshes the cached manifest and shows the downstream automation payload.
7. **Publish** — `mc publish-release --format json` creates provider releases after human review.

<!-- {/recommendedCommandFlow} -->

<!-- {@assistantRepoGuidance} -->

- Read `monochange.toml` before proposing release workflow changes.
- Run `mc validate` before and after release-affecting edits.
- Use `mc discover --format json` to inspect package ids, group ownership, and dependency edges.
- Use `mc diagnostics --format json` for a structured view of all pending changesets with git and review context.
- Prefer `mc change` plus `.changeset/*.md` files over ad hoc release notes.
- Use `mc release --dry-run --format json` before mutating release state.

<!-- {/assistantRepoGuidance} -->

<!-- {@cliStepTypes} -->

**Standalone steps** (no prerequisites):

- `Validate` — validate config and changeset targets
- `Discover` — discover packages across ecosystems
- `CreateChangeFile` — write a `.changeset/*.md` file
- `AffectedPackages` — evaluate changeset policy from CI-supplied paths and labels
- `DiagnoseChangesets` — show changeset context and review metadata
- `RetargetRelease` — repair a recent release by moving its tags

**Release-state consumer steps** (require `PrepareRelease`):

- `PrepareRelease` — compute release plan, update versions, changelogs, and versioned files
- `CommitRelease` — create a local release commit
- `PublishRelease` — create provider releases
- `OpenReleaseRequest` — open or update a release pull request
- `CommentReleasedIssues` — comment on issues referenced in changesets

**Generic step:**

- `Command` — run an arbitrary shell command with template interpolation

<!-- {/cliStepTypes} -->

<!-- {@releaseTitleConfig} -->

Two template fields control how release names and changelog version headings render:

- **`release_title`** — plain text title for provider releases (GitHub, GitLab, Gitea)
- **`changelog_version_title`** — markdown-capable title for changelog version headings

Both are configurable at `[defaults]`, `[package.*]`, and `[group.*]` levels.

Available template variables: `{{ version }}`, `{{ id }}`, `{{ date }}`, `{{ time }}`, `{{ datetime }}`, `{{ changes_count }}`, `{{ tag_url }}`, `{{ compare_url }}`.

```toml
[defaults]
release_title = "{{ version }} ({{ date }})"
changelog_version_title = "[{{ version }}]({{ tag_url }}) ({{ date }})"

[group.main]
release_title = "v{{ version }} — released {{ date }}"
```

<!-- {/releaseTitleConfig} -->

<!-- {@lintingPolicyReference} -->

Use this guide when the task is to explain, apply, or update monochange lint policy.

This reference reflects the current workspace lint configuration in the repository `Cargo.toml` plus the crate-level `#![forbid(clippy::indexing_slicing)]` declarations used across the Rust crates.

## Daily linting workflow

For normal repo work:

```bash
devenv shell fix:all
devenv shell lint:all
```

For documentation synchronization checks:

```bash
devenv shell docs:check
```

Use `docs:update` after editing shared `.templates/` content.

## How to read the policy

monochange uses a mix of:

- **workspace rust lints** — compiler-level safety and hygiene rules
- **workspace clippy groups** — broad quality buckets like correctness and performance
- **targeted clippy overrides** — rules we intentionally deny, warn, or allow
- **crate-level forbids** — stricter local rules when a specific panic pattern is unacceptable

The goal is not "never write interesting code." The goal is to avoid correctness bugs, avoid panic-prone indexing patterns, stay portable across Rust editions, and keep only the pedantic warnings that actually improve the codebase.

## Workspace rust lints

### `rust_2021_compatibility = warn`

**Why:** catches patterns that behave differently across editions.

**When to care:** when writing macros, pattern matches, or syntax that might become edition-sensitive.

**Without the rule:** edition migration issues can accumulate silently.

**With the rule:** you get an early warning before the next edition change becomes painful.

### `rust_2024_compatibility = warn`

**Why:** keeps the codebase ready for Rust 2024 semantics.

**When to care:** when introducing syntax or macro usage that may change in the 2024 edition.

**Without the rule:** future upgrades become a large cleanup project.

**With the rule:** new code is nudged toward edition-safe patterns now.

### `unsafe_code = deny`

**Why:** monochange should not rely on unchecked memory operations for release-planning logic.

**When to use:** almost always for business logic, config parsing, changelog generation, and CLI orchestration.

**Without the rule:**

```rust
unsafe {
    std::ptr::read(ptr)
}
```

Unsafe blocks can slip in and become maintenance hotspots.

**With the rule:** prefer safe standard-library APIs:

```rust
let first = values.first().copied();
```

### `unstable_features = deny`

**Why:** published tooling should compile on stable Rust.

**When to use:** for libraries and CLIs that must stay portable for contributors and CI.

**Without the rule:** nightly-only features can leak into the codebase.

**With the rule:** the code stays stable-channel compatible.

### `unused_extern_crates = warn`

**Why:** dead extern declarations add noise and make dependencies harder to audit.

**Without the rule:** old compatibility imports linger.

**With the rule:** unused declarations are cleaned up quickly.

### `unused_import_braces = warn`

**Why:** removes unnecessary syntax noise.

**Without the rule:**

```rust
use std::fmt;
```

**With the rule:**

```rust
use std::fmt;
```

### `unused_lifetimes = warn`

**Why:** unused lifetimes usually mean an API signature is more complex than necessary.

**Without the rule:**

```rust
fn name<'a>(value: &str) -> &str {
	value
}
```

**With the rule:**

```rust
fn name(value: &str) -> &str {
	value
}
```

### `unused_macro_rules = warn`

**Why:** dead macro arms are easy to forget and hard to test.

**Without the rule:** macro definitions keep stale branches.

**With the rule:** only exercised macro rules survive.

### `unused_qualifications = warn`

**Why:** fully qualifying names that are already in scope hurts readability.

**Without the rule:**

```rust
use std::path::PathBuf;

let path = std::path::PathBuf::new();
```

**With the rule:**

```rust
use std::path::PathBuf;

let path = PathBuf::new();
```

### `variant_size_differences = warn`

**Why:** very uneven enum variants can cause surprising memory bloat.

**Without the rule:** a single large variant can inflate every enum value.

**With the rule:** you consider boxing or reshaping the large variant.

### `edition_2024_expr_fragment_specifier = allow`

**Why it is allowed:** this lint is intentionally relaxed to avoid noisy churn while macro-related edition support settles.

**When the allowance is appropriate:** when the code is otherwise clear and no migration risk is introduced.

**Without the allowance:** the repo would force extra macro cleanups that do not improve the current product behavior.

## Workspace clippy groups

### `clippy::correctness = deny`

**Why:** correctness issues are the highest-risk category.

**Without it:** real bugs can land as "just warnings."

**With it:** code that is likely wrong fails the lint pass.

### `clippy::suspicious = warn`

**Why:** suspicious constructs often compile but suggest a logic mistake.

**Without it:** subtle mistakes look legitimate.

**With it:** you get a review checkpoint before the bug becomes user-visible.

### `clippy::style = warn`

**Why:** style warnings keep code predictable and easier to scan.

**Without it:** equivalent patterns drift across the codebase.

**With it:** contributors converge on the same idioms.

### `clippy::complexity = warn`

**Why:** overly complex code is harder to review and easier to break.

**Without it:** nested or overly clever logic grows unnoticed.

**With it:** clippy nudges you toward extraction and simpler control flow.

**Example of what this pressure is trying to prevent:**

```rust
if should_release {
    if let Some(group) = group {
        if group.publish {
            if !group.members.is_empty() {
                publish(group);
            }
        }
    }
}
```

A flatter version is easier to review:

```rust
let Some(group) = group else {
    return;
};

if !should_release || !group.publish || group.members.is_empty() {
    return;
}

publish(group);
```

### `clippy::perf = warn`

**Why:** hot-path inefficiencies are easier to fix when caught early.

**Without it:** unnecessary allocations and slower patterns blend in.

**With it:** common performance footguns get surfaced during normal linting.

### `clippy::pedantic = warn`

**Why:** pedantic lints catch a lot of polish issues that improve API and code quality.

**Why not deny:** the group is intentionally broad and sometimes noisy.

**monochange approach:** enable the group, then explicitly allow the few rules where local readability or practicality matters more.

## Explicit clippy policy

### `blocks_in_conditions = allow`

**Why it is allowed:** small computed conditions can be clearer inline than as a throwaway binding.

**Without the allowance:**

```rust
if {
    let ready = state.is_ready();
    ready
} {
    run();
}
```

Clippy would complain even when the structure is readable.

**With the current policy:** this pattern is allowed, but extract it if the block becomes non-trivial.

### `cargo_common_metadata = allow`

**Why it is allowed:** workspace metadata is managed centrally, so per-crate metadata completeness is not always the right enforcement point.

**Without the allowance:** clippy would push repetitive metadata into every crate even when the workspace already provides it.

**With the current policy:** add metadata where it matters, but do not create boilerplate just to silence the lint.

### `cast_possible_truncation = allow`

**Why it is allowed:** some numeric conversions are deliberate and guarded by domain knowledge.

**Without the allowance:**

```rust
let byte = value as u8;
```

would warn every time, even when the value is known to fit.

**With the current policy:** the cast is permitted, but reviewers should still expect surrounding reasoning or bounds checks when truncation is not obviously safe.

### `cast_possible_wrap = allow`

**Why it is allowed:** signed/unsigned conversions sometimes reflect external protocol or storage requirements.

**Without the allowance:** every deliberate sign-changing cast becomes noise.

**With the current policy:** use the cast intentionally and document tricky cases.

### `cast_precision_loss = allow`

**Why it is allowed:** some reporting or ratio calculations intentionally trade precision for convenience.

**Without the allowance:** floating-point conversions generate warnings even in non-critical display logic.

**With the current policy:** precision-loss casts are allowed, but avoid them in semver, version, or identity logic where exactness matters.

### `cast_sign_loss = allow`

**Why it is allowed:** conversions to unsigned values are sometimes part of external API shaping.

**Without the allowance:** routine boundary conversions become noisy.

**With the current policy:** keep the cast local and obvious.

### `expl_impl_clone_on_copy = allow`

**Why it is allowed:** an explicit `Clone` impl on a `Copy` type can occasionally be clearer or more controlled than a derive.

**Without the allowance:** clippy would force a derive-only style.

**With the current policy:** explicit impls are allowed when there is a concrete reason, not as default habit.

### `items_after_statements = allow`

**Why it is allowed:** tests and small helper scopes sometimes read better when local items appear near their use.

**Without the allowance:**

```rust
fn test_case() {
	let input = sample();

	fn sample() -> &'static str {
		"ok"
	}

	assert_eq!(input, "ok");
}
```

would warn.

**With the current policy:** that layout is acceptable when it improves locality.

### `missing_errors_doc = allow`

**Why it is allowed:** internal functions are numerous, and forcing `# Errors` on all of them creates noisy docs.

**Without the allowance:** every fallible helper would need a doc section.

**With the current policy:** still document errors on public APIs and non-obvious behavior, but do not require boilerplate on every internal helper.

### `missing_panics_doc = allow`

**Why it is allowed:** similar to `missing_errors_doc`, this avoids boilerplate on internal helpers.

**Without the allowance:** even intentionally internal panic paths need formal docs.

**With the current policy:** public or surprising panic behavior should still be documented deliberately.

### `module_name_repetitions = allow`

**Why it is allowed:** crate boundaries and domain naming sometimes make repetition the clearest choice.

**Without the allowance:**

```rust
mod release_record;
struct ReleaseRecord;
```

can trigger a warning even though the names are clear.

**With the current policy:** choose clarity over lint golf.

### `must_use_candidate = allow`

**Why it is allowed:** clippy suggests `#[must_use]` very aggressively.

**Without the allowance:** many private helpers would get noisy suggestions.

**With the current policy:** apply `#[must_use]` intentionally on public APIs, builders, and values where dropping the result is genuinely a bug.

### `no_effect_underscore_binding = allow`

**Why it is allowed:** intentionally ignored intermediate values sometimes help document intent in tests or command setup.

**Without the allowance:** underscore-prefixed bindings can still warn even when they make the code easier to follow.

**With the current policy:** use them sparingly and only when they clarify intent.

### `tabs-in-doc-comments = allow`

**Why it is allowed:** command output, tables, or copied terminal content may legitimately contain tabs.

**Without the allowance:** documentation cleanup would fight preserved examples.

**With the current policy:** tabs are acceptable in docs when they preserve exact formatting.

### `too_many_lines = allow`

**Why it is allowed:** some orchestration functions, renderers, or test modules are large for domain reasons.

**Without the allowance:** contributors would spend time splitting code purely to satisfy an arbitrary line limit.

**With the current policy:** long functions are allowed, but extraction is still preferred when it improves comprehension.

### `wildcard_dependencies = deny`

**Why:** published tools should not depend on unconstrained crate versions.

**Without the rule:**

```toml
serde = "*"
```

can make builds non-reproducible and difficult to audit.

**With the rule:** dependencies must be explicitly versioned.

### `wildcard_imports = allow`

**Why it is allowed:** some test modules and highly local scopes read better with a wildcard import.

**Without the allowance:**

```rust
use super::*;
```

would warn in common test layouts.

**With the current policy:** wildcard imports remain acceptable in narrow scopes, especially tests.

## Crate-level forbid: `clippy::indexing_slicing`

Most monochange Rust crates start with:

```rust
#![forbid(clippy::indexing_slicing)]
```

**Why:** indexing and slicing can panic, and monochange spends a lot of time parsing external files, manifests, and user input.

**Without the rule:**

```rust
let first = values[0];
let suffix = &text[1..];
```

These compile, but they panic on malformed or short input.

**With the rule:** prefer checked access:

```rust
let first = values.first().copied();
let suffix = text.get(1..);
```

**Another example in manifest parsing:**

```rust
let version = package_json["version"].as_str();
```

looks compact but assumes the key exists and the JSON shape is right. Checked access makes the failure mode explicit:

```rust
let version = package_json
    .get("version")
    .and_then(|value| value.as_str());
```

**When to use this stricter rule:** parsing, config loading, release planning, changelog rendering, and any code that handles external repository state.

## What "with and without linting" looks like in practice

### Without the monochange lint posture

- unsafe or nightly-only code can sneak in
- panic-prone indexing is easier to miss
- wildcard dependencies can weaken reproducibility
- edition migration issues accumulate quietly
- pedantic improvements never surface

### With the current monochange lint posture

- correctness issues fail fast
- suspicious, style, complexity, performance, and pedantic issues show up in review
- some noisy lints are intentionally relaxed where the team prefers readability or lower boilerplate
- panic-prone indexing is blocked at crate level

## When to add a local `#[allow(...)]`

A local allow is acceptable when:

- the lint is technically correct but the preferred alternative is harder to read
- the code is constrained by a protocol, generated shape, or test pattern
- you can explain the exception in one sentence

Example:

```rust
#[allow(clippy::too_many_arguments)]
fn build_release_payload(
	owner: &str,
	repo: &str,
	version: &str,
	tag: &str,
	notes: &str,
	draft: bool,
	prerelease: bool,
) {
	// ...
}
```

Use this sparingly. If the function can be improved with a struct or builder, prefer that.

## Recommended validation loop after edits

```bash
devenv shell fix:all
devenv shell lint:all
mc validate
```

If you changed shared docs too:

```bash
devenv shell docs:check
```

<!-- {/lintingPolicyReference} -->
