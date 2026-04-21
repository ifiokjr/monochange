---
"@monochange/skill": docs
---

#### add modular skill docs and a full linting guide

The packaged `@monochange/skill` docs now split the agent guidance into focused deep dives while keeping `REFERENCE.md` at the top level as the high-context reference document.

**Before:**

The package centered on `SKILL.md` plus a few top-level docs, but it did not have a dedicated `skills/` folder for focused topics and it did not explain the current workspace lint policy rule by rule.

**After:**

The package now includes:

- `skills/changesets.md` for creating and managing `.changeset/*.md` files
- `skills/commands.md` for choosing the right `mc` command and command flow
- `skills/configuration.md` for creating and extending `monochange.toml`
- `skills/linting.md` for the current rust/clippy rules, why they exist, and what changes with and without them
- updated `SKILL.md` and `REFERENCE.md` links so agents can jump between the concise entrypoint and the deeper reference material

**Skill bundle example:**

```text
SKILL.md
REFERENCE.md
skills/
  README.md
  changesets.md
  commands.md
  configuration.md
  linting.md
```

This makes the published skill package easier to load incrementally while giving agents a much denser reference surface for current monochange features.
