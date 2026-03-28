# Release planning

Create a change input file with the CLI:

```bash
mc changes add --root . --package sdk_core --bump minor --reason "public API addition"
```

Or write one manually:

```markdown
---
sdk_core: minor
---

#### public API addition
```

Optionally include Rust semver evidence:

```markdown
---
sdk_core: patch
evidence:
  sdk_core:
    - rust-semver:major:public API break detected
---

#### breaking API change
```

Generate a plan directly when you want to inspect the raw planner output:

```bash
mc plan release --root . --changes .changeset/my-change.md --format json
```

Preferred repository workflow:

```bash
mc release --dry-run
mc release
```

The workflow reads `.changeset/*.md`, computes the synced release, updates manifests, appends per-package changelog entries, and deletes consumed changesets only after a successful run.

Planning rules in this milestone:

- direct changes default to `patch` when no explicit bump is supplied
- dependents default to the configured `parent_bump`
- Rust semver evidence can escalate both the changed crate and its dependents
- version-group synchronization runs before final output is rendered
