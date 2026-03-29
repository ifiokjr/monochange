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

[package.sdk-core]
path = "crates/sdk_core"
type = "cargo"

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

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
```

## 2. Validate and verify workspace discovery

```bash
mc check --root .
mc workspace discover --root . --format json
```

## 3. Supply change input

Preferred CLI flow:

```bash
mc changes add --root . --package sdk-core --bump minor --reason "public API addition"
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
mc plan release --root . --changes .changeset/my-change.md --format json
mc release --dry-run
mc release
```

Expected outcome:

- directly changed packages receive the requested or inferred increment
- transitive dependents receive at least the configured parent bump
- grouped packages share one planned version
- package and group metadata drive release targets
- the workflow updates manifests, changelogs, and configured `versioned_files`
- consumed `.changeset/*.md` files are deleted only after a fully successful prepare run

## 5. Run repository validation

```bash
lint:all
test:all
build:all
build:book
```
