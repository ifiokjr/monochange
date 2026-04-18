# Plans

Use `docs/plans/` to keep multi-step work, implementation notes, and follow-up cleanup in the repository where both humans and agents can find them.

## Layout

- `docs/plans/active/` — current plans with status, decisions, and next steps
- `docs/plans/completed/` — archived plans that reflect shipped work
- `docs/plans/tech-debt.md` — recurring cleanup targets, deferred follow-ups, and quality notes

## Workflow

1. Create or update a plan in `docs/plans/active/` before starting complex work.
2. Keep the plan small, concrete, and tied to file paths, commands, and acceptance checks.
3. Record decision notes in the plan instead of leaving them only in chat or PR comments.
4. When the work lands, move the plan to `docs/plans/completed/` or fold the remaining items into `docs/plans/tech-debt.md`.

## Recommended plan shape

A good plan usually includes:

- problem statement
- scope and non-goals
- affected files or crates
- ordered checklist of work items
- validation commands
- follow-up risks or cleanup notes
