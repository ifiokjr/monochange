---
monochange: test
monochange_analysis: test
monochange_cargo: test
monochange_config: test
monochange_core: test
monochange_dart: test
monochange_deno: test
monochange_ecmascript: test
monochange_forgejo: test
monochange_gitea: test
monochange_github: test
monochange_gitlab: test
monochange_go: test
monochange_graph: test
monochange_hosting: test
monochange_lint: test
monochange_lint_testing: test
monochange_linting: test
monochange_npm: test
monochange_publish: test
monochange_python: test
monochange_schema: test
monochange_semver: test
monochange_telemetry: test
monochange_test_helpers: test
---

# Normalize Rust unit test file layout

Move Rust unit tests into colocated `__tests__/` directories and name each file after the module under test with a `_tests.rs` suffix.
