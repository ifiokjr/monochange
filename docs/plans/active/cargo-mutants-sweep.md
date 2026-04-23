# Cargo mutants workspace sweep

## Problem statement

Run cargo-mutants across the monochange Rust workspace from the latest `main`, fix surviving mutants with the smallest production changes possible, and add regression tests plus new property tests where they improve mutation resistance.

## Scope

- run one or more cargo-mutants passes across workspace crates
- triage surviving mutants into fixable vs equivalent/timeout/tooling buckets
- add failing regression tests first for real gaps
- add property-based tests when they provide better invariant coverage than example tests
- keep validation, patch coverage, and docs/fixtures in sync with touched crates
- open a PR, monitor CI, and merge with squash once green

## Non-goals

- broad refactors unrelated to mutation survivors
- changing mutation-testing product policy beyond what is required for this sweep
- forcing non-equivalent findings into comments without first confirming with tests/fixtures

## Affected areas

- workspace Rust crates identified by the sweep
- `fixtures/tests/**` for any filesystem-backed scenarios
- `.changeset/*.md` if published crate code changes are required
- CI/PR metadata only if needed for validation or merge

## Checklist

- [x] confirm worktree is based on latest `origin/main`
- [ ] establish initial cargo-mutants baseline across workspace crates
- [x] record surviving mutants by crate and prioritize real test gaps
- [x] add failing tests / fixtures / snapshots for each real survivor found so far in `monochange_config`
- [x] add property tests where invariants are a better fit than examples
- [ ] run `fix:all`
- [ ] run targeted tests, then full validation (`lint:all`, `test:all`, `mc validate`)
- [ ] ensure patch coverage for changed executable lines is 100%
- [ ] add/update changeset if published crate code changed
- [ ] open PR, monitor checks, and squash merge when green

## Validation commands

- `devenv shell cargo mutants -p <crate> --no-shuffle --output mutants-report/<crate>`
- `devenv shell fix:all`
- `devenv shell lint:all`
- `devenv shell test:all`
- `devenv shell mc validate`
- patch coverage command to be determined from current workspace tooling

## Notes

- Reuse prior findings from `docs/plans/active/mutation-testing-kani-analysis.md` where applicable, but re-run against current `main` before making changes.
- If a survivor appears equivalent, document the reasoning in the plan and only leave source comments when the equivalence is confidently demonstrated.
- Completed focused pass on `monochange_config`.
- Added fixture-backed regression coverage for:
  - ecosystem versioned-file inheritance across Cargo, Deno, and Dart
  - explicit group version bump inference when the highest member version is not last in group order
  - explicit group version bump inference when group member order is descending
  - excluded changelog types rejecting invalid configured change types
  - defaults changelog behavior for disabled/package-default cases
- Added proptests for `infer_bump_from_versions` major/minor/patch invariants.
- Current suspected equivalent survivors in `monochange_config`:
  - `default_parent_bump -> Default::default()` because `BumpSeverity::default()` is `Patch`
  - delete `PackageType::Cargo` arm in `package_type_to_ecosystem_type` because the required wildcard arm also maps to `EcosystemType::Cargo`
  - replace `>` with `>=` when choosing the max group member version because equal versions produce the same chosen maximum value
- Completed focused pass on `monochange_graph`.
- Current suspected equivalent/benign outcomes in `monochange_graph`:
  - deleting `trigger_type` in the `DecisionState` initializer because `DecisionState::default()` already sets `trigger_type` to `"none"`
  - replacing `>` with `>=` in trigger-priority comparison because all trigger priorities are unique (`3`, `2`, `1`, `0`)
  - one additional `>` -> `>=` mutant timed out in `apply_decision`; no evidence yet that it reflects a real correctness gap
- Completed focused pass on `monochange_semver` with an empty `missed.txt`; no new work required there.
- Completed focused pass on `monochange_core`.
- Added fixture-backed/test-only coverage for:
  - discovery filtering when a parent `.git` path exists outside the workspace root
  - block-comment stripping with embedded `*` characters before the closing marker
- Current suspected equivalent survivors in `monochange_core`:
  - deleting `MonochangeError::render` arms for `Diagnostic`, `HttpRequest`, and `Cancelled` because each specialized branch currently renders the same string as the fallback `self.to_string()` path
- Completed focused pass on `monochange_hosting`.
- Added mutation-killing coverage for:
  - exact changelog owner matching in `release_pull_request_body`
  - exact changelog owner matching in `release_body`
  - successful GET/POST/PUT/PATCH JSON helpers
  - `get_optional_json` handling of both 404 and successful responses
  - packaging/build compatibility for `src/__tests.rs` via `include = ["src/*.rs", ...]` and `#[cfg(test)] mod __tests;`
- `monochange_hosting` rerun result: 34 caught, 43 unviable, 0 missed.
