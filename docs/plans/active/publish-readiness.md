# Publish readiness enforcement

## Status

- Previous slice: `mc publish-readiness` shipped in PR #292.
- Current branch: `feat/publish-readiness-enforcement`.
- Current slice: require and validate a readiness artifact before real `mc publish` registry mutation.

## Problem

Package publishing now has a standalone, non-mutating readiness command, but `mc publish` can still start registry mutation without proving that readiness was checked against the same release record and package selection. monochange needs an explicit artifact boundary so CI can fail before mutation when readiness is missing, blocked, stale, malformed, or generated for a different package set.

## Scope

This slice covers the first enforcement step:

- Extend the publish-readiness JSON artifact with a schema version, kind, release-record commit metadata, and package-set fingerprint.
- Add `readiness` as a publish workflow input.
- Require `--readiness <PATH>` for real `PublishPackages` runs.
- Keep `--dry-run` publish previews usable without a readiness artifact.
- Validate artifact kind, schema, status, release-record commit, duplicate package entries, package fingerprint, and selected package set before registry mutation.
- Treat already-published packages as non-blocking when the artifact and current readiness agree.
- Update reference docs, CI guides, templates, and generated docs for the new publish flow.

## Non-goals for this slice

- Full config/manifests/lockfile hashing.
- `mc publish-plan --readiness` integration.
- `mc publish-bootstrap` or first-time placeholder orchestration changes.
- `monochange/actions` publishing API redesign.
- Cargo-specific metadata/remediation blockers beyond the existing dry-run publish checks.

## Affected files

- `crates/monochange/src/cli_runtime.rs`
- `crates/monochange/src/monochange.init.toml`
- `crates/monochange/src/package_publish.rs`
- `crates/monochange/src/publish_readiness.rs`
- `monochange.toml`
- `.templates/cli-steps.t.md`
- `.templates/project.t.md`
- `readme.md`
- `docs/src/readme.md`
- `docs/src/guide/02-setup.md`
- `docs/src/guide/07-trusted-publishing.md`
- `docs/src/guide/08-github-automation.md`
- `docs/src/guide/13-ci-and-publishing.md`
- `docs/src/guide/14-multi-package-publishing.md`
- `docs/src/guide/15-publish-rate-limits.md`
- `docs/src/reference/cli-steps/00-index.md`
- `docs/src/reference/cli-steps/10-publish-release.md`
- `docs/src/reference/cli-steps/16-publish-packages.md`
- `.changeset/publish-readiness-enforcement.md`

## Checklist

- [x] Create isolated worktree and branch `feat/publish-readiness-enforcement`.
- [x] Extend readiness artifacts with stable schema/kind and release-record metadata.
- [x] Add package-set fingerprint validation.
- [x] Add artifact loading and validation helpers.
- [x] Require `--readiness <PATH>` for real `PublishPackages` runs.
- [x] Keep dry-run publish previews artifact-free.
- [x] Add unit coverage for successful validation and all validation failure classes.
- [x] Add unit coverage for missing and blank readiness paths.
- [x] Update publish command config in root and init templates.
- [x] Update docs and templates for the readiness-enforced publish flow.
- [x] Run generated docs update.
- [x] Run formatting and lint checks.
- [x] Run targeted tests.
- [x] Run full validation.
- [x] Run coverage and confirm 100% patch coverage.
- [ ] Push branch and open PR.
- [ ] Merge after required checks pass.

## Validation log

- [x] `devenv shell cargo test -p monochange readiness --lib` (initial implementation; passed with warnings that were fixed afterward)
- [x] `devenv shell cargo test -p monochange execute_cli_command_requires_readiness_for_package_publish_steps_without_matching_packages --lib`
- [x] `devenv shell cargo test -p monochange_core display_and_publish --lib`
- [x] `devenv shell cargo fmt --check`
- [x] `git diff --check`
- [x] `devenv shell mdt update`
- [x] `devenv shell lint:test`
- [x] `devenv shell mc validate`
- [x] `devenv shell coverage:all`
- [x] `devenv shell coverage:patch` (`PATCH_COVERAGE 395/395 (100.00%)`)

## Decisions

- Real package publication requires a readiness artifact; dry-runs remain frictionless.
- Artifact validation happens immediately before the existing rate-limit plan and package publish execution, so failures happen before registry mutation.
- The first fingerprint is intentionally a deterministic package-set identity rather than a cryptographic digest; it covers package id, ecosystem, registry, and version.
- `already_published` remains non-blocking as long as current readiness and the artifact agree.
- `unsupported` remains blocking because monochange cannot mutate unsupported ecosystem publications safely.

## Follow-up roadmap

- [ ] Add deeper freshness checks for workspace config, manifests, lockfiles, and publish tooling inputs.
- [ ] Add optional readiness consumption to `mc publish-plan`.
- [ ] Expand Cargo readiness semantics first.
- [ ] Expand npm readiness semantics second.
- [ ] Add `mc publish-bootstrap` for first-time package setup.
- [ ] Design retry/resume around explicit readiness for remaining work.
