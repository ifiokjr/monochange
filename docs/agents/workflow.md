# Workflow expectations

- Create a feature branch from `main`.
- Rebase onto `main` regularly while working so the branch does not fall behind and merge conflicts stay small.
- Always check the PR for merge conflicts before merging.
- Handle merge conflicts with a rebase onto `main` before merging, then rerun relevant validation.
- Only use squash merging.
- When creating or updating a PR, manage failing checks proactively and use the scheduler to keep monitoring follow-up CI work until it is green.
- For non-trivial behavior changes, start with a failing test.
- Implement the smallest change that makes the tests pass.
- After making changes, run `fix:all` so formatting and clippy autofixes are applied before final validation.
- Update docs, READMEs, fixtures, changeset examples, and templates when behavior changes.
- Always include a `.changeset/*.md` file when changing code in any published crate. Before opening a PR, run `mc affected --changed-paths <files>` (passing every changed file relative to the repo root) to confirm that attached changesets cover all affected packages. If `mc affected` reports uncovered packages, add a changeset with `mc change --package <id> --bump patch --reason "describe the change"`. The only exception is when changes are limited to paths already in `ignored_paths` (tests, snapshots, docs) — in that case the `no-changeset-required` label may be used instead.
- When adding or changing configuration options in any crate, update the annotations in `monochange.toml` to reflect the new option, its defaults, available values, and purpose. Use the existing comment style as a guide. Where possible, use mdt shared blocks in `.templates/` so the same documentation propagates to the guide, README, and config file.
- Run the full local validation suite before opening a PR.
- During review, explicitly check architecture boundaries for touched code: core defines contracts, adapter crates own implementation details, and `crates/monochange` only orchestrates.
- If touched code adds provider/ecosystem-specific validation or mutation logic outside an adapter crate, either move it behind adapter dispatch or document why that exception is unavoidable.
- For refactors that move implementation details across crate boundaries, add or update realistic fixtures and integration tests so touched-code coverage stays above 92%.
