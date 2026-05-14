# Durable schema migrations

## Status

Implementation in `refactor/durable-release-migrations`: `monochange_schema` now owns version parsing, release-record current-version validation, and a Rust-based release-record migration pipeline.

## Problem statement

monochange writes JSON artifacts that can outlive the binary that produced them. Release records are embedded in git commit messages and are effectively immutable history. Publish readiness, publish bootstrap, prepared release cache, and publish resume/result artifacts can also be persisted between workflow steps or across machines.

Those artifacts currently mix internal Rust struct layout with serialized wire shape. Some unreleased code paths already carry `kind` and `schemaVersion`, but no public release has shipped them successfully. The first public durable schema starts at `v = "0.1"`; future breaking schema changes must have explicit forward migration paths.

## Decisions captured

- Durable/public contract scope: persisted artifacts only.
- Transient CLI stdout JSON is not a durable public contract unless it is written as an artifact and later read by monochange.
- Durable artifacts must migrate only in memory; do not rewrite historical commits or artifact files while reading them.
- Missing version headers must be rejected as unsafe.
- Newer unsupported schema versions must hard-fail all commands that try to consume them.
- The version field should be named `v`.
- `v` is a string in `major.minor` form, e.g. `"0.1"` or `"8.2"`; patch versions are intentionally excluded.
- The current `v` value is owned by `monochange_schema` and intentionally decoupled from the crate package version.
- `0.1` is the first public schema version. There is no backward-compatibility obligation for unreleased `schemaVersion` artifacts.
- After `0.1`, older compatible no-op transitions still need explicit migration entries.
- JSON Schema files should be generated and committed.
- Add a new `monochange_schema` crate and keep it outside the main monochange release group.

## What “CLI JSON” means here

`--format json` currently renders many command results to stdout, for example `mc discover --format json`, `mc release --dry-run --format json`, `mc step:diagnose-changesets --format json`, `mc step:release-record --format json`, and `mc step:tag-release --dry-run --format json`.

That JSON is useful for automation, but it is different from persisted artifacts because monochange does not necessarily read it back later. For this plan, only JSON artifacts that monochange writes to disk or embeds into git history and later consumes are durable public schemas.

If a command both renders JSON to stdout and writes the same object to an artifact file, the artifact shape is covered. The stdout rendering can reuse the schema type, but migration compatibility is guaranteed for the persisted artifact path.

## Rust migration pipeline

Migration edges live in Rust so the code that documents a schema transition is also the code that applies it. A release-record migration declares its source and destination schema versions plus the function that mutates the JSON payload.

```rust
MigrationEdge {
	from: SchemaVersion::new(0, 0),
	to: SchemaVersion::new(0, 1),
	apply: migrate_0_0_to_0_1,
}
```

For no-op compatibility releases, the edge still exists and points at a validating no-op function. That keeps every older durable schema version covered by the same CI path as migrations that rewrite fields.

Implementation note: `crates/monochange_schema/src/migrations/` exposes explicit release-record migration edges. User-facing migration context belongs in changesets rather than a separate generated schema-migration asset.

## Durable schema inventory

### Release record

- Current location: `crates/monochange_core/src/lib.rs`
- Parser/render helpers: `parse_release_record_block`, `render_release_record_block`
- Pre-public fields: `kind`, `schemaVersion`, `createdAt`, `command`, release target data, changesets, package publications, provider metadata
- Public `0.1` behavior: writes `v = "0.1"`, validates `kind`, and rejects missing/non-current `v` values before bridging to the internal `RELEASE_RECORD_SCHEMA_VERSION`
- Durability: forever; embedded in release commits under `<!-- monochange:release-record:start -->`
- Migration need: highest priority

Compatibility note: no release records have shipped publicly yet. The first public schema writes and requires `v = "0.1"`; records missing `v` are rejected rather than migrated from unreleased `schemaVersion` shapes.

### Publish readiness artifact

- Current location: `crates/monochange/src/publish_readiness.rs`
- Current type: `PublishReadinessReport`
- Current header: `kind = "monochange.publishReadiness"`, `schemaVersion = 2`
- Current reader: `read_report_artifact`
- Current behavior: serde defaults currently allow missing `schemaVersion`, missing `kind`, and missing `inputFingerprint`
- Durability: persisted by `mc step:publish-readiness --output <path>` and consumed by publish planning
- Migration need: first public schema should write `v = "0.1"`; reject missing version going forward

### Publish bootstrap artifact

- Current location: `crates/monochange/src/publish_bootstrap.rs`
- Current type: `PublishBootstrapReport`
- Current header: `kind = "monochange.publishBootstrap"`, `schemaVersion = 1`
- Current writer: `write_bootstrap_artifact`
- Current reader: no primary monochange consumer found in this pass
- Durability: persisted by `mc step:placeholder-publish --output <path>` for CI/manual retry/audit notes
- Migration need: first public schema should write `v = "0.1"`; add canonical parser even if the CLI currently only writes it

