---
monochange: minor
monochange_core: minor
monochange_cargo: minor
monochange_npm: minor
monochange_config: minor
monochange_deno: minor
monochange_dart: minor
monochange_graph: minor
monochange_semver: minor
---

#### replace legacy config with package/group release model

Migrate `monochange.toml` from legacy version-group and package-override configuration to explicit package and group declarations. This update also adds `mc check`, validates changesets against configured ids, and carries group-owned release identity through release preparation, changelogs, versioned files, and docs.
