# Publish readiness, planning, bootstrap, and resume

## Status

- Previous slices shipped `mc publish-readiness` (PR #292), readiness artifact enforcement in `mc publish` (PR #301), Cargo-first publish-readiness blockers (PR #303), `mc publish-plan --readiness <path>` (PR #305), and `mc publish-bootstrap --from <ref> --output <path>` (PR #318).
- Current branch: `feat/publish-resume-flow`.
- Current slice: add package publish result artifacts and retry/resume semantics for real `mc publish` runs.

## Problem

A package registry publish can fail after earlier packages were already published. Before this slice, `mc publish` surfaced the command failure but did not preserve a structured result artifact that CI could use for a safe follow-up run. Operators needed to infer which packages completed and which ones still needed a retry.

## Scope

This slice extends `PublishPackages`/`mc publish` so it can:

- accept `--output <path>` and write a JSON package publish result artifact,
- preserve failed publish attempts as `PackagePublishStatus::Failed` rows in the report,
- write the result artifact before returning a non-zero error for failed publish outcomes,
- accept `--resume <path>` from an earlier real `mc publish` run,
- skip completed package versions from the prior result (`published`, `skipped_existing`, and `skipped_external`),
- retry failed or pending work using the same readiness validation boundary,
- reject resume artifacts from dry-run or placeholder publish flows.

## Non-goals for this slice

- Replacing `mc publish-readiness`; real `mc publish` still validates readiness before registry mutation.
- Treating bootstrap artifacts as publish resume artifacts.
- Adding registry-specific transactionality. This remains a best-effort orchestration layer over external registries.
- Redesigning `monochange/actions`; actions can adapt after the CLI contract stabilizes.

## Affected files

- `crates/monochange/src/package_publish.rs`
- `crates/monochange/src/cli_runtime.rs`
- `crates/monochange/src/publish_bootstrap.rs`
- `crates/monochange/src/publish_readiness.rs`
- `crates/monochange/src/monochange.init.toml`
- `crates/monochange_core/src/lib.rs`
- `monochange.toml`
- `.templates/cli-steps.t.md`
- `.templates/project.t.md`
- `docs/src/guide/13-ci-and-publishing.md`
- `docs/src/reference/cli-steps/16-publish-packages.md`
- `packages/monochange__skill/*`
- `.changeset/publish-resume-flow.md`

## Checklist

- [x] Create isolated worktree and branch `feat/publish-resume-flow`.
- [x] Add deserialization support for package publish reports.
- [x] Add `PackagePublishStatus::Failed` and failure-preserving publish execution.
- [x] Add `--output` artifact writing for `PublishPackages`.
- [x] Add `--resume` filtering from previous real publish reports.
- [x] Return non-zero after writing output artifacts for failed publish outcomes.
- [x] Update command metadata and input validation.
- [x] Add focused unit coverage for resume filtering, artifact I/O, status labels, and failure surfacing.
- [x] Update user docs and packaged skill docs.
- [x] Run formatting and lint checks.
- [x] Run full validation.
- [ ] Run coverage and confirm 100% patch coverage after commit.
- [ ] Push branch and open PR.
- [ ] Merge after required checks pass.

## Validation log

- [x] `devenv shell cargo test -p monochange package_publish --lib`
- [x] `devenv shell cargo test -p monochange_core display_and_publish_steps --lib`
- [x] `devenv shell cargo test -p monochange optional_publish_resume_and_output_paths_trim_and_reject_blank_values --lib`
- [x] `devenv shell cargo fmt --check`
- [x] `git diff --check`
- [x] `devenv shell dprint check`
- [x] `devenv shell mdt check`
- [x] `devenv shell mc validate`
- [x] `devenv shell lint:test`
- [x] `devenv shell coverage:all`
- [ ] `devenv shell coverage:patch` after commit
- [x] `CI=false devenv shell build:all`

## Decisions

- Resume artifacts must come from real `mc publish` reports (`mode = release`, `dry_run = false`). Placeholder and dry-run reports are rejected.
- Completed statuses for resume are `published`, `skipped_existing`, and `skipped_external`; failed or blocked work remains retryable/pending.
- Result artifacts use the existing `PackagePublishReport` JSON shape so rendered CLI output, readiness plumbing, and resume parsing share one contract.
- `--output` is optional for compatibility, but CI documentation should recommend it for all real publish jobs.
- A failed package publish report is still non-zero after the artifact is written.

## Follow-up roadmap

- [ ] Add deeper freshness checks for workspace config, manifests, lockfiles, and publish tooling inputs.
- [ ] Expand npm readiness semantics second.
- [x] Add `mc publish-bootstrap` for first-time package setup.
- [x] Add retry/resume around explicit readiness for remaining work.
