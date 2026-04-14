# Configuration

Repository configuration lives in `monochange.toml`.

## Defaults

<!-- {=configurationDefaultsSnippet} -->

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
- `publish`
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

`extra_changelog_sections` can also be set on:

- `[defaults]`
- `[package.<id>]`
- `[group.<id>]`

Defaults are inherited by packages and groups; package/group definitions append target-specific sections on top of the workspace defaults.

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

## Package publishing

Built-in package publishing is configured through `publish` on packages and ecosystems.

```toml
[ecosystems.npm.publish]
enabled = true
mode = "builtin"
registry = "npm"
trusted_publishing = true

[package.web.publish]
mode = "builtin"

[package.web.publish.placeholder]
readme_file = "docs/web-placeholder.md"
```

Supported fields:

- `enabled` — include this package in managed publishing
- `mode` — `builtin` or `external`
- `registry` — public registry override for the package ecosystem
- `trusted_publishing` — `true`/`false` or a table with `enabled`, `repository`, `workflow`, and `environment`
- `placeholder.readme` — inline placeholder README content
- `placeholder.readme_file` — workspace-relative file to use as placeholder README content

Inheritance flows from `[ecosystems.<name>.publish]` to matching packages, and package-level values override the inherited defaults.

Built-in publishing currently targets only the canonical public registry for each supported ecosystem:

- Cargo → `crates.io`
- npm packages → `npm`
- Deno packages → `jsr`
- Dart / Flutter packages → `pub.dev`

If you need a private or custom registry, set `mode = "external"` and handle publication outside monochange.

### Placeholder publishing

`mc placeholder-publish` exists for the bootstrap case where a package must already exist in the registry before you can finish automation setup such as trusted publishing.

For each managed package with built-in publishing enabled, monochange:

- checks whether the package already exists in its configured public registry
- skips packages that already exist
- publishes a placeholder package only for packages that are missing
- uses version `0.0.0`
- renders a default placeholder README unless `placeholder.readme` or `placeholder.readme_file` overrides it

`placeholder.readme` and `placeholder.readme_file` are mutually exclusive. If both are set, config validation fails.

### Trusted publishing

`trusted_publishing` lets you tell monochange that package publication is expected to come from a verified GitHub Actions context.

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[package.cli.publish.trusted_publishing]
enabled = true
repository = "owner/repo"
workflow = "publish.yml"
environment = "publisher"
```

When `trusted_publishing` is enabled:

- npm packages can be configured automatically with `npm trust github ...`
- pnpm workspaces use `pnpm exec npm trust ...` and `pnpm publish`, so workspace protocol and catalog dependency handling stays aligned with the workspace manager
- Cargo, `jsr`, and `pub.dev` currently require manual trusted-publishing setup; monochange reports the setup URL and blocks built-in release publishing until trust is configured

monochange resolves the GitHub trust context from:

- explicit `repository`, `workflow`, and `environment` values in config
- otherwise `[source]` plus GitHub Actions environment such as `GITHUB_WORKFLOW_REF` and `GITHUB_JOB`
- and, when possible, the workflow job environment declared in `.github/workflows/<file>.yml`

If monochange cannot determine the GitHub repository or workflow for an npm package, automatic trust setup cannot proceed.

### Current implementation limits

The built-in package publishing flow is intentionally narrow for now:

- no private or custom registry support in `mode = "builtin"`
- no built-in retry scheduler or delayed requeue for registry rate limits yet
- manual trusted-publishing setup is still required for `crates.io`, `jsr`, and `pub.dev`

If your workflow needs any of those today, keep the package on `mode = "external"` and let your own CI or scripts own publication.

## Groups

Groups own outward release identity for their member packages.

```toml
[group.sdk]
packages = ["sdk-core", "web-sdk", "mobile-sdk"]
changelog = "changelog.md"
versioned_files = [{ path = "group.toml", type = "cargo" }]
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
- `[group.<id>.changelog].include` can filter which member-targeted changesets appear in the group changelog without changing release planning or package changelogs

For grouped changelog filtering, use the changelog table form:

```toml
[group.sdk.changelog]
path = "docs/sdk-changelog.md"
include = ["sdk-cli"]
```

`include` accepts:

- `"all"` — include direct group-targeted changesets and all member-targeted changesets (default)
- `"group-only"` — include only direct group-targeted changesets
- `[]` or `["package-id", ...]` — include direct group-targeted changesets plus member-targeted changesets only when every target in that group is listed

## Versioned files

`versioned_files` are additional managed files beyond native manifests.

Examples:

```toml
# package-scoped shorthand infers the package ecosystem
versioned_files = ["Cargo.toml"]
versioned_files = ["**/crates/*/Cargo.toml"]

# explicit typed entries remain available
versioned_files = [{ path = "group.toml", type = "cargo", name = "sdk-core" }]
versioned_files = [{ path = "docs/version.txt", type = "cargo" }]
versioned_files = [
	{ path = "Cargo.toml", type = "cargo", fields = ["workspace.metadata.bin.monochange.version"], prefix = "" },
]
versioned_files = [
	{ path = "package.json", type = "npm", fields = ["metadata.bin.monochange.version"] },
]

# ecosystem-level defaults inherited by matching packages
[ecosystems.npm]
versioned_files = ["**/packages/*/package.json"]
```

