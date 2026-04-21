---
monochange_cargo: change
monochange_config: change
monochange_core: change
monochange_dart: change
monochange_deno: change
monochange_gitea: change
monochange_github: change
monochange_gitlab: change
monochange_graph: change
monochange_hosting: change
monochange_npm: change
monochange_semver: change
---

#### move crate include lists into published manifests

The published library crates in this workspace now declare their `include` file lists in each crate's own `Cargo.toml` instead of inheriting that setting from `[workspace.package]`.

**Before (`crates/monochange_core/Cargo.toml`):**

```toml
[package]
include = { workspace = true }
readme = "readme.md"
```

The package contents depended on the root workspace manifest carrying:

```toml
[workspace.package]
include = ["src/**/*.rs", "Cargo.toml", "readme.md"]
```

**After:**

```toml
[package]
include = ["src/**/*.rs", "Cargo.toml", "readme.md"]
readme = "readme.md"
```

This keeps each published crate self-contained when packaging, auditing, or updating manifest metadata and avoids relying on a shared workspace-level `include` definition for crates.io package contents.
