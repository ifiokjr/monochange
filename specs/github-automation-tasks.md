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

- [x] T030 Create `crates/monochange_github` for GitHub API integration.
- [x] T031 Implement GitHub repository and release config parsing.
- [x] T032 Implement release payload rendering from prepared release data.
- [x] T033 Add workflow support for `PublishGitHubRelease`.
- [x] T034 Add dry-run output for GitHub release publication.
- [x] T035 Add integration tests for grouped and ungrouped GitHub release payloads.
- [x] T036 Add release-note customization for `change_templates`, `extra_changelog_sections`, and optional change `type` / `details` fields.

> Follow-up note: add git-derived release-note template variables next (`$commit_hash`, `$commit_author_name`, and related metadata).

## Phase 4 — Release PR automation

- [x] T040 Implement release PR config parsing.
- [x] T041 Implement branch naming, commit generation, and PR title/body rendering.
- [x] T042 Add workflow support for `OpenReleasePullRequest`.
- [x] T043 Add idempotent update behavior for an existing release PR.
- [x] T044 Add integration and snapshot tests for release PR bodies and branch behavior.

## Phase 5 — Deployment orchestration

- [x] T050 Add deployment intent types and config parsing.
- [x] T051 Add workflow support for `Deploy` or manifest-driven deployment orchestration.
- [x] T052 Add merge-trigger examples in `.github/workflows/` documentation.
- [x] T053 Add tests for deployment trigger evaluation and environment metadata.
- [x] T054 Document safe deployment gating and environment usage.

## Phase 6 — Changeset bot policy

- [x] T060 Add bot policy config parsing.
- [x] T061 Implement a reusable changeset policy evaluator.
- [x] T062 Add CLI or workflow support for PR policy enforcement.
- [x] T063 Add GitHub Action examples for required changeset checks.
- [x] T064 Add rule coverage for skip labels, changed paths, ignored paths, and invalid changesets.
- [x] T065 Add snapshot tests for bot diagnostics and comments.

## Phase 7 — Docs, examples, and rollout

- [x] T070 Update `readme.md` and `docs/src/guide/06-release-planning.md` for the new automation model.
- [x] T071 Add a GitHub automation guide to the mdBook.
- [x] T072 Add example `monochange.toml` snippets for changelog formats, GitHub releases, release PRs, deployments, and bot rules.
- [x] T073 Re-run docs sync, linting, testing, coverage, and book builds.
- [x] T074 Dogfood the new automation on the MonoChange repository itself.

## Suggested execution order

1. Phase 1 — Changelog formats and release notes
2. Phase 2 — Release manifest
3. Phase 3 — GitHub releases
4. Phase 4 — Release PR automation
5. Phase 5 — Deployment orchestration
6. Phase 6 — Changeset bot policy
7. Phase 7 — Docs and rollout
