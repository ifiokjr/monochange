---
"@monochange/skill": docs
---

#### add granular changeset generation guidance to the skill package

The packaged `@monochange/skill` guidance now teaches agents how to manage changesets as a lifecycle instead of only telling them to create one.

**Before:**

The skill told agents to use `mc change` and `mc diagnostics`, but it did not explain when to create a new changeset versus updating, replacing, or removing an existing one. It also did not make granular package-centric changesets a first-class rule.

**After:**

Agents are now instructed to:

- review existing `.changeset/*.md` files before writing a new one
- keep changesets package-centric and split unrelated features apart
- combine near-duplicate changesets when the outward change is the same across multiple related packages
- update an existing changeset only when the same feature expands in scope
- remove stale changesets when a feature is reverted before release
- dedicate separate changesets to breaking changes with migration guidance

**Skill guidance example:**

```markdown
# Separate unrelated features

---
core: minor
---

#### add file diff preview

...

---
core: minor
---

#### add changelog format detection

...
```

This makes the packaged skill better aligned with monochange's current agent rules for granular, user-facing release notes.
