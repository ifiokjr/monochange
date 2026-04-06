# Testing requirements

- Every non-trivial behavior change should start with a failing test.
- Release-planning logic needs realistic fixture coverage.
- Cross-ecosystem behavior should remain consistent across Cargo, npm-family, Deno, Dart, and Flutter.
- Keep `mc validate` green alongside the rest of the validation suite.

## Fixture-first testing

- **All tests that interact with the filesystem must load their content from `fixtures/tests/` directories**, not from inline strings embedded in Rust test code.
- Fixture files (e.g. `monochange.toml`, `Cargo.toml`, `.changeset/*.md`) live under `fixtures/tests/<test-suite>/<scenario>/` and can be copied into a `tempdir` at test time when the test needs a writable workspace.
- Read-only tests (e.g. config validation that only calls `load_workspace_configuration`) may point directly at the fixture path without copying.
- If a scenario needs a different file payload, add a new fixture variant rather than writing inline strings in the test body.
- This rule applies to unit tests in `__tests.rs` modules as well as integration tests in `tests/*.rs` — if a test writes config or manifest files to disk, those files must originate from the fixtures directory.
- The fixture-first approach makes it easy to visually audit test scenarios, reuse fixtures across tests, and catch regressions via `git diff` on fixture content.
- Runtime git-repository provider tests are still exempt because they intentionally create and mutate live repositories.
