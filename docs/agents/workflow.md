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
- Run the full local validation suite before opening a PR.
