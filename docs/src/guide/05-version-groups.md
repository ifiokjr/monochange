# Version groups

A version group forces multiple packages to share one planned version.

<!-- {=versionGroupsExample} -->

```toml
[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk"]
strategy = "shared"
```

<!-- {/versionGroupsExample} -->

When any member releases:

<!-- {=versionGroupsBehavior} -->

- the highest required bump in the group wins
- every member in the group receives that bump
- one planned group version is calculated from the highest current member version
- dependents of newly synced members still receive propagated parent bumps
- unmatched members produce warnings during discovery
- mismatched current versions produce warnings when `warn_on_group_mismatch = true`

<!-- {/versionGroupsBehavior} -->

<!-- {=versionGroupsCurrentStatus} -->

`strategy` is parsed from config, but the current implementation always applies shared synchronized versioning behavior.

<!-- {/versionGroupsCurrentStatus} -->
