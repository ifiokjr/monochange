# Quickstart: Cross-Ecosystem Release Planning Foundation

## Goal

Validate that monochange can discover a mixed-ecosystem repository, calculate transitive release impact, validate config and changesets, and keep configured groups synchronized.

## Prerequisites

- `devenv shell`
- `install:all`
- a repository containing at least one supported ecosystem

## 1. Create `monochange.toml`

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true
package_type = "cargo"
changelog = "{path}/changelog.md"

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

[ecosystems.cargo]
enabled = true

[ecosystems.npm]
enabled = true

[ecosystems.deno]
enabled = true

[ecosystems.dart]
enabled = true

[cli.validate]

[[cli.validate.steps]]
type = "Validate"

[cli.discover]

[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.discover.steps]]
type = "Discover"

[cli.change]

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

[cli.release]

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"
```

## 2. Validate and verify workspace discovery

```bash
mc validate
mc discover --format json
```

## 3. Supply change input

Preferred CLI flow:

```bash
mc change --package sdk-core --bump minor --reason "public API addition"
```

Equivalent manual file:

```markdown
---
sdk-core: minor
---

#### public API addition
```

A grouped release can target the group id directly:

```markdown
---
sdk: minor
---

#### coordinated SDK release
```

## 4. Compute a release plan

```bash
mc release --dry-run --format json
mc release
```

Expected outcome:

- directly changed packages receive the requested or inferred increment
- transitive dependents receive at least the configured parent bump
- grouped packages share one planned version
- package and group metadata drive release targets
- the release command updates manifests, changelogs, and configured `versioned_files`
- consumed `.changeset/*.md` files are deleted only after a fully successful prepare run

## 5. Run repository validation

```bash
lint:all
test:all
build:all
build:book
```
