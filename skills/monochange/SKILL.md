---
name: monochange
description: Guides agents through monochange discovery, changesets, release planning, and provider-aware release workflows. Use when working on `monochange.toml`, `.changeset/*.md`, release automation, grouped versions, cross-ecosystem monorepo releases, CLI step composition, or MCP tool interactions.
---

# monochange

monochange is a cross-ecosystem release planner for monorepos that span more than one package ecosystem.

## Quick start

1. Read `monochange.toml` first — it is the single source of truth.
2. Run `mc validate` before making release-affecting edits.
3. Use `mc discover --format json` to inspect the workspace model.
4. Use `mc change` to write explicit release intent as `.changeset/*.md` files.
5. Use `mc release --dry-run --format json` before mutating release state.
6. Use `mc diagnostics --format json` to inspect changeset context and git provenance.

## Core principles

- **monochange.toml is the source of truth** — packages, groups, source providers, ecosystems, and `[cli.<command>]` entries all live here
- **Explicit change files** — use `.changeset/*.md` for release intent, not implicit commit messages
- **Dry-run first** — always preview with `--dry-run` before applying changes
- **Package ids over group ids** — most changes target packages; groups are for shared ownership
- **Validate early and often** — `mc validate` catches issues before they become problems

## Recommended command flow

<!-- {=recommendedCommandFlow} -->

1. **Validate** — `mc validate` checks config and changeset targets.
2. **Discover** — `mc discover --format json` inspects the workspace model.
3. **Create changesets** — `mc change --package <id> --bump <severity> --reason "..."` writes explicit release intent.
4. **Preview release** — `mc release --dry-run --format json` shows planned bumps, changelog output, and changed files.
5. **Inspect changeset context** — `mc diagnostics --format json` shows git provenance and linked review metadata for all pending changesets.
6. **Inspect cached manifest** — `mc release --dry-run --format json` refreshes the cached manifest and shows the downstream automation payload.
7. **Publish** — `mc publish-release --format json` creates provider releases after human review.

<!-- {/recommendedCommandFlow} -->

## CLI commands

