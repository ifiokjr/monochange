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

[package.sdk-core]
path = "crates/sdk_core"

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

[[workflows]]
name = "validate"
help_text = "Validate monochange configuration and changesets"

[[workflows.steps]]
type = "Validate"

[[workflows]]
name = "discover"
help_text = "Discover packages across supported ecosystems"

[[workflows.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[workflows.steps]]
type = "Discover"

[[workflows]]
name = "change"
help_text = "Create a change file for one or more packages"

[[workflows.inputs]]
name = "package"
type = "string_list"
required = true

[[workflows.inputs]]
name = "bump"
type = "choice"
choices = ["patch", "minor", "major"]
default = "patch"

[[workflows.inputs]]
name = "reason"
type = "string"
required = true

[[workflows.steps]]
type = "CreateChangeFile"

[[workflows]]
name = "release"
help_text = "Prepare a release from discovered change files"

[[workflows.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[workflows.steps]]
type = "PrepareRelease"
```

<!-- {/projectSetupConfig} -->

<!-- {=projectSetupConfigNote} -->

This guide shows the preferred package/group configuration model together with the default top-level workflows emitted by `mc init`.

If you omit `[[workflows]]`, MonoChange synthesizes the default `validate`, `discover`, `change`, and `release` workflows automatically. Repositories can then customize those commands by declaring workflows explicitly in `monochange.toml`.

<!-- {/projectSetupConfigNote} -->

Then validate and verify discovery:

<!-- {=projectDiscoverCommand} -->

```bash
mc validate
mc discover --format json
```

<!-- {/projectDiscoverCommand} -->

Use configured package ids such as `sdk-core` and group ids such as `sdk` in changesets and CLI commands.
