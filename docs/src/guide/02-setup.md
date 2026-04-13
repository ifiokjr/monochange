# Your first release plan

Use this guide after installation when you want one local, beginner-safe walkthrough.

You will stop at `mc release --dry-run --format json`, so nothing is published.

## 1. Generate a starter config with `mc init`

Run this at the repository root:

```bash
mc init
```

`mc init` detects packages, writes an annotated `monochange.toml`, and gives you a better starting point than hand-authoring a first config from scratch.

The generated file becomes the source of truth for commands like `mc validate`, `mc discover`, `mc change`, and `mc release`. If you later want editable copies of the built-in CLI commands, run:

```bash
mc populate
```

That appends any missing default command definitions to `monochange.toml` without overwriting commands you already defined.

### Automated CI setup with `--provider`

When you know which source provider you will use for release automation, include the `--provider` flag during initialization:

```bash
mc init --provider github
```

<!-- {=initProviderFeature} -->

The `--provider` flag supports `github`, `gitlab`, and `gitea`. When provided, `mc init`:

1. **Configures the `[source]` section** — adds provider-specific settings for releases and pull/merge requests
2. **Generates provider CLI commands** — includes `commit-release` and `release-pr` commands in `monochange.toml`
3. **Creates workflow files** (GitHub only) — writes `.github/workflows/release.yml` and `.github/workflows/changeset-policy.yml`
4. **Auto-detects owner/repo** — parses `git remote get-url origin` to pre-populate `[source]`

Example generated configuration with `--provider github`:

```toml
[source]
provider = "github"
owner = "ifiokjr"      # auto-detected from git remote
repo = "monochange"   # auto-detected from git remote

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

## 2. Validate the generated workspace

```bash
mc validate
```

This confirms that the generated config and any existing `.changeset/*.md` files agree with the workspace.

If validation fails, fix the reported problem first, then rerun `mc validate`.

## 3. Discover the package ids you will actually use

<!-- {=projectDiscoverCommand} -->

```bash
mc validate
mc discover --format json
```

<!-- {/projectDiscoverCommand} -->

The most important thing to find in discovery output is the package id you want to target in your first change file.

If you are unsure what id to use later, rerun discovery and copy one from the output.

## 4. Create one change file

```bash
mc change --package <id> --bump patch --reason "describe the change"
```

Most changes should target a package id.

monochange will propagate bumps to dependents and synchronize configured groups for you, so group ids are best reserved for intentionally shared ownership.

## 5. Preview the release plan safely

<!-- {=projectDryRunCommand} -->

```bash
mc release --dry-run --format json
```

<!-- {/projectDryRunCommand} -->

This is the right stopping point for a first-time user.

You get a concrete preview of the release plan without publishing anything or opening provider requests.

## Package ids vs. group ids

Use this rule of thumb:

- **package ids first** — most authored changes belong to one package
- **group ids later** — use a group id only when the change is intentionally owned by the whole group

That keeps your first changes simple while still letting monochange synchronize grouped packages when needed.

## First-failure recovery

### `mc init` says a config already exists

Keep the existing `monochange.toml`, inspect it, and continue with `mc validate`. If you want to regenerate the config from scratch, pass the `--force` flag:

```sh
mc init --force
```

### `mc validate` reports config or changeset errors

Fix the reported issue first. `mc validate` is the fastest way to get back to a known-good workspace.

### `mc change` says the package id is unknown

Run `mc discover --format json` again and copy an id directly from the output.

### You are not ready to hand-edit config yet

That is normal. Stay with the generated `monochange.toml` until the basic flow feels familiar.

## When to edit `monochange.toml` by hand

Most first-time users should not start by writing a large config manually.

Reach for manual edits when you want to:

- rename or reorganize package ids
- define groups with `[group.<id>]`
- customize changelog paths or formats
- add provider configuration for release publishing or release PRs
- expand the CLI surface beyond the default generated commands

## Reference: expanded configuration example

The example below shows the broader package, group, changelog, source-provider, and CLI-command model.

Use it as reference material after the generated config makes sense.

<!-- {=projectSetupConfig} -->

```toml
[defaults]
parent_bump = "patch"
warn_on_group_mismatch = true
package_type = "cargo"

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

[package.sdk-core]
path = "crates/sdk_core"
extra_changelog_sections = [
	{ name = "Security", types = ["security"], default_bump = "patch" },
]

[package.web-sdk]
path = "packages/web-sdk"
type = "npm"

[package.mobile-sdk]
path = "packages/mobile-sdk"
type = "dart"

[group.sdk]
packages = ["sdk-core", "web-sdk", "mobile-sdk"]
tag = true
release = true
version_format = "primary"

[group.sdk.changelog]
path = "docs/sdk-changelog.md"
format = "monochange"

[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[source.releases]
source = "monochange"

[source.pull_requests]
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

[cli.validate]
help_text = "Validate monochange configuration and changesets"

[[cli.validate.steps]]
name = "validate workspace"
type = "Validate"

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

[cli.change]
help_text = "Create a change file for one or more packages"

[[cli.change.inputs]]
name = "interactive"
type = "boolean"
short = "i"

[[cli.change.inputs]]
name = "package"
type = "string_list"

[[cli.change.inputs]]
name = "bump"
type = "choice"
choices = ["none", "patch", "minor", "major"]
default = "patch"

[[cli.change.inputs]]
name = "version"
type = "string"

[[cli.change.inputs]]
name = "reason"
type = "string"

[[cli.change.inputs]]
name = "type"
type = "string"

[[cli.change.inputs]]
name = "details"
type = "string"

[[cli.change.inputs]]
name = "output"
type = "path"

[[cli.change.steps]]
name = "create change file"
type = "CreateChangeFile"

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

<!-- {/projectSetupConfig} -->

<!-- {=projectSetupConfigNote} -->

This guide shows the preferred package/group configuration model together with an expanded CLI command surface.

<!-- {/projectSetupConfigNote} -->
