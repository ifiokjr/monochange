---
monochange_hosting: none
---

#### align hosting manifest coverage with package publication metadata

`monochange_hosting` did not gain a new public API in this PR, but its manifest coverage now exercises the expanded `ReleaseManifest` shape that includes package publication metadata.

**Before:**

```rust
let manifest = ReleaseManifest {
    release_targets: vec![],
    released_packages: vec![],
    changed_files: vec![],
    ..todo!()
};
```

**After:**

```rust
let manifest = ReleaseManifest {
    release_targets: vec![],
    package_publications: vec![],
    released_packages: vec![],
    changed_files: vec![],
    ..todo!()
};
```

No direct `monochange_hosting` API behavior changed here; this note exists so the package-level compatibility update is tracked explicitly instead of being hidden inside a broader umbrella changeset.
