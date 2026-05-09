---
monochange: patch
monochange_cargo: minor
---

# Move Cargo placeholder manifests into monochange_cargo

Move crates.io placeholder `Cargo.toml` and `src/lib.rs` generation out of `monochange::package_publish` and into `monochange_cargo`.
