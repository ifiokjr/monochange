# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## monochange_schema [0.1.0](https://github.com/monochange/monochange/releases/tag/monochange_schema/v0.1.0) (2026-05-07)

### Breaking Change

#### Publish durable release schema contracts

Impact: release records now use the first public durable schema header, `v = "0.1"`, and monochange rejects missing, invalid, old, or future durable schema versions instead of reading unsafe historical shapes. The new `monochange_schema` crate owns schema version parsing, release-record wire validation, committed schema assets, and the initially empty machine-readable migration changelog.

Usage: editors can use the hosted configuration schema once GitHub Pages publishes the docs, or the raw GitHub fallback immediately. Durable release records now embed the public version field instead of the internal Rust-only `schemaVersion` field:

```json
{
	"v": "0.1",
	"kind": "monochange.releaseRecord"
}
```

The `monochange_schema` package remains independently versioned from the main release group. Its crate version starts at `0.0.0` on this branch, while this major changeset gives release planning the explicit signal to publish the first crate release without changing the durable public schema version `0.1`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #396](https://github.com/monochange/monochange/pull/396) _Introduced in:_ [`563ef83`](https://github.com/monochange/monochange/commit/563ef83fa21260518ae60c972240e2f0562e9bc2)
