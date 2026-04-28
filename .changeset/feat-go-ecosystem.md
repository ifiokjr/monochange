---
"@monochange/cli": patch
"@monochange/skill": patch
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_go: minor
monochange_lint: patch
---

#### add Go ecosystem support

monochange now discovers and manages Go modules from `go.mod` files in single-module and multi-module repositories.

**Configuration:**

```toml
[defaults]
package_type = "go"

[package.api]
path = "api"

[package.shared]
path = "shared"

[ecosystems.go]
enabled = true
```

**What it discovers:**

- Go modules by scanning for `go.mod` files
- Multi-module monorepos with separate modules in subdirectories
- Module paths, including major version suffixes (`/v2`, `/v3`)
- Cross-module `require` directives as dependency edges
- Indirect dependencies marked as development dependencies

**Version management:**

- Go versions come from git tags, not manifest files — the adapter reports `None` for `current_version` and stores the module path as metadata for tag resolution
- Updates `require` directives in `go.mod` when cross-module dependencies change
- Preserves `replace`, `exclude`, `retract` directives and comments
- Adds `v` prefix to version strings automatically when missing

**Lockfile commands:**

- Infers `go mod tidy` for all Go modules (updates both `go.mod` and `go.sum`)
- Configurable via `[ecosystems.go].lockfile_commands`

**Key design decisions:**

- Module names are derived from the last non-version segment of the module path (`github.com/org/repo/api/v2` → `api`)
- The full module path and relative directory path are stored as metadata for downstream tag resolution
- Parse errors during discovery are treated as warnings, not hard errors
