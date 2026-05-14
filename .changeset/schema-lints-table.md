---
monochange_config: patch
monochange_schema: patch
---

# Add lints to the monochange config schema

Allow the top-level `[lints]` table in generated `monochange.toml` JSON Schema assets. The lint configuration schema is intentionally permissive so all current and future lint rule shapes are accepted by editors and TOML language servers.
