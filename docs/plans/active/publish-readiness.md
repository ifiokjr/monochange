# Publish readiness Cargo guards

## Status

- Previous slices: `mc publish-readiness` shipped in PR #292; readiness artifact enforcement shipped in PR #301.
- Current branch: `feat/publish-readiness-cargo-guards`.
- Current slice: add Cargo-first publish-readiness blockers for current manifest publishability before built-in crates.io mutation.

## Problem

Readiness artifacts protect `mc publish` from stale package sets, but the readiness check still mostly reflected registry dry-run status. Cargo packages can be known to fail crates.io publication before mutation when the current manifest opts out of publication or omits required crates.io metadata. monochange should catch those blockers during readiness and enforce the same result before real publish.

## Scope

This slice covers built-in Cargo publishes to crates.io:

- Block current manifests with `publish = false`.
- Block current manifests with `publish = [...]` when the registry list does not include `crates-io`.
- Block missing `description`.
- Block missing both `license` and `license-file`.
- Accept workspace-inherited `description`, `license`, and `license-file` from `[workspace.package]`.
- Keep already-published versions non-blocking when current readiness and the saved artifact agree.
- Surface blocked dry-runs as readiness `blocked`, and reject real publishing before any publish command runs.

## Non-goals for this slice

- Cargo dependency packaging checks beyond existing `cargo publish` behavior.
- Automated crates.io trusted-publisher enrollment.
- npm, JSR, or pub.dev metadata-specific readiness expansion.
- Full manifest/lockfile artifact hashing.
- `mc publish-plan --readiness` integration.

## Affected files

- `crates/monochange/src/package_publish.rs`
- `crates/monochange/src/publish_readiness.rs`
- `crates/monochange/src/cli_runtime.rs`
- `docs/src/guide/13-ci-and-publishing.md`
- `docs/src/reference/cli-steps/16-publish-packages.md`
- `docs/src/readme.md`
- `readme.md`
- `packages/monochange__skill/SKILL.md`
- `packages/monochange__skill/skills/reference.md`
- `packages/monochange__skill/skills/trusted-publishing.md`
- `.changeset/publish-readiness-cargo-guards.md`

## Checklist

- [x] Create isolated worktree and branch `feat/publish-readiness-cargo-guards`.
- [x] Add Cargo manifest readiness blockers.
- [x] Map blocked publish dry-runs to blocked readiness status.
- [x] Reject real publish before registry mutation when blockers are present.
- [x] Add focused unit coverage for missing metadata, `publish = false`, publish registry arrays, workspace inheritance, dry-run blocking, and real-publish rejection.
- [x] Update user docs and packaged skill docs for Cargo readiness behavior.
- [ ] Run formatting and lint checks.
- [ ] Run full validation.
- [ ] Run coverage and confirm 100% patch coverage.
- [ ] Push branch and open PR.
- [ ] Merge after required checks pass.

## Validation log

- [x] `devenv shell cargo fmt`
- [x] `devenv shell cargo test -p monochange cargo_publish_readiness_blockers --lib`
- [x] `devenv shell cargo test -p monochange execute_publish_requests_marks_dry_run_cargo_metadata_blockers --lib`
- [x] `devenv shell cargo test -p monochange execute_publish_requests_rejects_real_cargo_metadata_blockers --lib`
- [x] `devenv shell cargo test -p monochange publish_readiness --lib`
- [x] `devenv shell cargo test -p monochange package_publish_status_label --lib`

## Decisions

- Validate the current manifest immediately after registry existence checks. Already-published versions are still skipped before manifest checks, which preserves idempotent/resumable publishes.
- Limit this first ecosystem-specific guard to built-in Cargo publishes targeting crates.io.
- Treat `repository`, `homepage`, and `documentation` as useful recommendations but not hard blockers for this slice.
- Keep first-time bootstrap separate from readiness; `publish = false` or missing crates.io metadata should be fixed before built-in publication.

## Follow-up roadmap

- [ ] Add deeper freshness checks for workspace config, manifests, lockfiles, and publish tooling inputs.
- [ ] Add optional readiness consumption to `mc publish-plan`.
- [ ] Expand npm readiness semantics second.
- [ ] Add `mc publish-bootstrap` for first-time package setup.
- [ ] Design retry/resume around explicit readiness for remaining work.
