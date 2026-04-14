---
monochange_github: patch
---

#### update GitHub release examples for package publication metadata

The `monochange_github` crate documentation now shows the current `ReleaseManifest` shape when you build GitHub release requests manually.

**Before:**

```rust
let manifest = ReleaseManifest {
    release_targets: vec![target],
    released_packages: vec!["workflow-core".to_string()],
    changed_files: Vec::new(),
    changesets: Vec::new(),
    changelogs: Vec::new(),
    deleted_changesets: Vec::new(),
    plan,
    ..todo!()
};
```

**After:**

```rust
let manifest = ReleaseManifest {
    release_targets: vec![target],
    released_packages: vec!["workflow-core".to_string()],
    package_publications: Vec::new(),
    changed_files: Vec::new(),
    changesets: Vec::new(),
    changelogs: Vec::new(),
    deleted_changesets: Vec::new(),
    plan,
    ..todo!()
};
```

If you construct `ReleaseManifest` values yourself before calling `build_release_requests`, the docs.rs example now matches the expanded manifest type from `monochange_core`.
