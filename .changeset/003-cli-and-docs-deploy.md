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

#### add CLI change-file scaffolding and deploy mdBook docs automatically

`mc changes add` now scaffolds a ready-to-commit changeset file in `.changeset/`:

```bash
mc changes add --package sdk-core --bump minor --reason "expose new helper method"
# writes .changeset/<timestamp>-sdk-core.md with the correct frontmatter
```

The generated file wires directly into `mc plan release` and `mc release` without any manual editing. Round-trip tests were added to confirm that files produced by `changes add` pass validation and appear in the release plan with the expected bump severity.

The repository also gained a GitHub Actions workflow that builds and publishes the mdBook automatically on every push to `main` and whenever a release tag is created, so the user-facing guides stay in sync with each shipped version.
