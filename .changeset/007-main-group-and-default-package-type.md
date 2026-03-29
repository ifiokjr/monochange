---
monochange: minor
monochange_config: minor
monochange_core: minor
---

#### add default package types and simplify the main release group

Add `defaults.package_type` so single-ecosystem repositories can omit `type` from `[package.<id>]` entries when a default package type is configured. This update also renames the repository's release group to `main`, keeps per-package changelogs on lowercase `changelog.md` files, removes path-style changeset targets, and fixes the docs-release workflow so mdBook deployment does not depend on the broken Nix evaluation path seen on main.
