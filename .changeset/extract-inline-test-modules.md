---
monochange: test
monochange_analysis: test
monochange_cargo: test
monochange_config: test
monochange_core: test
monochange_dart: test
monochange_deno: test
monochange_ecmascript: test
monochange_lint: test
monochange_lint_testing: test
monochange_linting: test
monochange_npm: test
monochange_publish: test
monochange_schema: test
monochange_telemetry: test
monochange_test_helpers: test
---

# Extract inline test modules into separate files

Move all inline `#[cfg(test)] mod tests { ... }` blocks out of source files into dedicated test files. This reduces source file sizes and keeps test code in a consistent `__tests/` directory structure next to the module it tests.
