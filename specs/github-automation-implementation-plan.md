# Implementation Plan: GitHub Releases, Changelog Formats, Release PRs, Deployments, and Changeset Bot Rules

**Branch**: `spike/github-release-automation-plan` | **Date**: 2026-03-29 | **Design**: [`specs/github-automation-design.md`](./github-automation-design.md)

## Summary

Add a GitHub-aware automation layer on top of MonoChange's current workflow-driven CLI so repositories can render structured changelogs, publish GitHub releases, open release pull requests, trigger deployments after merge, and enforce changeset policies on pull requests.

This work should build on the current config-driven CLI and release planner rather than replacing it.

## Objectives

1. Add explicit changelog format support.
2. Add a shared release-notes data model.
3. Add machine-readable release manifest output.
4. Add GitHub release publication.
5. Add release PR creation and update.
6. Add deployment orchestration hooks for merged release PRs.
7. Add PR-time changeset policy enforcement suitable for a bot workflow.
8. Keep core planning reusable and mostly GitHub-agnostic.

## Non-Goals

- implementing every deployment provider directly inside MonoChange
- building a fully hosted GitHub App in the first slice
- replacing repository CI/CD systems with MonoChange-owned pipelines
- remote package publishing for every ecosystem in the same milestone
- multi-forge support in the first GitHub-focused slice

## Proposed Crate and Layering Changes

## 1. `monochange_core`

Add pure domain types for:

- changelog formats
- release-note sections
- GitHub automation intent models
- deployment intent models
- release manifest structures
- new workflow step definitions

Avoid HTTP, authentication, or GitHub API calls here.

## 2. `monochange_config`

Extend config parsing and validation for:

- default changelog format settings
- GitHub release settings
- release PR settings
- deployment settings
- bot policy settings

Validation should catch:

- incompatible changelog format combinations
- missing required GitHub repo metadata
- invalid workflow step configuration
- impossible deployment trigger combinations

## 3. `monochange`

Extend CLI and execution logic for:

- rendering release manifests
- running typed GitHub automation steps
- dry-run output for all new automation behaviors
- invoking GitHub runtime operations through a dedicated integration layer

## 4. new crate: `monochange_github`

Recommended responsibilities:

- GitHub REST API client wrapper
- release creation/update logic
- pull request creation/update logic
- bot comment/status helpers
- conversion between MonoChange release artifacts and GitHub payloads

## Proposed Workflow Step Additions

Add these typed steps to `WorkflowStepDefinition`:

- `RenderReleaseManifest`
- `PublishGitHubRelease`
- `OpenReleasePullRequest`
- `Deploy`
- `EnforceChangesetPolicy`

The generic `Command` step should remain for repository-specific scripts.

## Proposed Config Surfaces

## A. Changelog config

```toml
[defaults.changelog]
format = "keep_a_changelog"
include_links = true

[package.monochange.changelog]
format = "cargo_like"

[group.main.changelog]
format = "markdown_sections"
sections = ["summary", "packages", "evidence"]
```

## B. GitHub config

```toml
[github]
owner = "ifiokjr"
repo = "monochange"

[github.releases]
enabled = true
draft = false
prerelease = false
source = "monochange"
```

## C. Release PR config

```toml
[github.pull_requests]
enabled = true
branch_prefix = "monochange/release"
title = "chore(release): prepare release"
labels = ["release", "automated"]
auto_merge = false
```

## D. Deployment config

```toml
[[deployments]]
name = "production"
trigger = "release_pr_merge"
workflow = "deploy-production"
```

## E. Bot policy config

```toml
[github.bot.changesets]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["crates/**", "packages/**"]
ignored_paths = ["docs/**", "*.md"]
```

## Release Manifest Contract

Introduce a stable JSON artifact, for example:

```json
{
	"workflow": "release",
	"dryRun": false,
	"version": "1.2.0",
	"releaseTargets": [],
	"releasedPackages": [],
	"changedFiles": [],
	"changelogs": [],
	"deployments": []
}
```

