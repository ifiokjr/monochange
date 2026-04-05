---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_cargo: minor
monochange_dart: minor
monochange_deno: minor
monochange_graph: minor
monochange_npm: minor
monochange_semver: minor
---

#### add default package types and simplify the main release group

`defaults.package_type` lets single-ecosystem repositories omit the `type` field from every `[package.<id>]` entry:

```toml
# before – type required on every entry
[package.core]
path = "crates/core"
type = "cargo"

[package.cli]
path = "crates/cli"
type = "cargo"

# after – set it once in defaults
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[package.cli]
path = "crates/cli"
```

Per-package changelog files now default to lowercase `changelog.md` rather than `CHANGELOG.md`. Path-style changeset targets (e.g. `crates/core: minor`) are no longer accepted; use the package id instead. Both the config parser and the changeset validator enforce this.

**`monochange_config`** exposes the new `defaults.package_type` field. **`monochange_core`** gains the `PackageType` enum and the `defaults` propagation logic used by all ecosystem adapters.
