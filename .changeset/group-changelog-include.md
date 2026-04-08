---
main: minor
---

#### add group changelog include filters

You can now keep a group changelog focused on the packages that define the group's public surface without changing package changelogs or release planning.

**Before:** every member-targeted changeset in a group could flow into the group changelog.

```toml
[group.main.changelog]
path = "changelog.md"
```

**After:** grouped changelogs can opt into direct group-only notes or an allowlist of member packages.

```toml
[group.main.changelog]
path = "changelog.md"
include = "group-only"
# or: include = ["cli"]
```

Direct group-targeted changesets are always included. Member-targeted changesets still affect synchronized group versions and package changelogs, but monochange now filters them out of the group changelog unless the group changelog `include` policy allows them.
