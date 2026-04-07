---
main: major
---

#### redesign authored changeset target metadata

> **Breaking change** — authored changesets now use package or group ids as the only top-level frontmatter keys. Remove legacy `type`, `origin`, and `evidence` top-level blocks and move target-specific metadata onto each target value instead.

**Before:**

```markdown
---
sdk-core: patch
type:
  sdk-core: security
evidence:
  sdk-core:
    - rust-semver:major:public API break detected
---

#### rotate signing keys
```

**After:**

```markdown
---
sdk-core:
  bump: patch
  type: security
---

#### rotate signing keys
```

If a configured changelog section declares `default_bump`, you can also use scalar type shorthand:

```toml
[package.sdk-core]
extra_changelog_sections = [
	{ name = "Security", types = ["security"], default_bump = "patch" },
]
```

```markdown
---
sdk-core: security
---
```

`mc change` now accepts `--bump none` for type-only and version-only entries, and the legacy `--evidence` flag is no longer accepted.
