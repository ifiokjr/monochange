# monochange

> manage versions and releases for your multiplatform, multilanguage monorepo

`monochange` is a Rust workspace for cross-ecosystem package discovery and release planning.

Current milestone capabilities:

- discover Cargo, npm/pnpm/Bun, Deno, Dart, and Flutter packages
- normalize dependency edges across ecosystems
- coordinate shared version groups from `monochange.toml`
- compute release plans from explicit change input
- prepare synced releases through config-defined workflows
- apply Rust semver evidence when provided
- ship documentation through the mdBook in `docs/`

## Quick start

```bash
devenv shell
install:all
mc workspace discover --root . --format json
mc changes add --root . --package monochange --bump minor --reason "add release planning"
mc release --dry-run
mc release
```

Example configuration:

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

Example change input:

```markdown
---
sdk_core: minor
---

#### public API addition
```

Rust semver evidence can be attached explicitly:

```markdown
---
sdk_core: patch
evidence:
  sdk_core:
    - rust-semver:major:public API break detected
---

#### breaking API change
```

## Development

```bash
monochange --help
mc --help
lint:all
test:all
build:all
build:book
```

See `docs/` for user-facing guides and `CONTRIBUTING.md` for workflow expectations.
