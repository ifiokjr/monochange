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

Legacy repositories may still contain `[[package_overrides]]` entries such as:

```toml
[[package_overrides]]
package = "crates/sdk_core"
changelog = "crates/sdk_core/changelog.md"
```

Under the new model, move that changelog configuration onto the matching `[package.<id>]` declaration instead. When `[defaults].package_type` is set, package entries may also omit an explicit `type`.

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

<!-- {/configurationWorkflowsSnippet} -->

<!-- {@configurationWorkflowVariables} -->

- default command substitution when `variables` is omitted: `{{ version }}`, `$group_version`, `$released_packages`, `$changed_files`, and `$changesets`
- command templates can read CLI inputs through `{{ inputs.name }}`; bare input names still work for backward compatibility
- every step can override the inputs it receives with `inputs = { ... }`; direct references like `"{{ inputs.labels }}"` preserve list and boolean values when rebinding to built-in steps
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

Use a group id only when the change is intentionally owned by the whole group and should read that way in release output. Legacy manifest-relative paths and directory paths may still appear in older repos during migration, but `mc validate` should guide you toward declared ids.

<!-- {/configurationPackageReferenceRules} -->

<!-- {@configurationCurrentStatus} -->

Current implementation notes:

- `defaults.include_private` is parsed, but discovery behavior is still centered on the supported fixture-driven CLI commands in this milestone
- `version_groups.strategy` belongs to the legacy model and should be migrated to `[group.<id>]`
- legacy `[[workflows]]` configuration is no longer supported; use `[cli.<command>]` plus `[[cli.<command>.steps]]` instead
- `[ecosystems.*].enabled/roots/exclude` are parsed, but discovery still scans all supported ecosystems regardless of those settings today
- `package_overrides.changelog` is a legacy setting that should be migrated to package declarations
- `defaults.strict_version_conflicts` controls whether conflicting explicit `version` entries across changesets warn-and-pick-highest (default) or fail planning outright
- source automation expects `[source]` with provider-specific settings under `[source.releases]`, `[source.pull_requests]`, and `[source.bot.changesets]`; GitHub remains the default provider
- live GitHub release and release-request publishing uses `octocrab` with `GITHUB_TOKEN` / `GH_TOKEN`; GitLab and Gitea use direct HTTP APIs
- release-request publishing still uses local `git` for branch, commit, and push operations before provider API updates when not in dry-run mode
- changeset policy commands currently apply only to the GitHub provider and expect `[source.bot.changesets]`, a `changed_paths` command input, and reusable diagnostics for GitHub Actions consumption
- supported command steps today are `Validate`, `Discover`, `CreateChangeFile`, `PrepareRelease`, `CommitRelease`, `RenderReleaseManifest`, `PublishRelease`, `OpenReleaseRequest`, `CommentReleasedIssues`, `AffectedPackages`, `DiagnoseChangesets`, `RetargetRelease`, and `Command`
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

- `mc release-manifest` writes a stable JSON artifact for downstream automation
- `mc publish-release` previews or publishes provider releases from the structured release notes
- `mc release-pr` previews or opens an idempotent provider release request
- `mc affected` evaluates pull-request changeset policy from CI-supplied changed paths and labels

<!-- {/githubAutomationOverview} -->

<!-- {@githubAutomationWorkflowCommands} -->

```bash
mc release --dry-run --format json
mc release-manifest --dry-run
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

- declaring `[github]`, `[github.releases]`, and `[github.pull_requests]` in `monochange.toml`
- running a real `changeset-policy` GitHub Actions workflow that shells into `mc affected`

<!-- {/githubAutomationDogfoodNotes} -->

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
6. **Generate manifest** — `mc release-manifest --dry-run` writes a stable JSON artifact for downstream automation.
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
- `RenderReleaseManifest` — write a stable JSON manifest
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
