---
"monochange_config": feat
---

#### support lints field in ecosystem configuration

Add `lints` field to `RawEcosystemSettings` and pass it through to `EcosystemSettings` during configuration normalization. This enables `[ecosystem.<name>.lints]` sections in `monochange.toml`.
