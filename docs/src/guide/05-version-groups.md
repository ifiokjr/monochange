# Version groups

A version group forces multiple packages to share one planned version.

```toml
[[version_groups]]
name = "sdk"
members = ["cargo/sdk-core", "packages/web-sdk"]
```

When any member releases:

- the highest required bump in the group wins
- every member in the group receives that bump
- one planned group version is calculated from the highest current member version
- dependents of newly synced members still receive propagated parent bumps
