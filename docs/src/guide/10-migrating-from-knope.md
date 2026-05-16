# Migrating from knope

This guide walks through converting a `knope.toml` configuration to `monochange.toml`.

monochange was originally inspired by knope and shares many of the same ideas — changeset-driven releases, configurable workflows, GitHub integration — but uses a different configuration surface and adds cross-ecosystem support.

## Quick comparison

| Feature                | knope                         | monochange                                        |
| ---------------------- | ----------------------------- | ------------------------------------------------- |
| Config file            | `knope.toml`                  | `monochange.toml`                                 |
| CLI binary             | `knope`                       | `monochange` / `mc`                               |
| Changeset directory    | `.changeset/`                 | `.changeset/`                                     |
| Changeset format       | Markdown frontmatter          | Markdown frontmatter                              |
| Conventional commits   | Supported                     | Not supported                                     |
| Single-package config  | `[package]`                   | `[package.<id>]`                                  |
| Multi-package config   | `[packages.<name>]`           | `[package.<id>]`                                  |
| Version groups         | Implicit (single `[package]`) | Explicit `[group.<id>]`                           |
| Workflows              | `[[workflows]]`               | `[cli.<command>]`                                 |
| GitHub config          | `[github]`                    | `[source]` (provider-neutral)                     |
| Ecosystem support      | Rust, Go, JS                  | Rust, npm, pnpm, Bun, Deno, Dart, Flutter, Python |
| Dependency propagation | Not built-in                  | Automatic parent bumps                            |

## Step 1 — Replace the config file

Delete `knope.toml` and create `monochange.toml` at the repository root.

## Step 2 — Migrate package declarations

### Single-package knope repository

knope uses a bare `[package]` table for single-package repos:

```toml
# knope.toml
[package]
versioned_files = [
	{ path = "Cargo.toml", type = "cargo" },
	{ path = "Cargo.lock", type = "cargo" },
]
changelog = "changelog.md"
scopes = ["core", "cli"]
extra_changelog_sections = [
	{ name = "Notes", types = ["note"] },
	{ name = "Documentation", types = ["docs"] },
]
```

In monochange, every package gets a named `[package.<id>]` entry. Use `[defaults]` to reduce boilerplate and `[group.<id>]` when all packages should share one version:

```toml
# monochange.toml
[defaults]
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"

[package.my-crate]
path = "."
versioned_files = [{ path = "Cargo.lock", type = "cargo" }]
extra_changelog_sections = [
	{ name = "Notes", types = ["note"] },
	{ name = "Documentation", types = ["docs"] },
]
```

> **Note:** knope's `scopes` filter conventional commits to specific packages. monochange does not use conventional commits — use changeset frontmatter keys instead.

### Multi-package knope repository

knope uses `[packages.<name>]` for multi-package repos:

```toml
# knope.toml
[packages.sdk_core]
versioned_files = [
	"crates/sdk_core/Cargo.toml",
	{ path = "Cargo.lock", dependency = "sdk_core" },
	{ path = "Cargo.toml", dependency = "sdk_core" },
]
changelog = "crates/sdk_core/changelog.md"

[packages.sdk_cli]
versioned_files = [
	"crates/sdk_cli/Cargo.toml",
	{ path = "Cargo.lock", dependency = "sdk_cli" },
]
changelog = "crates/sdk_cli/changelog.md"
```

In monochange, use `[package.<id>]` entries with a `path` field. monochange updates native manifests automatically for supported ecosystems, so `versioned_files` only needs to cover _extra_ managed files:

```toml
# monochange.toml
[defaults]
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/changelog.md"

[package.sdk_core]
path = "crates/sdk_core"
versioned_files = [
	"Cargo.lock",
	{ path = "Cargo.toml", dependency = "sdk_core" },
]

[package.sdk_cli]
path = "crates/sdk_cli"
versioned_files = [
	{ path = "Cargo.lock", type = "cargo" },
]
```

> **Tip:** you do not need to list the package's own `Cargo.toml` as a versioned file — monochange discovers and updates native manifests automatically.

## Step 3 — Migrate version groups

knope's single `[package]` table implicitly groups all crates under one version. When migrating a repo that uses `[package]` with multiple `versioned_files` dependency entries, create an explicit `[group.<id>]`:

```toml
# monochange.toml
[group.main]
packages = ["sdk_core", "sdk_cli"]
tag = true
release = true
version_format = "primary"

[group.main.changelog]
path = "changelog.md"
```

Group behavior:

- all members share one synchronized version
- `tag`, `release`, and `version_format` are owned by the group
- member packages can still have their own changelogs
- members without direct changes get a configurable `empty_update_message` fallback

## Step 4 — Migrate workflows to CLI commands

knope uses `[[workflows]]` arrays. monochange uses `[cli.<command>]` map entries that become top-level CLI subcommands.

### knope workflow

