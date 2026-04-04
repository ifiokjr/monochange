---
monochange: minor
monochange_core: minor
monochange_config: minor
---

#### ecosystem-typed versioned_files with glob, prefix, and npm/dart manifest updates

Replace bare-string and dependency-keyed versioned_files with a single typed struct: `{ path, type, prefix, fields, name }`. Add `EcosystemType` enum (`cargo`/`npm`/`dart`) with per-ecosystem default fields and prefixes. Add glob support for paths. Add `dependency_version_prefix` to `EcosystemSettings`. Add `build_npm_manifest_updates` and `build_dart_manifest_updates` for own-version updates.
