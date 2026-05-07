# `monochange_schema`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_schema"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange**schema-orange?logo=rust)](https://crates.io/crates/monochange_schema) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange**schema-1f425f?logo=docs.rs)](https://docs.rs/monochange_schema/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_schema)](https://codecov.io/gh/monochange/monochange?flag=monochange_schema) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

`monochange_schema` owns durable JSON wire contracts for monochange artifacts that are embedded in git history or written to disk for later monochange commands.

Reach for this crate when you need to render, validate, or migrate public artifact versions without depending on the higher-level release planner.

## Why use it?

- keep durable wire schemas separate from internal Rust structs
- parse schema versions in the public `major.minor` format written as `v`
- validate commit-embedded release records before the CLI deserializes them into domain types
- publish machine-readable migration changelog metadata next to the schema assets

## Version policy

The crate package version and durable artifact schema version are intentionally independent.

- The crate starts at `0.0.0` on development branches so release planning can explicitly publish the first crate release.
- Durable release records already use public schema version `0.1` because `0.1` is the first supported wire contract.
- Patch releases of this crate do not change a durable `v` value.
- Future breaking durable schema changes add a new `major.minor` value plus migration changelog entries.

## Public schema assets

Current moving aliases are published with the documentation site:

- <https://monochange.github.io/monochange/schemas/release-record.schema.json>
- <https://monochange.github.io/monochange/schemas/monochange.schema.json>

Stable versioned copies are also generated for durable/editor integrations that need a non-moving URL, starting with `release-record.v0.1.schema.json` and `monochange.v0.1.schema.json`.

Run the repository scripts when changing schema templates or public constants:

```bash
schema:update
schema:check
```

`schema:update` regenerates committed schema assets from the templates and Rust wire constants. `schema:check` compares the generated output against the committed files and is part of `lint:all`.

## Example

```rust
use monochange_schema::CURRENT_SCHEMA_VERSION_TEXT;
use monochange_schema::release_record;
use serde_json::json;

let durable = release_record::render_current_value(json!({
    "schemaVersion": 1,
    "kind": release_record::KIND,
    "createdAt": "2026-04-06T12:00:00Z",
    "command": "release-pr",
    "releaseTargets": [],
    "releasedPackages": [],
    "changedFiles": []
}))?;

assert_eq!(durable["v"], CURRENT_SCHEMA_VERSION_TEXT);
assert!(durable.get("schemaVersion").is_none());
# Ok::<(), monochange_schema::SchemaError>(())
```

## Scope

- `SchemaVersion` parsing and rendering
- `release_record` durable wire validation and rendering helpers
- `migration_changelog` structured migration metadata
- committed JSON Schema and migration changelog assets under `schemas/`
