---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_graph: minor
monochange_cargo: none
monochange_semver: none
monochange_lint_testing: none
"@monochange/cli": none
"@monochange/skill": none
---

#### add `caused_by` changeset context for dependency propagation

You can now annotate dependency-only follow-up changesets with `caused_by`, use `mc change --caused-by ...` to author them, inspect the linkage in diagnostics output, and suppress matching automatic dependency propagation during release planning.
