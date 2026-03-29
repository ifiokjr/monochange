<!-- {=monochangeCoreCrateDocs} -->

# `monochange_core`

Shared domain types for `monochange`.

This crate defines:

- normalized package and dependency records
- version-group definitions and planned group outcomes
- change signals and compatibility assessments
- release-plan domain types
- shared error and result types

## Example

```rust
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;
use std::path::PathBuf;

let package = PackageRecord::new(
    Ecosystem::Cargo,
    "demo",
    PathBuf::from("crates/demo/Cargo.toml"),
    PathBuf::from("."),
    Some(Version::new(1, 2, 3)),
    PublishState::Public,
);

assert_eq!(package.name, "demo");
assert_eq!(package.current_version, Some(Version::new(1, 2, 3)));
```

<!-- {/monochangeCoreCrateDocs} -->