```toml
# knope.toml
[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"

[[workflows.steps]]
type = "Command"
command = "dprint fmt"

[[workflows.steps]]
type = "Command"
command = "git add --all"

[[workflows.steps]]
type = "Command"
command = 'git commit -m "chore: prepare releases {{ version }}"'

[[workflows.steps]]
type = "Command"
command = "git push"

[[workflows.steps]]
type = "Release"
```

### monochange equivalent

```toml
# monochange.toml
[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"

[cli.publish-release]
help_text = "Prepare a release and publish provider releases"

[[cli.publish-release.steps]]
type = "PrepareRelease"

[[cli.publish-release.steps]]
type = "PublishRelease"
```

### Workflow step mapping

| knope step         | monochange step         | Notes                                                                                                             |
| ------------------ | ----------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `PrepareRelease`   | `PrepareRelease`        | Same name, same purpose                                                                                           |
| `CreateChangeFile` | `CreateChangeFile`      | Same name                                                                                                         |
| `Release`          | `PublishRelease`        | knope's `Release` creates GitHub releases; monochange calls this `PublishRelease` and supports multiple providers |
| `Command`          | `Command`               | Same name; monochange adds `dry_run_command` and `shell = true`                                                   |
| —                  | `OpenReleaseRequest`    | New: open/update a release PR                                                                                     |
| —                  | `PrepareRelease`        | New: refresh the cached `.monochange/release-manifest.json` artifact for downstream CI                            |
| —                  | `AffectedPackages`      | New: PR changeset policy enforcement                                                                              |
| —                  | `Validate`              | New: validate config and changesets                                                                               |
| —                  | `Discover`              | New: list workspace packages                                                                                      |
| —                  | `CommentReleasedIssues` | New: comment on closed issues referenced in changesets                                                            |

### Common knope workflow → monochange command recipes

**Create a changeset** (knope `document-change`):

```toml
# monochange.toml
[cli.change]
help_text = "Create a change file"

[[cli.change.inputs]]
name = "package"
type = "string_list"
required = true

[[cli.change.inputs]]
name = "bump"
type = "choice"
choices = ["patch", "minor", "major"]
default = "patch"

[[cli.change.inputs]]
name = "reason"
type = "string"
required = true

[[cli.change.steps]]
type = "CreateChangeFile"
```

**Open a release PR** (no knope equivalent):

```toml
# monochange.toml
[cli.release-pr]
help_text = "Open or update a release pull request"

[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
```

> **Key difference:** knope workflows often include manual `git add`, `git commit`, and `git push` Command steps. monochange handles git operations internally when using `PublishRelease` or `OpenReleaseRequest`, so you can drop those manual steps.

## Step 5 — Migrate GitHub configuration

### knope

```toml
# knope.toml
[github]
owner = "my-org"
repo = "my-repo"
```

### monochange

monochange uses a provider-neutral `[source]` table. GitHub is the default provider:

```toml
# monochange.toml
[source]
provider = "github" # default, can be omitted
owner = "my-org"
repo = "my-repo"

[source.releases]
enabled = true
source = "monochange"

[source.releases]
branches = ["main"]
enforce_for_tags = true
enforce_for_publish = true
enforce_for_commit = false
changeset_context_timeout_seconds = 120

[source.pull_requests]
enabled = true
branch_prefix = "monochange/release"
base = "main"
title = "chore(release): prepare release"
labels = ["release", "automated"]
```

monochange also supports GitLab and Gitea providers:

```toml
[source]
provider = "gitlab"
owner = "my-group"
repo = "my-project"
host = "gitlab.example.com"
```

## Step 6 — Migrate changeset files

monochange and knope both use markdown-frontmatter changesets under `.changeset/`. The format is compatible, but there are differences in how packages are referenced.

### knope changeset

```markdown
---
my_crate: minor
---

# add new feature

Details about the feature.
```

### monochange changeset

Same format — but use declared **package ids** or **group ids** as keys:

```markdown
---
my_crate: minor
---

# add new feature

Details about the feature.
```

If you have a group, you can target the group directly:

```markdown
---
main: minor
---

# coordinated release across all packages
```

> **Note:** a changeset may not reference both a group id and one of its member package ids in the same file. Use either the group id or individual package ids.

## Step 7 — Handle knope-specific features

### Conventional commits

knope can derive version bumps from conventional commit messages. monochange does not support conventional commits — all version changes must come from changeset files.

If your knope config uses conventional commits alongside changesets:

```toml
# knope.toml — remove this
[changes]
ignore_conventional_commits = false # or absent
```

Switch to changeset-only workflows. Use `mc step:create-change-file` to create changesets:

```bash
mc step:create-change-file --package my_crate --bump minor --reason "add new feature"
```

### knope `scopes`

knope uses `scopes` to filter conventional commits to specific packages. Since monochange doesn't use conventional commits, there is no equivalent. Remove `scopes` entries from your config.

### knope `[bot.releases]`

```toml
# knope.toml
[bot.releases]
enabled = true
```

In monochange, release automation is configured through `[changesets.affected]`:

```toml
# monochange.toml
[changesets.affected]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["crates/**", "packages/**"]
ignored_paths = ["docs/**", "readme.md"]
```

