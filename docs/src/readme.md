# Introduction

`monochange` is a cross-ecosystem release planner for monorepos.

The first milestone focuses on:

- discovering packages across Cargo, npm/pnpm/Bun, Deno, Dart, and Flutter
- building one normalized dependency graph
- coordinating shared version groups
- planning transitive releases from explicit change input
- preparing releases through config-defined workflows
- surfacing Rust semver evidence when provided

## Core workflow

Create a `monochange.toml` file:

```toml
[defaults]
parent_bump = "patch"
include_private = false

[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk"]

[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"
```

Discover the workspace:

```bash
mc workspace discover --root . --format json
```

Create a change file:

```bash
mc changes add --root . --package monochange --bump minor --reason "add release planning"
```

Preview the release workflow:

```bash
mc release --dry-run
```

Inspect the raw planner when needed:

```bash
mc plan release --root . --changes .changeset/my-change.md --format json
```

Prepare the release:

```bash
mc release
```

Validate the repository:

```bash
lint:all
test:all
build:all
build:book
```

## What the JSON output includes

Discovery output includes:

- normalized package records
- dependency edges
- version groups
- warnings

Release-plan output includes:

- per-package bump decisions
- synchronized group outcomes
- compatibility evidence
- warnings and unresolved items
