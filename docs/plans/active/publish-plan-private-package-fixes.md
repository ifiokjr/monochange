# Publish planning and placeholder publishing fixes

## Goal

Fix publish planning so disabled or private packages never appear in publish batches, make placeholder publishing treat any existing registry version as already bootstrapped, correct npm trusted-publishing setup URLs, and harden crates.io lookups so transient API behavior does not break `mc publish-plan`.

## Scope

- `crates/monochange/src/package_publish.rs`
- `crates/monochange/src/publish_rate_limits.rs`
- `crates/monochange/tests/cli_output.rs`
- `fixtures/tests/publish-rate-limits/*`
- `.changeset/*`

## Non-goals

- redesign the publish workflow beyond the already landed batching flow
- change registry rate-limit policy values

## Checklist

- [ ] add failing tests for disabled/private package filtering in release publish planning
- [ ] add failing tests for placeholder bootstrap skipping when any registry version already exists
- [ ] add failing tests for npm manual setup URLs in placeholder dry-run output
- [ ] add failing tests for crates.io lookup fallback behavior
- [ ] implement the smallest publish request filtering and registry lookup changes
- [ ] run fixers, targeted tests, full validation, and patch coverage
- [ ] add a user-facing changeset and open a PR

## Validation

- `devenv shell cargo test -p monochange publish_rate_limits::tests::...`
- `devenv shell cargo test -p monochange package_publish::tests::...`
- `devenv shell cargo test -p monochange --test cli_output`
- `devenv shell fix:all`
- `devenv shell test:all`
- `devenv shell coverage:patch`
- `devenv shell mc validate`

## Notes

- Favor fixture-first coverage for workspace/discovery scenarios.
- Keep the publish filtering logic aligned between `mc publish` and `mc publish-plan`.
