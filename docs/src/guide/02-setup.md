# Setting up a project

Add a `monochange.toml` file at the repository root.

<!-- {=projectSetupConfig} -->

```toml
[defaults]
parent_bump = "patch"
warn_on_group_mismatch = true
package_type = "cargo"

[defaults.changelog]
path = "{path}/changelog.md"
format = "keep_a_changelog"

[release_notes]
change_templates = ["#### $summary\n\n$details", "- $summary"]

[package.sdk-core]
path = "crates/sdk_core"
extra_changelog_sections = [{ name = "Security", types = ["security"] }]

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

[github]
owner = "ifiokjr"
repo = "monochange"

[github.releases]
source = "monochange"

[github.pull_requests]
branch_prefix = "monochange/release"
base = "main"
title = "chore(release): prepare release"
labels = ["release", "automated"]
auto_merge = false

[github.bot.changesets]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["crates/**", "packages/**"]
ignored_paths = ["docs/**", "*.md"]

[[deployments]]
name = "production"
trigger = "release_pr_merge"
workflow = "deploy-production"
environment = "production"
release_targets = ["sdk"]
requires = ["main"]

[cli.validate]
help_text = "Validate monochange configuration and changesets"

[[cli.validate.steps]]
type = "Validate"

[cli.discover]
help_text = "Discover packages across supported ecosystems"

[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.discover.steps]]
type = "Discover"

[cli.change]
help_text = "Create a change file for one or more packages"

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

[[cli.change.inputs]]
name = "type"
type = "string"

[[cli.change.inputs]]
name = "details"
type = "string"

[[cli.change.steps]]
type = "CreateChangeFile"

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

[[workflows]]
name = "changeset-check"
help_text = "Evaluate pull-request changeset policy"

[[workflows.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[workflows.inputs]]
name = "changed_path"
type = "string_list"
required = true

[[workflows.inputs]]
name = "label"
type = "string_list"

[[workflows.steps]]
type = "EnforceChangesetPolicy"
```

<!-- {/projectSetupConfig} -->

<!-- {=projectSetupConfigNote} -->

This guide shows the preferred package/group configuration model together with the default top-level CLI commands emitted by `mc init`.

If you omit `[cli.<command>]` entries, MonoChange synthesizes the default `validate`, `discover`, `change`, and `release` commands automatically. Repositories can then customize those commands by declaring `[cli.<command>]` tables explicitly in `monochange.toml`.

<!-- {/projectSetupConfigNote} -->

Then validate and verify discovery:

<!-- {=projectDiscoverCommand} -->

```bash
mc validate
mc discover --format json
```

<!-- {/projectDiscoverCommand} -->

Use configured package ids such as `sdk-core` and group ids such as `sdk` in changesets and CLI commands.
