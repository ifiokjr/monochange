---
"monochange_schema": patch
"monochange_config": patch
---

# Derive schema version from package metadata

The schema crate now derives the durable schema version from its Cargo package version's major/minor components at compile time, keeping the public schema version aligned without a build script.