| Command              | Purpose                                                     | Key Flags                                                                 |
| -------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------- |
| `mc init`            | Generate a starter `monochange.toml` from detected packages | `--force`, `--provider <github                                            |
| `mc populate`        | Append missing built-in CLI command definitions to config   | _none_                                                                    |
| `mc validate`        | Validate config and changeset targets                       | _none_                                                                    |
| `mc discover`        | Discover packages across ecosystems                         | `--format` (text\|json)                                                   |
| `mc change`          | Create a `.changeset/*.md` file                             | `--interactive`, `--package`, `--bump`, `--reason`, `--type`, `--details` |
| `mc release`         | Prepare a release plan from changesets                      | `--dry-run`, `--diff`, `--format` (markdown\|text\|json)                  |
| `mc commit-release`  | Prepare a release and create a local commit                 | `--format`, `--dry-run`                                                   |
| `mc publish-release` | Create provider releases                                    | `--format`, `--dry-run`                                                   |
| `mc affected`        | Evaluate changeset policy from changed paths                | `--format`, `--changed-paths`, `--since`, `--verify`, `--label`           |
| `mc diagnostics`     | Show changeset context with git and review metadata         | `--format`, `--changeset`                                                 |
| `mc repair-release`  | Repair a recent release by retargeting tags                 | `--from`, `--target`, `--force`, `--sync-provider`                        |
| `mc release-record`  | Inspect durable release record from tag or commit           | `--from`, `--format`                                                      |
| `mc assist`          | Print assistant install and MCP setup guidance              | `<assistant>`, `--format`                                                 |
| `mc mcp`             | Start the stdio MCP server                                  | _none_                                                                    |

## Global flags

All commands support:

- `--log-level <FILTER>` — Set tracing filter (e.g., `debug`, `info`, `warn`)
- `-q, --quiet` — Suppress output, enable dry-run behavior
- `--progress-format <FORMAT>` — Control progress output (`auto`, `unicode`, `ascii`, `json`)
- `--dry-run` — Preview changes without applying

## CLI step types

<!-- {=cliStepTypes} -->

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

## MCP tools

<!-- {=mcpToolsList} -->

- `monochange_validate` — validate `monochange.toml` and `.changeset` targets
- `monochange_discover` — discover packages, dependencies, and groups across the repository
- `monochange_change` — write a `.changeset` markdown file for one or more package or group ids
- `monochange_release_preview` — prepare a dry-run release preview from discovered `.changeset` files
- `monochange_release_manifest` — generate a dry-run release manifest JSON document for downstream automation
- `monochange_affected_packages` — evaluate changeset policy from changed paths and optional labels

<!-- {/mcpToolsList} -->

## Configuration reference

### monochange.toml structure

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

[release_notes]
change_templates = [
    "#### {{ summary }}\n\n{{ details }}\n\n{{ context }}",
]

[package.<id>]
path = "crates/<id>"
type = "cargo"

[group.<id>]
packages = ["<id1>", "<id2>"]
tag = true
release = true

[source]
provider = "github"
owner = "<owner>"
repo = "<repo>"

[cli.<command>]
help_text = "..."

[[cli.<command>.steps]]
name = "..."
type = "..."
```

### Key configuration concepts

#### Versioned files

`versioned_files` update additional managed files beyond native manifests:

- **Package-scoped shorthand**: `versioned_files = ["Cargo.toml"]` — infers the ecosystem
- **Explicit typed entries**: `versioned_files = [{ path = "group.toml", type = "cargo" }]`
- **Regex entries**: `versioned_files = [{ path = "README.md", regex = 'v(?<version>\d+\.\d+\.\d+)' }]` — must include named `version` capture

#### Lockfile commands

Configure automatic lockfile refresh per ecosystem:

```toml
[ecosystems.cargo]
lockfile_commands = ["cargo update --workspace"]

[ecosystems.npm]
lockfile_commands = ["npm install --package-lock-only"]
```

**Defaults:**

- Cargo: `cargo update --workspace`
- npm: `npm install`
- pnpm: `pnpm install`
- Bun: `bun install`

#### Release titles

<!-- {=releaseTitleConfig} -->

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

#### Groups and changelog filtering

Groups synchronize versions across packages. Group changelogs can filter entries:

- `include = "all"` — all member changesets (default)
- `include = "group-only"` — only direct group-targeted changesets
- `include = ["package-id"]` — specific member changesets plus group-targeted ones

#### Source providers

Configure `[source]` for provider automation:

```toml
[source]
provider = "github" # or "gitlab", "gitea"
owner = "<owner>"
repo = "<repo>"

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
```

## Template variables

Template variables use Mustache/`{{ }}` syntax:

**Release titles:**

- `{{ version }}` — The new version being released
- `{{ id }}` — Package or group ID
- `{{ date }}` — Current date (YYYY-MM-DD)
- `{{ time }}` — Current time (HH:MM:SS)
- `{{ datetime }}` — Full datetime (ISO 8601)
- `{{ changes_count }}` — Number of changes
- `{{ tag_url }}` — URL to git tag
- `{{ compare_url }}` — URL comparing versions

**Changeset templates:**

- `{{ summary }}` — The changeset summary
- `{{ details }}` — Detailed description
- `{{ context }}` — Full metadata context (preferred)

**With source provider:**

- `{{ change_owner_link }}` — Link to author
- `{{ review_request_link }}` — Link to PR/MR
- `{{ introduced_commit_link }}` — Link to first commit
- `{{ closed_issue_links }}` — Links to closed issues
- `{{ related_issue_links }}` — Links to related issues

## Automated CI setup

Use `--provider` flag during init for automated CI configuration:

```bash
mc init --provider github
```

This configures:

1. `[source]` section with provider settings
2. CLI commands for `commit-release` and `release-pr`
3. GitHub Actions workflows (GitHub only)
4. Auto-detected owner/repo from git remote

**Supported providers:** `github`, `gitlab`, `gitea`

## Working rules

- Treat `monochange.toml` as the source of truth
- Prefer configured package or group ids over guessing manifest names
- Use `.changeset/*.md` files for explicit release intent
- Run dry-run flows before real release commands
- Keep docs, templates, and changelog behavior aligned with config changes
- Use `mc diagnostics --format json` to audit changesets before release

## Common workflows

### Create a changeset

```bash
mc change --package <id> --bump patch --reason "fix: resolve memory leak in cache"
```

### Preview release

```bash
mc release --dry-run --format json
mc release --dry-run --diff  # with unified diffs
```

### Apply release locally

```bash
mc release
```

### Create release PR

When `--provider` was used during `mc init`, a `commit-release` command is available:

```bash
mc commit-release --dry-run --format json  # preview
mc commit-release  # prepare, commit, and open PR
```

### Evaluate changeset policy

```bash
mc affected --format json --changed-paths src/main.rs
```

### Repair a release

```bash
mc repair-release --from v1.2.3 --target HEAD --dry-run
mc repair-release --from v1.2.3 --target HEAD
```

## References

- [REFERENCE.md](REFERENCE.md) — detailed install steps, changeset authoring, grouped release rules, input types, step composition
- [docs/src/guide/](../../docs/src/guide/) — user guides for setup, configuration, and advanced workflows
- [docs/src/reference/](../../docs/src/reference/) — CLI step reference and configuration details
