---
monochange: minor
monochange_core: minor
---

# Consolidate adapter traits to remove ecosystem match arms

Replace hardcoded ecosystem discovery match arms in `workspace_ops::discover_packages` with `EcosystemRegistry` dispatch. This is the foundation for removing all remaining ecosystem/provider match arms from `monochange` in favor of adapter registries.

- Add `EcosystemRegistry` to `monochange_core` with `push_adapter` and `discover_all` methods.
- Replace `discover_packages` body with `build_ecosystem_registry().discover_all(root)?`.
