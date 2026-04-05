---
monochange: patch
monochange_cargo: patch
monochange_config: patch
monochange_core: patch
monochange_dart: patch
monochange_deno: patch
monochange_graph: patch
monochange_npm: patch
monochange_semver: patch
---

#### document discovery, release planning, and contributor workflow

Expand the project documentation to cover the full first milestone workflow with concrete CLI examples. The updated guides walk through every step a new contributor needs:

```bash
# discover all packages across ecosystems
mc workspace discover --root . --format json

# create a change file for a specific package
mc changes add --root . --package my-lib --bump minor --reason "add new helper"

# inspect the release plan before committing
mc plan release --root . --changes .changeset/*.md --format json

# execute the release
mc release --dry-run
```

The mdBook guides were updated to document how `monochange.toml` configures packages and groups, how the graph propagation works when a dependency bumps its version, and how version-group synchronization keeps multi-ecosystem packages in step with each other.
