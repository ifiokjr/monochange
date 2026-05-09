---
monochange: minor
monochange_core: minor
monochange_config: minor
monochange_cargo: minor
monochange_dart: minor
monochange_deno: minor
monochange_go: minor
monochange_npm: minor
monochange_python: minor
---

# Consolidate adapter traits to remove ecosystem match arms

Replace hardcoded ecosystem match arms in `workspace_ops` and `monochange_config` with `EcosystemRegistry` dispatch via the `EcosystemAdapter` trait.

- Expand `EcosystemAdapter` in `monochange_core` with `load_configured`, `supported_versioned_file_kind`, and `validate_versioned_file`.
- Add `From<EcosystemType>` and `From<PackageType>` conversions for `Ecosystem`.
- Implement the new trait methods in all ecosystem adapter crates.
- Replace `discover_packages` body with `build_ecosystem_registry().discover_all(root)?`.
- Replace `discover_release_workspace` `load_configured` match arms with registry dispatch.
- Replace `path_is_supported_for_ecosystem` and `validate_ecosystem_version_readable` match arms in `monochange_config` with registry dispatch.
