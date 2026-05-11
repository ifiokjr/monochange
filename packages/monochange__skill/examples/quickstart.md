# Quickstart: npm packages

```bash
mc init
mc validate
mc step:discover --format json
```

```toml
[defaults]
package_type = "npm"
parent_bump = "patch"

[package."@acme/api"]
path = "packages/api"

[package."@acme/ui"]
path = "packages/ui"

[ecosystems.npm]
enabled = true
lockfile_commands = ["pnpm install --lockfile-only"]
```

Create release intent, then preview:

```bash
mc step:create-change-file --package @acme/api --bump minor --reason "Add webhook filters"
mc validate
mc step:prepare-release --dry-run --format json
```
