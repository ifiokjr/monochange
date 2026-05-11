# Testing requirements

- Every non-trivial behavior change should start with a failing test.
- Release-planning logic needs realistic fixture coverage.
- Cross-ecosystem behavior should remain consistent across Cargo, npm-family, Deno, Dart, and Flutter.
- Keep `mc validate` green alongside the rest of the validation suite.
- New integration tests must be added to the dedicated `crates/monochange_integration_tests` crate, not to individual package `tests/` directories.
- Rust unit tests inside crate `src/` trees must be placed in a `__tests__/` directory beside the source module and named after the source file: `analysis.rs` uses `#[path = "__tests__/analysis_tests.rs"] mod tests;`, `lib.rs` uses `#[path = "__tests__/lib_tests.rs"] mod tests;`, and `mod.rs` uses `#[path = "__tests__/mod_tests.rs"] mod tests;`. Do not add inline `#[cfg(test)] mod tests { ... }` blocks or sibling `__tests.rs` files. Integration tests in `<crate>/tests/` keep the standard Rust integration-test layout and should not be moved into `__tests__/`.
- `#[cfg(test)]` may only reference a test module and may appear at most once per source file. Keep test-only helpers, imports, state, and re-exports in test files; if helpers must be shared across crates, put them in the test helper crate instead of gating production source with `#[cfg(test)]`. Do not add or use `#[allow(dead_code)]`; move unused or test-only code into test files instead.
- PRs must keep patch coverage at 100% for executable changed lines in the Rust coverage report.

## Fixture-first testing

- **All tests that interact with the filesystem must load their content from `fixtures/tests/` directories**, not from inline strings embedded in Rust test code.
- Fixture files (e.g. `monochange.toml`, `Cargo.toml`, `.changeset/*.md`) live under `fixtures/tests/<test-suite>/<scenario>/` and can be copied into a `tempdir` at test time when the test needs a writable workspace.
- Prefer scenario folders with a layout like:
  - `fixtures/tests/<suite>/<scenario>/workspace/...` for the input workspace when the test suite needs a writable copy
  - additional checked-in files under the scenario only when they are part of the input being exercised
- Read-only tests (e.g. config validation that only calls `load_workspace_configuration`) may point directly at the fixture path without copying.
- If a scenario needs a different file payload or expected output, add a new fixture variant rather than writing inline strings in the test body.
- This rule applies to unit tests in `__tests__/*_tests.rs` modules as well as integration tests in `tests/*.rs` — if a test writes config or manifest files to disk, those files must originate from the fixtures directory.
- The fixture-first approach makes it easy to visually audit test scenarios, reuse fixtures across tests, and catch regressions via `git diff` on fixture content.
- Runtime git-repository provider tests are still exempt because they intentionally create and mutate live repositories.

## Output assertions

- Prefer **external `insta` snapshots** over inline snapshots when comparing output.
- This applies to human-readable output such as CLI help, stdout/stderr text, changelog text, markdown, and rendered release bodies **and** to structured machine-readable output such as JSON manifests or dry-run payloads.
- For Rust tests, prefer built-in snapshot generation via `insta::assert_snapshot!`, `insta::assert_json_snapshot!`, or `insta_cmd::assert_cmd_snapshot!` instead of maintaining parallel hand-authored `expected` files.
- Treat `String::contains(...)` assertions on rendered output as a code smell. When the output matters enough to assert, snapshot the full rendered value instead of checking a few substrings.
- Prefer **insta redactions** over **insta filters** when stabilizing dynamic output. Redactions preserve the structural assertion while replacing environment-, time-, or input-dependent fields with stable placeholders.
- Keep JSON snapshots readable: do not leave multiline string fields embedded as escaped `\n` sequences inside snapshotted JSON objects. Redact the JSON field (for example, `"[multiline text]"`) and add a separate string snapshot for the multiline contents.
- Keep checked-in snapshots relevant. `test:cargo` and CI reject unreferenced `.snap` files; use `snapshot:update` to regenerate snapshots and delete unreferenced snapshot files.
- Use filters only when the snapshot target is effectively unstructured text and selector-based redactions are not practical.
- When using `rstest`, give each parametrized case a stable snapshot suffix so every case gets its own external snapshot file.
- If a test can be expressed as “copy scenario workspace, run command, snapshot the output”, prefer that pattern over large in-test `assert_eq!` trees.
- Keep imperative assertions for scenarios that are genuinely stateful or easier to understand as focused semantic checks (for example multi-step git history setup, partial property assertions, or intentionally dynamic output).

## rstest usage

- Reach for `rstest` when multiple integration scenarios share the same command shape and only differ by fixture path, arguments, or expected output.
- Prefer parameterized `rstest` cases over open-coded loops when each scenario should show up as a distinct named test failure.
