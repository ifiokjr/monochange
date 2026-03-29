---
monochange: patch
---

#### add CLI change-file scaffolding and deploy mdBook docs automatically

Improve the CLI and documentation delivery workflow around the first feature
slice. This change strengthens the `changes add` flow so teams can generate
repo-native changesets directly from `monochange`, and it also rounds out the
supporting tests that verify those generated files integrate correctly with
release planning.

In addition, the repository now builds and deploys the mdBook automatically on
pushes to `main` and on published releases. That makes the user-facing guides
available as part of the normal release workflow instead of depending on manual
book publishing steps.
