---
monochange: minor
monochange_core: minor
monochange_config: minor
monochange_cargo: minor
monochange_npm: minor
monochange_dart: minor
monochange_deno: minor
---

#### add lockfile-aware versioned_files defaults and shorthand

Support package-scoped string shorthand for versioned_files, ecosystem-level default versioned_files inheritance with per-package opt-out, Deno ecosystem typing, lockfile auto-discovery, explicit lockfile overrides, and readable validation for typed globs that match unsupported file types. Move ecosystem-specific file support and lockfile discovery/update knowledge into the ecosystem crates so the workspace layer only dispatches by ecosystem.
