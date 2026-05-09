---
"monochange_schema": major
"monochange": minor
"monochange_core": minor
"monochange_test_helpers": minor
---

# Rename public release record field from `v` to `schemaVersion`

All public durable schemas now use `schemaVersion` (a `"major.minor"` string) instead of `v`:

- `monochange_schema` release record shape: field name changed from `v` to `schemaVersion`
- `monochange_core` parsing: handles legacy `v` and integer `schemaVersion` via normalization
- Integration tests, JSON schemas, and snapshots updated accordingly

## Before

```json
{
	"v": "0.1",
	"kind": "monochange.releaseRecord"
}
```

## After

```json
{
	"schemaVersion": "0.1",
	"kind": "monochange.releaseRecord"
}
```
