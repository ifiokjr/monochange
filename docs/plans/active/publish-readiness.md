# Publish readiness artifact freshness

## Status

- Previous publish-readiness slices shipped `mc publish-readiness`, readiness enforcement in `mc publish`, Cargo readiness blockers, readiness-backed publish planning, `mc publish-bootstrap`, and package publish resume artifacts.
- Current branch: `feat/publish-readiness-freshness`.
- Current slice: add deeper freshness checks for readiness artifacts by fingerprinting publish inputs that affect registry behavior.

## Problem

A readiness artifact already proves that a release record and package set were ready at the time the artifact was written. It did not explicitly prove that the workspace publish inputs stayed unchanged afterward. A workflow could generate readiness, then alter `monochange.toml`, a package manifest, a lockfile, `.npmrc`, Cargo config, or another publish tooling input before `mc publish --readiness <path>` consumed the artifact.

## Scope

This slice extends readiness artifacts so they include a deterministic `inputFingerprint` derived from:

- `monochange.toml`,
- package manifests for configured package types,
- root and package lockfiles,
- root publish tooling files such as `.npmrc`, `.cargo/config.toml`, `.cargo/config`, `rust-toolchain.toml`, `rust-toolchain`, workspace `Cargo.toml`, `pnpm-workspace.yaml`, Python tooling config, Deno config, and pubspec files.

`mc publish` and readiness-backed `mc publish-plan` rebuild current readiness and reject saved artifacts whose publish input fingerprint no longer matches.

## Non-goals for this slice

- Replacing release-record and package-set validation.
- Adding cryptographic guarantees; this is a deterministic freshness guard, not a signature scheme.
- Modeling every possible ecosystem-specific side file. The tracked set can grow as additional publish integrations mature.

## Affected files

- `crates/monochange/src/publish_readiness.rs`
- `crates/monochange/src/cli_runtime.rs`
- `docs/src/reference/cli-steps/09-plan-publish-rate-limits.md`
- `docs/src/reference/cli-steps/16-publish-packages.md`
- `docs/src/guide/13-ci-and-publishing.md`
- `docs/src/guide/15-publish-rate-limits.md`
- `.templates/project.t.md`
- `docs/src/readme.md`
- `readme.md`
- `packages/monochange__skill/*`
- `.changeset/publish-readiness-input-fingerprint.md`

## Checklist

- [x] Create isolated worktree and branch `feat/publish-readiness-freshness`.
- [x] Add `inputFingerprint` to readiness reports.
- [x] Fingerprint workspace config, manifests, lockfiles, and publish tooling inputs.
- [x] Reject stale readiness artifacts during real publish validation.
- [x] Reject stale readiness artifacts during readiness-backed publish planning.
- [x] Add focused unit coverage for tracked input paths, fingerprint changes, stale validation errors, and artifact rendering.
- [x] Update docs, skill guidance, and changeset.
- [x] Run formatting and validation.
- [x] Run coverage and confirm 100% patch coverage after commit.
- [ ] Push branch and open PR.
- [ ] Merge after required checks pass.

## Validation log

- [x] `devenv shell cargo test -p monochange publish_readiness --lib`
- [x] `devenv shell cargo fmt --check`
- [x] `git diff --check`
- [x] `devenv shell dprint check`
- [x] `devenv shell mdt check`
- [x] `devenv shell mc validate`
- [x] `devenv shell lint:test`
- [x] `devenv shell coverage:all`
- [x] `devenv shell coverage:patch` after commit — `PATCH_COVERAGE 267/267 (100.00%)`
- [x] `CI=false devenv shell build:all`

## Decisions

- Readiness artifacts are invalidated by publish input changes even when the release record commit and package set are unchanged.
- The fingerprint is based on sorted relative paths plus file contents so path ordering does not depend on filesystem traversal.
- The readiness schema version is bumped to `2` because the artifact contract now includes `inputFingerprint`.
- Missing `inputFingerprint` still deserializes for better user-facing validation errors, but validation rejects it because it cannot match a freshly generated report.
