# Documentation workflow

Shared documentation blocks live in `.templates/` and are synchronized with `mdt`.

- Treat `AGENTS.md` as the table of contents, not the full manual.
- Keep `ARCHITECTURE.md`, `docs/agents/*.md`, and `docs/plans/` as the repo-local system of record for agent-facing guidance.
- Edit provider blocks in `.templates/` when one change should update multiple docs.
- Run `docs:update` after changing shared docs or consumer blocks.
- Run `docs:check` before opening a PR to confirm shared blocks are synchronized and agent-facing documentation stays fresh.
- For complex or multi-step work, create or update a plan under `docs/plans/active/`, then move it to `docs/plans/completed/` when the work lands.
- Treat `docs/` as a product surface when behavior changes.
