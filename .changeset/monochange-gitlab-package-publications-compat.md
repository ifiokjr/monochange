---
monochange_gitlab: none
---

#### align GitLab release fixtures with package publication metadata

`monochange_gitlab` did not gain a new outward API in this PR, but its release-manifest fixture coverage now includes the expanded `package_publications` field from `monochange_core`.

**Before:**

```rust
let manifest = ReleaseManifest {
    release_targets: vec![target],
    released_packages: vec!["workflow-core".to_string()],
    changed_files: vec![PathBuf::from("Cargo.toml")],
    ..todo!()
};
```

**After:**

```rust
let manifest = ReleaseManifest {
    release_targets: vec![target],
    package_publications: vec![],
    released_packages: vec!["workflow-core".to_string()],
    changed_files: vec![PathBuf::from("Cargo.toml")],
    ..todo!()
};
```

No direct GitLab publish or release-request behavior changed in this crate; this changeset records the package-scoped compatibility update instead of bundling it into a larger multi-package note.
