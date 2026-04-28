# Publish readiness, planning, and bootstrap

## Status

- Previous slices shipped `mc publish-readiness` (PR #292), readiness artifact enforcement in `mc publish` (PR #301), Cargo-first publish-readiness blockers (PR #303), and `mc publish-plan --readiness <path>` (PR #305).
- Current branch: `feat/publish-bootstrap-command`.
- Current slice: add `mc publish-bootstrap --from <ref> --output <path>` so first-time placeholder package setup is release-record scoped and leaves a JSON result artifact.

## Problem

`mc placeholder-publish` can reserve package names, but it is not tied to a durable release record and does not write an artifact that explains which release package set was bootstrapped. The recommended package publishing lifecycle now uses readiness artifacts as a boundary before real registry mutation. First-time package setup needs the same explicit, CLI-first shape:

1. generate readiness for the release commit,
2. bootstrap missing packages if readiness identifies first-time registry setup,
3. rerun readiness,
4. plan/publish from the fresh readiness artifact.

## Scope

This slice adds a top-level `mc publish-bootstrap` command that:

- requires `--from <ref>` and discovers the release record from that ref,
- selects package ids from the release record, optionally intersected with repeated `--package <id>` filters,
- runs the existing placeholder publishing flow for the selected release package set,
- supports `--dry-run`, `--format text|markdown|json`, and `--output <path>`,
- writes a JSON artifact with `kind = "monochange.publishBootstrap"`, the resolved/release-record commits, selected package ids, and the placeholder publish report.

## Non-goals for this slice

- Retry/resume semantics for real package publishing.
- Readiness artifact validation of bootstrap result artifacts.
- Automated registry-side trusted-publisher enrollment.
- Replacing the lower-level `mc placeholder-publish` command.

## Affected files

- `crates/monochange/src/publish_bootstrap.rs`
- `crates/monochange/src/cli.rs`
- `crates/monochange/src/lib.rs`
- `crates/monochange/src/cli_help.rs`
- `crates/monochange/src/__tests.rs`
- `docs/src/guide/13-ci-and-publishing.md`
- `docs/src/readme.md`
- `readme.md`
- `.templates/project.t.md`
- `packages/monochange__skill/SKILL.md`
- `packages/monochange__skill/skills/commands.md`
- `packages/monochange__skill/skills/reference.md`
- `packages/monochange__skill/skills/trusted-publishing.md`
- `.changeset/publish-bootstrap-command.md`

## Checklist

- [x] Create isolated worktree and branch `feat/publish-bootstrap-command`.
- [x] Add release-record-scoped publish bootstrap command.
- [x] Add JSON bootstrap result artifact writing.
- [x] Add focused unit and CLI dispatch coverage.
- [x] Update user docs and packaged skill docs.
- [x] Run formatting and lint checks.
- [x] Run full validation.
- [ ] Run coverage and confirm 100% patch coverage after commit.
- [ ] Push branch and open PR.
- [ ] Merge after required checks pass.

## Validation log

- [x] `devenv shell cargo fmt`
- [x] `devenv shell cargo test -p monochange publish_bootstrap --lib`
- [x] `devenv shell cargo test -p monochange publish_bootstrap_dispatches_from_release_record_and_writes_artifact --lib`
- [x] `devenv shell cargo test -p monochange render_command_help_for_publish_bootstrap --lib`
- [x] `devenv shell cargo test -p monochange cli_help_returns_success_output --lib`
- [x] `devenv shell mc validate`
- [x] `devenv shell lint:test`
- [x] `devenv shell mdt check`
- [x] `devenv shell coverage:all`
- [x] `CI=false devenv shell build:all`
- [ ] `devenv shell coverage:patch` after commit

## Decisions

- Keep bootstrap separate from readiness. A bootstrap artifact records first-time setup work, but `mc publish` still requires a fresh readiness artifact before real package publishing.
- Scope bootstrap selection to the release record. Repeated `--package` filters intersect with release-record package ids instead of bootstrapping arbitrary workspace packages.
- Keep `mc placeholder-publish` available as the lower-level escape hatch for reserving names outside a release-scoped lifecycle.

## Follow-up roadmap

- [ ] Add deeper freshness checks for workspace config, manifests, lockfiles, and publish tooling inputs.
- [ ] Expand npm readiness semantics second.
- [x] Add `mc publish-bootstrap` for first-time package setup.
- [ ] Design retry/resume around explicit readiness for remaining work.
