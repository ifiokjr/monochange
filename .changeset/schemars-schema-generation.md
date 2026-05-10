---
monochange_core: minor
monochange_config: minor
monochange_schema: minor
---

# Migrate JSON Schema generation from hand-tuned templates to schemars

Schema assets (`monochange.schema.json` and `release-record.schema.json`) are now generated from the Rust type tree via the `schemars` crate, eliminating manual drift between source types and committed schemas.

### Added

- `schema` feature on `monochange_core` and `monochange_config` gating `schemars`.
- `JsonSchema` derives on `ReleaseRecord`, `RawWorkspaceConfiguration`, and their transitive types.
- `monochange_core::schema` and `monochange_config::schema` modules providing `release_record()` and `workspace_configuration()` schema generation functions.
- `xtask` binary crate providing `schema update` and `schema check` subcommands, with a `cargo xtask` alias.

### Changed

- `devenv.nix` `schema:update` / `schema:check` now invoke `cargo xtask schema update` and `cargo xtask schema check`.
- `$defs` keys use camelCase names (e.g. `packageDefinition`) via `#[schemars(rename)]` attributes.
- Release-record `schemaVersion` and `kind` emit `const` constraints instead of `default`.

### Removed

- `scripts/schema-assets.sh` shell script.
- `schemas/templates/*.schema.template.json` template files.
