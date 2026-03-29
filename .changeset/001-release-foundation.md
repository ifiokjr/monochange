---
monochange: minor
monochange_core: minor
monochange_cargo: minor
monochange_npm: minor
monochange_config: minor
monochange_deno: minor
monochange_dart: minor
monochange_graph: minor
monochange_semver: minor
---

#### add cross-ecosystem discovery and release planning foundation

Introduce the first end-to-end release planning foundation for `monochange`. This change adds normalized package discovery across Cargo, npm/pnpm/Bun, Deno, Dart, and Flutter workspaces, along with shared core models for packages, dependency edges, version groups, change signals, and release plans.

It also adds the initial planning engine that can take explicit change input, propagate bumps through dependency relationships, synchronize grouped package versions, and incorporate Rust semver evidence when present. The CLI now ties these pieces together so repository owners can discover a workspace, create change files, and compute a release plan from one consistent toolchain.
