---
monochange_core: patch
monochange_schema: patch
monochange_config: patch
---

# allow boolean and numeric literals in `CliInputDefinition.default`

The JSON schema for `monochange.toml` `[cli.*.inputs]` previously rejected boolean and numeric defaults, even though the Rust deserializer already accepted them correctly.

**Before:**

```toml
[[cli.release-pr.inputs]]
name = "no_verify"
type = "boolean"
default = true # jsonschema error: "true is not of types \"null\", \"string\""
```

**After:**

The `default` field in `CliInputDefinition` now accepts `string | boolean | integer | number | null` in the generated schema. TOML like the snippet above validates cleanly, and numeric defaults such as `default = 42` are also accepted.

The internal `CliInputDefault` enum gained `Integer(i64)` and `Number(f64)` variants, and the `schemars` derive now generates a multi-type `anyOf` schema for the `default` property.