Typed manifest entries can update dependency sections and arbitrary string fields inside TOML or JSON manifests. Dependency targets in `versioned_files` must reference declared package ids. Groups must use explicit typed entries because monochange cannot infer a group ecosystem from a bare string.

### Regex versioned files

<!-- {=configurationRegexVersionedFilesSnippet} -->

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

## Lockfile commands

By default monochange rewrites supported lockfiles directly from the release plan. That keeps normal `mc release` runs close to `--dry-run` speed instead of launching package managers just to rewrite workspace version strings.

Built-in direct lockfile updates cover:

- Cargo: `Cargo.lock`
- npm-family: `package-lock.json`, `pnpm-lock.yaml`, `bun.lock`, and `bun.lockb`
- Deno: `deno.lock`
- Dart / Flutter: `pubspec.lock`

If you configure `lockfile_commands` for an ecosystem, monochange stops using the built-in direct updater for that ecosystem and those commands fully own lockfile refresh. Use that escape hatch only when your workspace needs package-manager-side regeneration beyond version rewrites.

For Cargo specifically, monochange no longer falls back to `cargo generate-lockfile` automatically when a lockfile looks incomplete. That keeps `mc release` on the fast path and leaves the final dependency-resolution refresh under your control: either configure `[ecosystems.cargo].lockfile_commands` explicitly or run `cargo generate-lockfile` / `cargo check` yourself afterwards.

If you want to measure that tradeoff before opting into a refresh command, run the `prepare_release_apply_cargo_lockfile_refresh` Criterion benchmark. It compares the default `direct_rewrite` path against an explicit `full_refresh_command` run on the same synthetic Cargo workspace.

```toml
[ecosystems.npm]
lockfile_commands = [
	{ command = "pnpm install --lockfile-only", cwd = "packages/web" },
	{ command = "npm install --package-lock-only", cwd = "packages/legacy", shell = true },
]
```

`cwd` is resolved relative to the workspace root. `shell = false` runs the command directly, `shell = true` uses `sh -c`, and `shell = "bash"` uses a custom shell binary.

## CLI commands

CLI commands are user-defined top-level commands. monochange starts from its built-in default command set, then applies each `[cli.<command>]` entry as a full command override when the name matches or as an additional command when it does not. Each resulting command becomes invocable as `mc <command>`.

If you want editable copies of the built-in commands in your config file, run `mc populate`. It appends only the missing default command definitions to `monochange.toml` and leaves existing `[cli.<command>]` entries unchanged.

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

CLI command interpolation variables:

<!-- {=configurationWorkflowVariables} -->

- built-in command variables are available directly as `{{ version }}`, `{{ group_version }}`, `{{ released_packages }}`, `{{ changed_files }}`, and `{{ changesets }}`
- command templates can read CLI inputs through `{{ inputs.name }}`
- every step can override the inputs it receives with `inputs = { ... }`; direct references like `"{{ inputs.labels }}"` preserve list and boolean values when rebinding to built-in steps
- built-in commands already attach descriptive step `name` labels such as `prepare release` and `publish release`; keep or replace those labels when you want progress output to stay readable
- custom command variables become available when `variables` is present: map your own names to variables such as `version`, `group_version`, `released_packages`, `changed_files`, and `changesets`
- `dry_run_command` on a `Command` step replaces `command` only when the CLI command is run with `--dry-run`
- `shell = true` runs the command through the current shell; the default mode runs the executable directly after shell-style splitting

<!-- {/configurationWorkflowVariables} -->

Performance tip: keep the default `mc release` path focused on built-in steps such as `PrepareRelease`. Arbitrary `Command` steps shell out to external tools, so expensive follow-up work like formatting, validation, publishing, or pushes should usually be gated behind an explicit input such as `when = "{{ inputs.commit }}"` if you want local release preparation to stay sub-second.

`RetargetRelease` is intentionally different from `PrepareRelease`-driven steps. It operates from git history plus source/provider information, discovers the durable `ReleaseRecord`, and then exposes structured `retarget.*` outputs for later command steps.

See [Repairable releases](./12-repairable-releases.md) for when to use `mc repair-release` versus publishing a new patch release.

## GitHub release settings

Use `[source]` plus `[source.releases]` when you want command steps such as `PublishRelease` to derive repository release payloads from the prepared release. GitHub remains the default provider when `provider` is omitted.

<!-- {=configurationGitHubSnippet} -->

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

## Ecosystem settings

These settings are parsed from config and document intended control points for discovery:

<!-- {=configurationEcosystemSettingsSnippet} -->

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

## Changelog configuration

<!-- {=configurationPackageOverridesSnippet} -->

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

## Package references

<!-- {=configurationPackageReferenceRules} -->

Package references in changesets and CLI commands should use configured ids.

Prefer package ids when a leaf package changed. That keeps the authored change as specific as possible, and monochange will still propagate bumps to dependents and synchronize any configured groups automatically.

Use a group id only when the change is intentionally owned by the whole group and should read that way in release output.

<!-- {/configurationPackageReferenceRules} -->

## Current status

<!-- {=configurationCurrentStatus} -->

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

## Validation

Run:

```bash
mc validate
```

`mc validate` validates:

- package and group declarations
- manifest presence for each package type
- group membership rules
- `versioned_files` structural rules (type/regex conflicts, capture groups)
- `versioned_files` content checks: file existence, version field readability, regex pattern matching
- `.changeset/*.md` targets and overlap rules
- Cargo workspace version-group constraints
- `[source]` url scheme security (`https://` required)
