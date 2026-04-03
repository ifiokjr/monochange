# Configuration

Repository configuration lives in `monochange.toml`.

## Defaults

<!-- {=configurationDefaultsSnippet} -->

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

## Packages

Declare every release-managed package explicitly.

<!-- {=configurationVersionGroupsSnippet} -->

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

Required fields:

- `path`
- `type`, unless `[defaults].package_type` is set

Supported `type` values:

- `cargo`
- `npm`
- `deno`
- `dart`
- `flutter`

Optional package fields:

- `type`, when `[defaults].package_type` is set
- `changelog`
- `empty_update_message`
- `versioned_files`
- `tag`
- `release`
- `version_format`

`changelog` accepts three forms on packages:

- `true` → use `{{ path }}/CHANGELOG.md`
- `false` → disable the package changelog
- `"some/path.md"` → use that exact path

`[defaults].changelog` also accepts three forms:

- `true` → default every package to `{{ path }}/CHANGELOG.md`
- `false` → default every package to no changelog
- `"{{ path }}/changelog.md"` or another pattern → replace `{path}` with each package path

A package-level `changelog` value overrides the default for that package.

`empty_update_message` lets changelog targets render a readable fallback entry when a version update is required but no direct release notes were recorded for that target. This is especially useful for grouped packages that keep their own changelog entries even when only another member of the group changed.

`empty_update_message` can be set on:

- `[defaults]`
- `[package.<id>]`
- `[group.<id>]`

Template placeholders may include:

- `{{ package }}` / `{{ package_name }}`
- `{{ package_id }}`
- `{{ group }}` / `{{ group_name }}`
- `{{ group_id }}`
- `{{ version }}` / `{{ new_version }}`
- `{{ current_version }}` / `{{ previous_version }}`
- `{{ bump }}`
- `{{ trigger }}`
- `{{ ecosystem }}`
- `{{ release_owner }}` / `{{ release_owner_kind }}`
- `{{ members }}` / `{{ member_count }}` for group changelogs
- `{{ reasons }}`

Fallback order:

- package changelog entries: package → group → defaults → built-in message
- group changelog entries: group → defaults → built-in message

The built-in grouped-package fallback reads:

> No package-specific changes were recorded; `{{ package }}` was updated to {{ version }} as part of group `{{ group }}`.

## Groups

Groups own outward release identity for their member packages.

```toml
[group.sdk]
packages = ["sdk-core", "web-sdk", "mobile-sdk"]
changelog = "changelog.md"
versioned_files = ["group.toml"]
tag = true
release = true
version_format = "primary"
```

Rules:

- group members must already be declared under `[package.<id>]`
- package and group ids share one namespace
- a package may belong to only one group
- only one package or group may use `version_format = "primary"`
- group `tag`, `release`, and `version_format` override member package release identity
- package changelogs and package `versioned_files` still apply when grouped
- grouped packages can customize fallback changelog entries with `empty_update_message` when no direct package notes are present

## Versioned files

`versioned_files` are additional managed files beyond native manifests.

Examples:

```toml
versioned_files = ["Cargo.lock"]
versioned_files = [{ path = "group.toml", dependency = "sdk-core" }]
```

Dependency targets in `versioned_files` must reference declared package ids.

## CLI commands

CLI commands are user-defined top-level commands. Each `[cli.<command>]` entry becomes invocable as `mc <command>`, and legacy `[[workflows]]` tables are no longer supported.

<!-- {=configurationWorkflowsSnippet} -->

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

CLI command interpolation variables:

<!-- {=configurationWorkflowVariables} -->

- default command substitution when `variables` is omitted: `{{ version }}`, `$group_version`, `$released_packages`, `$changed_files`, and `$changesets`
- custom command substitution when `variables` is present: map your own replacement strings to variable names such as `version`, `group_version`, `released_packages`, `changed_files`, and `changesets`
- `dry_run_command` on a `Command` step replaces `command` only when the CLI command is run with `--dry-run`
- `shell = true` runs the command through the current shell; the default mode runs the executable directly after shell-style splitting

<!-- {/configurationWorkflowVariables} -->

## GitHub release settings

Use `[source]` plus `[source.releases]` when you want command steps such as `PublishRelease` to derive repository release payloads from the prepared release. GitHub remains the default provider when `provider` is omitted.

<!-- {=configurationGitHubSnippet} -->

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

## Ecosystem settings

These settings are parsed from config and document intended control points for discovery:

<!-- {=configurationEcosystemSettingsSnippet} -->

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

## Package overrides migration note

<!-- {=configurationPackageOverridesSnippet} -->

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

## Package references

<!-- {=configurationPackageReferenceRules} -->

Package references in changesets and CLI commands should use configured package ids or group ids. Legacy manifest-relative paths and directory paths may still appear in older repos during migration, but `mc validate` should guide you toward declared ids.

<!-- {/configurationPackageReferenceRules} -->

## Current status

<!-- {=configurationCurrentStatus} -->

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
- supported command steps today are `Validate`, `Discover`, `CreateChangeFile`, `PrepareRelease`, `RenderReleaseManifest`, `PublishRelease`, `OpenReleaseRequest`, `CommentReleasedIssues`, `AffectedPackages`, and `Command`
- legacy `PublishGitHubRelease`, `OpenReleasePullRequest`, and `EnforceChangesetPolicy` step names are still accepted as migration aliases

<!-- {/configurationCurrentStatus} -->

## Validation

Run:

```bash
mc validate
```

`mc validate` validates:

- package and group declarations
- manifest presence for each package type
- group membership rules
- `versioned_files` references
- `.changeset/*.md` targets and overlap rules