This artifact should be usable by:

- local preview tooling
- GitHub release publishing workflows
- deployment workflows after merge
- future bot/app logic

## Phased Plan

## Phase 0 — Contract lock and examples

1. Finalize terminology:
   - release PR vs changeset PR
   - deployment trigger naming
   - changelog format naming
2. Add design examples for grouped and ungrouped repos.
3. Decide whether the first bot release is Actions-only.

## Phase 1 — Changelog and release-note foundations

1. Add changelog format enums and configuration models.
2. Introduce structured release-note sections.
3. Refactor current changelog writing to render through the new model.
4. Add tests for package/group/default format selection.

## Phase 2 — Release manifest and automation intent

1. Introduce a stable release manifest type.
2. Add CLI and workflow support for manifest rendering.
3. Add dry-run JSON coverage for release manifest output.
4. Ensure changed files, release targets, and note sections are captured.

## Phase 3 — GitHub releases

1. Add `monochange_github` crate.
2. Implement authenticated GitHub release creation/update.
3. Add `PublishGitHubRelease` workflow step.
4. Support dry-run preview of would-be tags, releases, and bodies.
5. Add integration tests around release payload generation.

## Phase 4 — Release PR automation

1. Implement branch naming and commit generation.
2. Implement PR body rendering from release notes.
3. Add `OpenReleasePullRequest` workflow step.
4. Support idempotent update of an existing release PR.
5. Add tests for grouped releases and ungrouped releases.

## Phase 5 — Merge-driven deployment orchestration

1. Define deployment intent structures.
2. Add `Deploy` workflow step or manifest-driven deploy trigger support.
3. Add GitHub Actions examples for release PR merge triggers.
4. Support environment-aware deployment metadata.
5. Add docs for safe deployment gating.

## Phase 6 — Changeset bot policy

1. Implement a reusable PR policy engine in MonoChange.
2. Add `EnforceChangesetPolicy` behavior for CI usage.
3. Publish an example GitHub Action workflow for PR checks.
4. Add skip-label and changed-path policy support.
5. If needed, add a later path to a GitHub App wrapper.

## Recommended Milestone Split

### Milestone 1

- changelog formats
- release-note rendering
- release manifest

### Milestone 2

- GitHub releases

### Milestone 3

- release PR automation

### Milestone 4

- deployment orchestration
- bot policy checks

## Testing Strategy

### Unit tests

- config parsing and validation
- release-note rendering
- changelog format selection
- deployment trigger evaluation
- bot rule evaluation

### Integration tests

- grouped release manifests
- GitHub release payload rendering
- release PR content rendering
- deployment metadata generation

### Snapshot tests

- CLI output for GitHub-related dry-runs
- release manifest JSON
- release PR body rendering
- bot failure diagnostics

### Docs validation

- update `readme.md`
- update release-planning docs
- add GitHub automation guide pages
- add end-to-end examples for release PR and deploy flows

## Risks and Mitigations

### Risk: Changelog and GitHub notes diverge

Mitigation:

- require both to render from one shared release-note model

### Risk: PR automation becomes non-idempotent

Mitigation:

- model release PR identity explicitly and update in place

### Risk: Deployment support becomes provider-specific too early

Mitigation:

- model deployment intent and let repository workflows execute platform details

### Risk: GitHub bot permissions become unsafe

Mitigation:

- start with least-privilege status-check workflows
- avoid privileged fork execution in the first slice

### Risk: GitHub-only features leak into core planner

Mitigation:

- isolate transport and API code in `monochange_github`

## Acceptance Criteria

This planning slice is complete when the team agrees on:

1. the feature breakdown and delivery order
2. the configuration model direction
3. the new workflow step direction
4. the release manifest contract
5. the initial GitHub bot execution model
6. the testing strategy and milestone boundaries
