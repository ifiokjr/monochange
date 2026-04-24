# PR #276 failing checks

## Problem statement

PR #276 (`feat(lint): add beautiful interactive progress reporting for mc check and mc lint`) is blocked by failing `lint` and `coverage` checks.

## Scope

- fix compiler and clippy failures on the PR branch
- restore 100% patch coverage for executable changed lines
- rerun relevant validation locally
- push fixes to the PR branch and merge once GitHub checks are green

## Non-goals

- unrelated refactors outside the PR scope
- changing repository-wide coverage policy

## Affected files

- `crates/monochange/src/lint.rs`
- `crates/monochange/src/lint_check_reporter.rs`
- any additional test or fixture files required by coverage validation
- `.github/workflows/ci.yml` only for investigation if needed

## Plan

- [x] inspect failing CI logs and identify the smallest fixes
- [x] apply code and test updates needed for lint and coverage
- [x] run `devenv shell fix:all`
- [x] run targeted validation for touched code
- [x] run `devenv shell lint:all`
- [x] run `devenv shell coverage:patch`
- [ ] push fixes to `feat/beautify-lint-check-output`
- [ ] monitor GitHub checks and squash merge when all required checks pass

## Validation

- `devenv shell fix:all`
- `devenv shell lint:all`
- `devenv shell coverage:patch`

## Notes

- CI reported `unused_qualifications` in `crates/monochange/src/lint.rs`
- CI reported `clippy::indexing_slicing` in `crates/monochange/src/lint_check_reporter.rs`
- the `coverage` job uploads to Codecov and then fails when the patch gate is below target
- local validation now passes with `devenv shell lint:all` and `devenv shell coverage:patch`
