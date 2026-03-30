# Testing requirements

- Every non-trivial behavior change should start with a failing test.
- Release-planning logic needs realistic fixture coverage.
- Cross-ecosystem behavior should remain consistent across Cargo, npm-family, Deno, Dart, and Flutter.
- Keep `mc validate` green alongside the rest of the validation suite.