### knope `forced-release` workflow

knope's `forced-release` workflow runs `Release` without `PrepareRelease`. In monochange, use `publish-release` which always requires a `PrepareRelease` step first. For publishing without changesets, create a changeset manually or adjust the release flow.

### Regex-based versioned files

knope supports regex patterns in versioned files:

```toml
# knope.toml
versioned_files = [
	{ path = "readme.md", regex = "my_crate = \"(?<version>\\d+\\.\\d+\\.\\d+)\"" },
]
```

monochange does not currently support regex-based version file updates. For now, handle these with a `Command` step:

```toml
[[cli.release.steps]]
type = "Command"
command = "sed -i 's/my_crate = \"[0-9.]*\"/my_crate = \"{{ version }}\"/' readme.md"
shell = true
```

## Step 8 — Migrate GitHub Actions workflows

### knope GitHub Actions

A typical knope CI workflow runs `knope release` or `knope document-change`:

```yaml
# Before
- run: knope release
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### monochange GitHub Actions

Replace with the equivalent monochange command (assuming a `[cli.release]` workflow or using `mc step:prepare-release` directly):

```yaml
# After
- run: mc release
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

For PR-based release flows with monochange, add a changeset policy workflow:

```yaml
- name: run changeset policy
  run: |
    mc step:affected-packages --format json --verify \
      --changed-paths file1.rs \
      --changed-paths file2.rs
```

See [GitHub automation](./08-github-automation.md) for a complete workflow example.

## Complete migration example

### Before — `knope.toml`

```toml
[package]
versioned_files = [
	"Cargo.toml",
	{ dependency = "my_core", path = "Cargo.lock" },
	{ dependency = "my_core", path = "Cargo.toml" },
	{ dependency = "my_cli", path = "Cargo.lock" },
	{ dependency = "my_cli", path = "Cargo.toml" },
]
changelog = "changelog.md"
scopes = ["core", "cli"]
extra_changelog_sections = [
	{ name = "Notes", types = ["note"] },
	{ name = "Documentation", types = ["docs"] },
]

[changes]
ignore_conventional_commits = true

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"

[[workflows.steps]]
type = "Command"
command = "dprint fmt"

[[workflows.steps]]
type = "Command"
command = "git add --all"

[[workflows.steps]]
type = "Command"
command = 'git commit -m "chore: prepare releases {{ version }}"'

[[workflows.steps]]
type = "Command"
command = "git push"

[[workflows.steps]]
type = "Release"

[[workflows]]
name = "document-change"

[[workflows.steps]]
type = "CreateChangeFile"

[[workflows.steps]]
type = "Command"
command = "dprint fmt .changeset/* --allow-no-files"

[github]
owner = "my-org"
repo = "my-repo"
```

### After — `monochange.toml`

```toml
[defaults]
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"

[package.my_core]
path = "crates/my_core"
extra_changelog_sections = [
	{ name = "Notes", types = ["note"] },
	{ name = "Documentation", types = ["docs"] },
]

[package.my_cli]
path = "crates/my_cli"
extra_changelog_sections = [
	{ name = "Notes", types = ["note"] },
	{ name = "Documentation", types = ["docs"] },
]

[group.main]
packages = ["my_core", "my_cli"]
tag = true
release = true
version_format = "primary"

[group.main.changelog]
path = "changelog.md"

[source]
provider = "github"
owner = "my-org"
repo = "my-repo"

[source.releases]
enabled = true
source = "monochange"

[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"

[cli.publish-release]
help_text = "Prepare a release and publish provider releases"

[[cli.publish-release.steps]]
type = "PrepareRelease"

[[cli.publish-release.steps]]
type = "PublishRelease"

[cli.change]
help_text = "Create a change file"

[[cli.change.inputs]]
name = "package"
type = "string_list"
required = true

[[cli.change.inputs]]
name = "bump"
type = "choice"
choices = ["patch", "minor", "major"]
default = "patch"

[[cli.change.inputs]]
name = "reason"
type = "string"
required = true

[[cli.change.steps]]
type = "CreateChangeFile"

# `validate` is a built-in step command; run `mc step:validate` directly instead of defining [cli.validate].
```

## Migration checklist

- [ ] Delete `knope.toml`
- [ ] Create `monochange.toml` with `[defaults]` and `[package.<id>]` entries
- [ ] Add `[group.<id>]` if packages should share a version
- [ ] Replace `[[workflows]]` with `[cli.<command>]` entries
- [ ] Replace `[github]` with `[source]`
- [ ] Remove `scopes` and `[changes]` sections (no conventional commits)
- [ ] Update `.changeset/*.md` frontmatter keys to use declared package/group ids
- [ ] Update CI workflows from `knope <command>` to `mc <command>`
- [ ] Run `mc step:validate` to check config and changesets
- [ ] Run `mc step:prepare-release --dry-run` to verify the release plan
- [ ] Remove knope from your dependencies and install monochange
