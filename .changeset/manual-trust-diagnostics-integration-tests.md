---
monochange: test
---

#### add fixture-first integration coverage for manual trust diagnostics

Adds fixture-based CLI coverage for manual-registry trusted-publishing diagnostics.

The new integration tests cover:

- resolved GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- missing workflow configuration guidance when monochange cannot resolve the GitHub workflow yet
- placeholder-publish dry-run output in both text and JSON formats
