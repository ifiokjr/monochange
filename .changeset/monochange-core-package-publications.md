---
monochange_core: minor
---

#### add package publication targets and publish step definitions

`monochange_core` now models package publication as first-class release metadata instead of leaving registry publication outside the release graph.

**Before:**

```rust
use monochange_core::CliStepDefinition;
use monochange_core::ReleaseManifest;

let step = CliStepDefinition::PublishRelease { /* ... */ };

let manifest = ReleaseManifest {
    release_targets: vec![],
    released_packages: vec![],
    changed_files: vec![],
    // no package publication metadata
    ..todo!()
};
```

**After:**

```rust
use monochange_core::CliStepDefinition;
use monochange_core::PackagePublicationTarget;
use monochange_core::PublishMode;
use monochange_core::RegistryKind;
use monochange_core::ReleaseManifest;

let step = CliStepDefinition::PublishPackages { /* ... */ };

let manifest = ReleaseManifest {
    package_publications: vec![PackagePublicationTarget {
        package: "core".to_string(),
        ecosystem: monochange_core::Ecosystem::Cargo,
        registry: Some(monochange_core::PublishRegistry::Builtin(
            RegistryKind::CratesIo,
        )),
        version: "1.2.3".to_string(),
        mode: PublishMode::Builtin,
        trusted_publishing: Default::default(),
    }],
    ..todo!()
};
```

New public types include:

- `PublishMode`
- `RegistryKind`
- `PublishRegistry`
- `PlaceholderSettings`
- `TrustedPublishingSettings`
- `PublishSettings`
- `PackagePublicationTarget`

`PackageDefinition`, `EcosystemSettings`, `ReleaseManifest`, and `ReleaseRecord` now all carry publish metadata, and the built-in CLI command set includes `placeholder-publish` and `publish` step definitions.
