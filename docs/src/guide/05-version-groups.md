# Groups and shared release identity

A configured group forces multiple packages to share one planned version and one outward release identity.

<!-- {=versionGroupsExample} -->

```toml
[package.sdk-core]
path = "cargo/sdk-core"
type = "cargo"

[package.web-sdk]
path = "packages/web-sdk"
type = "npm"

[group.sdk]
packages = ["sdk-core", "web-sdk"]
tag = true
release = true
version_format = "primary"
```

<!-- {/versionGroupsExample} -->

When any member releases:

<!-- {=versionGroupsBehavior} -->

- the highest required bump in the group wins
- every member in the group receives that bump
- one planned group version is calculated from the highest current member version
- the group owns outward release identity
- member package changelogs can still be updated individually
- group changelog and group `versioned_files` can also be updated
- grouped packages can use `empty_update_message` when their own changelog needs a version-only update with no direct notes
- dependents of newly synced members still receive propagated parent bumps
- unmatched members (not found during discovery) produce warnings; unresolvable members (invalid IDs) produce errors
- mismatched current versions produce warnings when `warn_on_group_mismatch = true`

<!-- {/versionGroupsBehavior} -->

A changeset may reference the group id:

```markdown
---
sdk: minor
---

#### coordinated SDK release
```

But a changeset may not reference both the group id and one of its members in the same file.

To keep a group changelog focused on public surfaces while leaving package changelogs detailed, configure the grouped changelog table:

```toml
[group.sdk.changelog]
path = "changelog.md"
include = ["sdk-cli"]
```

Direct group-targeted changesets are always included. Member-targeted changesets are filtered only for the group changelog; package changelogs and release planning remain unchanged.

<!-- {=versionGroupsCurrentStatus} -->

Legacy `version_groups.strategy` is no longer the primary authoring model. The current implementation always derives synchronized release behavior from `[group.<id>]` declarations.

<!-- {/versionGroupsCurrentStatus} -->
