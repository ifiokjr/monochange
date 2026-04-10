---
monochange: minor
monochange_config: minor
---

#### allow group and member packages in the same changeset

A changeset can now reference both a group and one or more of its member packages. Previously this was rejected with an error.

**Before:**

```markdown
---
sdk: minor
core: patch
---
```

> error: changeset references both group `sdk` and member package `core`

**After:**

The changeset is accepted. Members explicitly listed (like `core: patch`) get their own bump level as a direct signal. Members not explicitly listed (like `web`) receive the group's bump level (`minor`) through group expansion.

This lets you express "release the whole group at minor, but record that core specifically had a patch-level change" in a single changeset file. Group synchronization during release planning still aligns final versions across all members.

The interactive `mc change` wizard also now allows selecting both a group and its members.
