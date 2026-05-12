# Migration: mixed Cargo and npm repo

```toml
[defaults]
parent_bump = "patch"

[package.acme_core]
path = "crates/acme_core"
type = "cargo"
versioned_files = ["Cargo.toml"]

[package."@acme/cli"]
path = "packages/cli"
type = "npm"

[group.main]
packages = ["acme_core", "@acme/cli"]
tag = true
release = true
version_format = "primary"
changelog = { path = "CHANGELOG.md", format = "keep_a_changelog" }

[ecosystems.cargo]
enabled = true
lockfile_commands = ["cargo generate-lockfile"]

[ecosystems.npm]
enabled = true
lockfile_commands = ["pnpm install --lockfile-only"]
```

Validate with:

```bash
mc step:validate
mc check
mc step:discover --format json
```
