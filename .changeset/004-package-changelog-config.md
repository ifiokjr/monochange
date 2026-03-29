---
monochange: patch
monochange_core: patch
monochange_cargo: patch
monochange_npm: patch
monochange_config: patch
monochange_deno: patch
monochange_dart: patch
monochange_graph: patch
monochange_semver: patch
---

#### add synced package changelog configuration

Add a real repository-level `monochange.toml` with explicit package-level release configuration. Each workspace crate now has a configured changelog location so release preparation can append release notes to the correct file instead of relying on implicit conventions.

This change also defines a single synced version group for the releaseable workspace crates. With that configuration in place, releases can keep the core packages aligned on one shared version while still preserving per-package changelog output.
