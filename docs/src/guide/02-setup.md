# Setting up a project

Add a `monochange.toml` file at the repository root.

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

[[cli.change.inputs]]
name = "evidence"
type = "string_list"

[[cli.change.inputs]]
name = "output"
type = "path"

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

<!-- {/projectSetupConfig} -->

<!-- {=projectSetupConfigNote} -->

This guide shows the preferred package/group configuration model together with an expanded CLI command surface.

<!-- {/projectSetupConfigNote} -->

Then validate and verify discovery:

<!-- {=projectDiscoverCommand} -->

```bash
mc validate
mc discover --format json
```

<!-- {/projectDiscoverCommand} -->

Use configured package ids such as `sdk-core` and group ids such as `sdk` in changesets and CLI commands.
