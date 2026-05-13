---
monochange_cargo: patch
monochange_lint: patch
monochange_npm: patch
monochange_test_helpers: patch
---

# Move lint testing helpers into shared test helpers

The private `monochange_lint_testing` crate has been removed. Its stable lint report and autofix formatting helpers now live in `monochange_test_helpers::lint_testing`, so publishable crates no longer depend on an unpublished workspace crate during Cargo package verification.