### Prepared release cache artifact

- Current location: `crates/monochange/src/prepared_release_cache.rs`
- Current type: private `PreparedReleaseArtifact`
- Current path: `.monochange/prepared-release-cache.json` by default, or explicit `--prepared-release <path>`
- Current header: `schemaVersion = 1`
- Current behavior: mismatched schema version is treated as stale, not migrated
- Durability: user marked cache artifacts as forever-readable
- Migration need: first public schema should write `v = "0.1"`; default-cache freshness checks can still reject stale workspace state after successful schema validation

### Package publish resume/result artifact

- Current location: `crates/monochange/src/package_publish.rs`
- Current type: `PackagePublishReport`
- Current writer/reader: `write_publish_report_artifact`, `read_publish_report_artifact`
- Current header: none
- Current path: `mc publish --output <path>` / `mc publish --resume <path>` flow
- Durability: persisted artifact used for resume, so it matches the “only persisted artifacts” policy even though it was not in the initial answer list
- Migration need: add a schema header before making this a long-term public contract

Compatibility note: because current artifacts have no version header, the “reject missing version as unsafe” decision means older publish resume artifacts should hard-fail with an actionable rerun/resume message instead of being migrated.

### Release manifest cache / dry-run JSON

- Current default path: `.monochange/release-manifest.json`
- Current type: `ReleaseManifest`
- Current writer: `write_release_manifest_file`
- Current reader: no primary monochange reader found in this pass
- Current role: cached downstream automation payload / dry-run output
- Durability: ambiguous. It is persisted, but it appears to be generated output rather than an input monochange must read back.
- Recommendation: keep it out of the forever-readable set unless monochange starts consuming it as input. If it becomes input, move it into `monochange_schema` first.

## Proposed `monochange_schema` crate

Create `crates/monochange_schema` as the lowest-level crate for durable wire contracts.

Recommended crate responsibilities:

- own durable artifact DTOs and their serde wire shape
- own `kind` constants
- own the current public `v` constant independently from `env!("CARGO_PKG_VERSION")`
- parse and validate `major.minor` schema versions
- reject missing `v` headers for the first public schema
- migrate raw `serde_json::Value` to the current version
- deserialize migrated values into current DTOs
- generate JSON Schema files
- expose compatibility fixtures/tests

Recommended dependencies:

- runtime: `serde`, `serde_json`, `thiserror`
- optional schema generation: `schemars`
- tests: `insta`, `similar-asserts` as needed

Recommended package setup:

- add it to Cargo workspace members through existing `crates/*`
- add `[package.monochange_schema] path = "crates/monochange_schema"` to `monochange.toml`
- do **not** add `monochange_schema` to `[group.main].packages`
- give `crates/monochange_schema/Cargo.toml` its own explicit `version = "0.0.0"` instead of `version.workspace = true`, then let the changeset publish the first crate release
- configure independent tags/releases for the schema crate, likely namespaced tags such as `monochange_schema/v0.1.0`

Dependency direction options:

1. Preferred: make `monochange_schema` mostly independent and keep DTOs as wire types. Convert between schema DTOs and richer core/CLI domain types at crate boundaries.
2. Acceptable short-term: let `monochange_schema` depend on `monochange_core` for shared enums only. Avoid this if it prevents `monochange_core` from using schema types without cycles.

The cleaner long-term boundary is: `monochange_schema` owns durable wire types; `monochange_core` owns internal planning/domain types; `monochange` CLI maps between them.

## Version policy

Current schema version:

```rust
pub const CURRENT_SCHEMA_VERSION_TEXT: &str = "0.1";

pub fn current_schema_version() -> Result<SchemaVersion, SchemaVersionParseError>;
```

The implementation should not use `unwrap` or `expect` in production code; callers either use the infallible text constant for rendering, or propagate the parse error through `SchemaError`.

Rules:

- write only `v`, never `schemaVersion`, for new artifacts
- `v` must match `^\d+\.\d+$`
- `0.0.0` development crate version still writes the first public durable schema version `"0.1"`
- crate patch releases do not change `v`
- future durable breaking changes update `CURRENT_SCHEMA_VERSION_TEXT` explicitly and add Rust migration edges
- exact current `major.minor`: deserialize current DTO
- older `major.minor` after `0.1`: run every migration edge to current, including no-op edges
- newer `major.minor`: hard-fail with “unsupported schema version; upgrade monochange” guidance
- missing `v`: reject as unsafe

## Migration architecture

Proposed API shape:

```rust
pub fn parse_artifact<T: DurableArtifact>(value: serde_json::Value) -> Result<T, SchemaError>;

pub trait DurableArtifact: Sized {
	const KIND: &'static str;
	fn current_version() -> SchemaVersion;
	fn migrate(
		value: serde_json::Value,
		from: SchemaVersion,
	) -> Result<serde_json::Value, SchemaError>;
	fn deserialize_current(value: serde_json::Value) -> Result<Self, SchemaError>;
}
```

