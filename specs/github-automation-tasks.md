# Tasks: GitHub Releases, Changelog Formats, Release PRs, Deployments, and Changeset Bot Rules

## Phase 0 — Contract and scope

- [ ] T001 Finalize terminology for release PRs, changeset PR checks, deployments, and GitHub release publication.
- [ ] T002 Lock the initial config surface for changelog, GitHub, deployment, and bot sections.
- [ ] T003 Decide whether the first bot implementation is GitHub Actions-only or includes a GitHub App.
- [x] T004 Define the release manifest JSON contract.

## Phase 1 — Changelog formats and release notes

- [x] T010 Add changelog format domain types in `crates/monochange_core/src/lib.rs`.
- [x] T011 Add config parsing for changelog format settings in `crates/monochange_config/src/lib.rs`.
- [x] T012 Refactor changelog writing to render through a structured release-note model in `crates/monochange/src/lib.rs`.
- [x] T013 Add tests for defaults, package overrides, and group overrides for changelog formats.
- [x] T014 Add snapshot coverage for rendered changelog output where useful.

## Phase 2 — Release manifest

- [x] T020 Add release manifest domain types in `crates/monochange_core/src/lib.rs`.
- [x] T021 Add release manifest rendering in `crates/monochange/src/lib.rs`.
- [x] T022 Add workflow support for a `RenderReleaseManifest` step.
- [x] T023 Add JSON snapshot tests for release manifests.
- [x] T024 Document the release manifest contract in `specs/` and user docs.

## Phase 3 — GitHub releases

- [ ] T030 Create `crates/monochange_github` for GitHub API integration.
- [ ] T031 Implement GitHub repository and release config parsing.
- [ ] T032 Implement release payload rendering from prepared release data.
- [ ] T033 Add workflow support for `PublishGitHubRelease`.
- [ ] T034 Add dry-run output for GitHub release publication.
- [ ] T035 Add integration tests for grouped and ungrouped GitHub release payloads.

## Phase 4 — Release PR automation

- [ ] T040 Implement release PR config parsing.
- [ ] T041 Implement branch naming, commit generation, and PR title/body rendering.
- [ ] T042 Add workflow support for `OpenReleasePullRequest`.
- [ ] T043 Add idempotent update behavior for an existing release PR.
- [ ] T044 Add integration and snapshot tests for release PR bodies and branch behavior.

## Phase 5 — Deployment orchestration

- [ ] T050 Add deployment intent types and config parsing.
- [ ] T051 Add workflow support for `Deploy` or manifest-driven deployment orchestration.
- [ ] T052 Add merge-trigger examples in `.github/workflows/` documentation.
- [ ] T053 Add tests for deployment trigger evaluation and environment metadata.
- [ ] T054 Document safe deployment gating and environment usage.

## Phase 6 — Changeset bot policy

- [ ] T060 Add bot policy config parsing.
- [ ] T061 Implement a reusable changeset policy evaluator.
- [ ] T062 Add CLI or workflow support for PR policy enforcement.
- [ ] T063 Add GitHub Action examples for required changeset checks.
- [ ] T064 Add rule coverage for skip labels, changed paths, ignored paths, and invalid changesets.
- [ ] T065 Add snapshot tests for bot diagnostics and comments.

## Phase 7 — Docs, examples, and rollout

- [ ] T070 Update `readme.md` and `docs/src/guide/06-release-planning.md` for the new automation model.
- [ ] T071 Add a GitHub automation guide to the mdBook.
- [ ] T072 Add example `monochange.toml` snippets for changelog formats, GitHub releases, release PRs, deployments, and bot rules.
- [ ] T073 Re-run docs sync, linting, testing, coverage, and book builds.
- [ ] T074 Dogfood the new automation on the MonoChange repository itself.

## Suggested execution order

1. Phase 1 — Changelog formats and release notes
2. Phase 2 — Release manifest
3. Phase 3 — GitHub releases
4. Phase 4 — Release PR automation
5. Phase 5 — Deployment orchestration
6. Phase 6 — Changeset bot policy
7. Phase 7 — Docs and rollout
