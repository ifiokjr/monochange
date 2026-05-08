---
"monochange_schema": patch
"monochange_config": patch
---

# Manage schema package version with monochange regex

The schema crate now exposes a monochange-managed full package version constant and derives the durable schema version from its major/minor components at compile time. Release planning updates the source constant through a regex `versioned_files` rule, keeping the public schema version aligned without a build script.
