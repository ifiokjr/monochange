---
monochange: minor
monochange_cargo: minor
monochange_config: minor
monochange_core: minor
monochange_dart: minor
monochange_deno: minor
monochange_go: minor
monochange_npm: minor
monochange_python: minor
---

# Move ecosystem constants out of core and delegate validation to ecosystem crates

Each ecosystem crate now owns its own `default_dependency_version_prefix()`, `default_dependency_fields()`, and `validate_versioned_file()` functions. The `EcosystemType::default_prefix()` and `EcosystemType::default_fields()` methods on `monochange_core::EcosystemType` are deprecated in favor of the ecosystem crate equivalents. `monochange_config` versioned file validation now dispatches to ecosystem crate validators instead of embedding ecosystem-specific parsing logic in config.

Closes #137 Closes #138
