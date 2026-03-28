# monochange

> manage versions and releases for your multiplatform, multilanguage monorepo

`monochange` is a Rust workspace for cross-ecosystem package discovery and release planning.

Current milestone capabilities:

- discover Cargo, npm/pnpm/Bun, Deno, Dart, and Flutter packages
- normalize dependency edges across ecosystems
- coordinate shared version groups from `monochange.toml`
- compute release plans from explicit change input
- apply Rust semver evidence when provided
- ship documentation through the mdBook in `docs/`

## Quick start

```bash
devenv shell
install:all
mc workspace discover --root . --format json
mc changes add --root . --package crates/monochange --bump minor --reason "add release planning"
mc plan release --root . --changes changes/1234567890-crates-monochange.toml --format json
```

Example configuration:

```toml
[defaults]
parent_bump = "patch"
include_private = false

[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk"]
```

Example change input:

```toml
[[changes]]
package = "crates/sdk_core"
bump = "minor"
reason = "public API addition"
```

Rust semver evidence can be attached explicitly:

```toml
[[changes]]
package = "crates/sdk_core"
reason = "breaking API change"
evidence = ["rust-semver:major:public API break detected"]
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
