# Quickstart: Cross-Ecosystem Release Planning Foundation

## Goal

Validate that monochange can discover a mixed-ecosystem repository, calculate transitive release impact, and keep version groups synchronized without GitHub bot automation.

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

[[version_groups]]
name = "sdk"
members = ["packages/web-sdk", "packages/mobile-sdk", "crates/sdk_core"]

[ecosystems.cargo]
enabled = true

[ecosystems.npm]
enabled = true

[ecosystems.deno]
enabled = true

[ecosystems.dart]
enabled = true
```

## 2. Verify workspace discovery

```bash
mc workspace discover --root . --format json
```

Expected outcome:

- all intended packages are listed once
- each package includes ecosystem identity and manifest location
- dependency edges are present
- version-group membership is visible for grouped packages
- warnings are emitted for invalid glob matches or mismatched grouped versions

## 3. Supply change input

Preferred CLI flow:

```bash
mc changes add --root . --package crates/sdk_core --bump minor --reason "public API addition"
```

Equivalent manual file:

```toml
[[changes]]
package = "crates/sdk_core"
bump = "minor"
reason = "public API addition"
```

For a Rust compatibility escalation example:

```toml
[[changes]]
package = "crates/sdk_core"
reason = "breaking API change"
evidence = ["rust-semver:major:public API break detected"]
```

## 4. Compute a release plan

```bash
mc plan release --root . --changes changes.toml --format json
```

Expected outcome:

- directly changed packages receive the requested or inferred increment
- transitive dependents receive at least the configured parent bump
- grouped packages share one planned version
- compatibility evidence appears in the output when supplied

## 5. Run repository validation

```bash
lint:all
test:all
build:all
build:book
```

## 6. Implementation notes

- package references in change files can use ids, names, relative manifest paths, or relative package directories
- Rust semver escalation is explicit in this milestone and is driven by `evidence` entries such as `rust-semver:major:...`
- version-group synchronization can create additional releases that then propagate parent bumps to their dependents

## 7. Migration considerations

- existing repositories can start with discovery only, then add change files and version groups incrementally
- teams migrating from ecosystem-specific tooling should keep native workspace manifests unchanged and layer `monochange.toml` on top
- GitHub bot automation remains intentionally out of scope for this first delivery