Practical artifact modules:

```text
monochange_schema::release_record
monochange_schema::publish_readiness
monochange_schema::publish_bootstrap
monochange_schema::prepared_release
monochange_schema::package_publish
monochange_schema::json_schema
monochange_schema::migration_log
```

Each module should expose:

- `KIND`
- `CURRENT_VERSION`
- current DTO types
- `parse_json_value`
- `parse_str`
- `to_json_value`
- `to_string_pretty`
- migration fixtures
- JSON Schema generator entry

## JSON Schema files

Commit generated schemas under stable paths. Normal schema generation maintains only moving current aliases plus deterministic current artifact fixtures:

```text
crates/monochange_schema/schemas/release-record.schema.json
crates/monochange_schema/schemas/monochange.schema.json
crates/monochange_schema/schemas/artifacts/current/release-record/{1..10}.json
crates/monochange_schema/schemas/artifacts/current/monochange/{1..10}.json
docs/src/schemas/release-record.schema.json
docs/src/schemas/monochange.schema.json
```

Release schema generation additionally writes immutable versioned schema copies:

```text
docs/src/schemas/release-record.v{version}.schema.json
docs/src/schemas/monochange.v{version}.schema.json
```

Generation is deterministic and checked by tests plus devenv scripts:

```text
schema:update
schema:check
schema:release:update
schema:release:check
```

`fix:all` runs `schema:update`; `lint:all` runs `schema:check`. Release automation uses `schema:release:update` after `PrepareRelease`, and release preflight uses `schema:release:check`.

## Implementation checklist

### Commit 1 — crate scaffold

- Add `crates/monochange_schema`.
- Add explicit independent crate version.
- Add workspace dependency entry.
- Add monochange package entry outside `group.main`.
- Add readme documenting durable schema guarantees.

### Commit 2 — version/header foundation

- Implement `SchemaVersion` parser/rendering for `major.minor` strings.
- Implement artifact header detection for `kind` and `v`.
- Implement `SchemaError` with missing, malformed, unsupported-newer, unsupported-kind, and migration-failed cases.
- Add tests for current/newer/missing versions; older-version tests begin once `0.2` introduces a migration edge.

### Commit 3 — release record migration

- Move or mirror release record DTOs into `monochange_schema::release_record`.
- Establish `v = "0.1"` as the first public release-record schema.
- Wire `render_release_record_block` to write `v`.
- Wire release-record discovery to parse via schema migration.
- Keep future historical release commits readable in memory only once migrations exist.
- Add fixtures for current v, missing version, and future v.

### Commit 4 — readiness/bootstrap artifacts

- Move `PublishReadinessReport` and `PublishBootstrapReport` wire DTOs into `monochange_schema`.
- Remove serde defaults that accept missing versions/kinds for durable artifact parsing.
- Establish `v = "0.1"` as the first public readiness/bootstrap schema.
- Add canonical parsers for bootstrap artifacts even if currently only written.

### Commit 5 — prepared release cache

- Move prepared release cache wire DTO into `monochange_schema` or wrap it with schema parsing there.
- Establish `v = "0.1"` as the first public prepared-release cache schema.
- Keep freshness validation separate from schema migration.
- Default cache may still be ignored when stale; explicit cache should fail only after parse/migration succeeds and freshness checks fail.

### Commit 6 — package publish resume/result artifact

- Add `kind` and `v` to `PackagePublishReport`.
- Reject no-version resume artifacts with a clear rerun message, per missing-version policy.
- Add current schema fixtures.

### Commit 7 — JSON Schema generation

- Add `schemars` derives or manual schema generation.
- Generate committed schema files for every current durable artifact.
- Add tests that fail when committed schemas are stale.

### Commit 8 — compatibility guardrails

- Add a fixture directory such as `crates/monochange_schema/fixtures/<artifact>/<version>/*.json`.
- Add tests proving every fixture migrates to current.
- Add tests proving future versions hard-fail.
- Add tests proving no-op migration edges are still executed/registered.
- Add docs explaining artifact support guarantees.

## Validation

Recommended commands after implementation:

```text
cargo test -p monochange_schema
cargo test -p monochange_core
cargo test -p monochange
mc step:validate
lint:all
```

Run the repository’s full validation once the refactor is complete:

```text
fix:all
build:all
lint:all
```

## Open decisions

- Should package publish resume/result artifacts be explicitly included now? This plan recommends yes because they are persisted and read back by monochange.
- Should release manifest cache files be excluded from forever compatibility until monochange consumes them as input? This plan recommends excluding them for now.
- Should fixture-backed migration functions be required before each durable schema version bump?
