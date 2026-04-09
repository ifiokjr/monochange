---
main: test
---

Refactor the Rust test suites toward a more consistent fixture-first structure with shared scenario helpers, external `insta` snapshots, and clearer per-scenario coverage.

This keeps filesystem-backed tests easier to review, makes snapshot updates more predictable, and improves coverage across the workspace without changing release behavior.
