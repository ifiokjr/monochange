---
monochange_gitea: change
monochange_github: change
monochange_gitlab: change
monochange_hosting: change
---

#### align provider and hosting release examples with package publication metadata

The hosting/provider crates in this PR all moved together around the same outward shape change: `ReleaseManifest` now carries `package_publications`, and the provider-facing examples and compatibility fixtures now show that field consistently.

**Before:**

```rust
let manifest = ReleaseManifest {
    release_targets: vec![target],
    released_packages: vec!["workflow-core".to_string()],
    changed_files: Vec::new(),
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
    ..todo!()
};
```

`monochange_github` updates its public example to match the new manifest shape, while `monochange_hosting`, `monochange_gitlab`, and `monochange_gitea` now exercise the same field in their compatibility coverage instead of lagging behind `monochange_core`.
