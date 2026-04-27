# Publish readiness command

## Status

- Branch: `feat/publish-readiness-command`
- Pull request: <https://github.com/monochange/monochange/pull/292>
- Current slice: initial top-level, non-mutating `mc publish-readiness` command

## Problem

Package publishing currently starts from release state and registry dry-run behavior without a standalone readiness artifact that CI can inspect before mutating registries. monochange needs a CLI-first readiness boundary so future publishing can fail early for blocked, stale, ambiguous, or unsupported package publish state.

## Scope

This plan covers the first incremental slice:

- Add `mc publish-readiness` as a top-level command.
- Reuse existing dry-run package publish checks to report initial readiness.
- Support package filtering and machine-readable artifacts.
- Document the command in CLI docs and command matrices.
- Keep the implementation non-mutating.

## Non-goals for this slice

- Require readiness artifacts from `mc publish`.
- Add fingerprint/freshness validation.
- Add `mc publish-bootstrap`.
- Redesign `monochange/actions` publishing APIs.
- Implement full ecosystem-specific blocker/remediation semantics.

## Affected files

- `crates/monochange/src/cli.rs`
- `crates/monochange/src/cli_help.rs`
- `crates/monochange/src/lib.rs`
- `crates/monochange/src/package_publish.rs`
- `crates/monochange/src/publish_readiness.rs`
- `crates/monochange/tests/snapshots/`
- `.templates/project.t.md`
- `readme.md`
- `docs/src/readme.md`
- `docs/src/guide/13-ci-and-publishing.md`
- `.changeset/publish-readiness-command.md`
- `tsconfig.json`
- `devenv.nix`

## Checklist

- [x] Create isolated worktree and branch `feat/publish-readiness-command`.
- [x] Add `mc publish-readiness` CLI definition with `--from`, `--package`, `--output`, and `--format`.
- [x] Wire command dispatch through `crates/monochange/src/lib.rs`.
- [x] Refactor package publishing dry-run execution for reuse by release ref.
- [x] Add `publish_readiness` report model and renderers for Markdown, text, and JSON.
- [x] Map existing package publish statuses to initial readiness statuses.
- [x] Add unit coverage for status mapping and render formats.
- [x] Add CLI help coverage for `publish-readiness`.
- [x] Update generated docs and command matrices.
- [x] Add changeset for the `monochange` crate.
- [x] Add root `tsconfig.json` so JS type linting has a project configuration.
- [x] Fix local worktree git configuration so pre-push tests run against isolated fixture repositories.
- [x] Update CLI snapshots for the new command.
- [x] Run targeted validation.
- [x] Run full local `devenv shell lint:test` successfully.
- [x] Create PR #292.
- [x] Raise patch coverage to 100% after the initial PR coverage failure.
- [ ] Wait for PR checks to complete.
- [ ] Fix any failed PR checks.
- [ ] Merge PR #292 after required checks pass.
- [ ] Move this plan to `docs/plans/completed/` after merge, or keep remaining readiness-enforcement work in a follow-up plan.

## Validation log

- [x] `devenv shell cargo test -p monochange publish_readiness --lib`
- [x] `devenv shell cargo test -p monochange cli_help::tests::render_command_help_for_publish_readiness --lib`
- [x] `devenv shell cargo check -p monochange`
- [x] `devenv shell cargo fmt --check`
- [x] `devenv shell dprint check ...`
- [x] `git diff --check`
- [x] `devenv shell cargo test -p monochange --test cli_help`
- [x] `devenv shell lint:test`
- [x] `devenv shell coverage:all`
- [x] `devenv shell coverage:patch` (`PATCH_COVERAGE 419/419 (100.00%)`)

## Decisions

- `publish-readiness` is intentionally top-level instead of hidden inside `mc publish`.
- The first implementation reuses existing publish dry-run checks to avoid duplicating registry logic prematurely.
- The artifact is JSON when written with `--output`, while display output defaults to Markdown for CI summaries.
- `already_published` is non-blocking so retries can skip consistently published versions.
- `unsupported` is blocking in the initial global status calculation.

## Follow-up roadmap

- [ ] Add readiness artifact fingerprinting for commit, release ref, selected packages, config, manifests, lockfiles, and schema version.
- [ ] Add `mc publish --readiness <PATH>` and reject missing, blocked, stale, or mismatched readiness.
- [ ] Add optional readiness consumption to `mc publish-plan`.
- [ ] Expand Cargo readiness semantics first.
- [ ] Expand npm readiness semantics second.
- [ ] Add `mc publish-bootstrap` for first-time package setup.
- [ ] Design retry/resume around explicit readiness for remaining work.
